// READ-ONLY SPECIFICATION CODEC. No ledger, signer, database or execution API.
// Binding/signature verification does not establish the truth of book equality.
import { createHash, createPublicKey, verify } from 'node:crypto';

const DOMAIN = Buffer.from('hawala-node-books-verification/v1\0', 'ascii');
export const FIELDS = Object.freeze([
  ['network_id', 32], ['installation_id', 32], ['authority_id', 32],
  ['scope_id', 32], ['legal_book_id', 32], ['authority_epoch', 8],
  ['source_sequence', 8], ['source_hash', 32], ['registry_digest', 32],
  ['compiler_digest', 32], ['projection_digest', 32], ['journal_digest', 32],
  ['accounting_state_digest', 32], ['cluster_id', 16], ['cluster_generation', 8],
  ['projector_fence_digest', 32], ['verifier_key', 32],
  ['verifier_mandate_digest', 32], ['verification_sequence', 8],
  ['previous_receipt_digest', 32],
].map(Object.freeze));
export const BODY_LENGTH = DOMAIN.length + FIELDS.reduce((n, [, size]) => n + size, 0);
export const ENVELOPE_LENGTH = BODY_LENGTH + 64;
const MAX64 = (1n << 64n) - 1n;
const MAX128 = (1n << 128n) - 1n;
const ZERO = Buffer.alloc(32);
const hash = bytes => createHash('sha256').update(bytes).digest();

function exactKeys(value, keys) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)
      || Object.getPrototypeOf(value) !== Object.prototype
      || Reflect.ownKeys(value).length !== keys.length
      || keys.some(k => !Object.hasOwn(value, k))) throw Error('record fields');
}
function integer(value, width, name) {
  if (typeof value !== 'bigint' || value < 0n || value >= 1n << BigInt(width * 8)) {
    throw Error(`integer range: ${name}`);
  }
  return Buffer.from(value.toString(16).padStart(width * 2, '0'), 'hex');
}
function bytes(value, width, name) {
  if (!Buffer.isBuffer(value) || value.length !== width) throw Error(`byte width: ${name}`);
  return value;
}
function validate(r) {
  exactKeys(r, FIELDS.map(([name]) => name));
  for (const [name, width] of FIELDS) {
    if (width === 32) {
      bytes(r[name], width, name);
      if (name !== 'previous_receipt_digest' && r[name].equals(ZERO)) {
        throw Error(`zero field: ${name}`);
      }
    } else integer(r[name], width, name);
  }
  for (const name of ['authority_epoch', 'cluster_generation', 'verification_sequence']) {
    if (r[name] === 0n) throw Error(`zero field: ${name}`);
  }
  if (r.cluster_id === 0n || r.cluster_id === MAX128) throw Error('reserved cluster ID');
  if ((r.verification_sequence === 1n) !== r.previous_receipt_digest.equals(ZERO)) {
    throw Error('receipt genesis link');
  }
}

export function signingBytes(r) {
  validate(r);
  return Buffer.concat([DOMAIN, ...FIELDS.map(([name, width]) =>
    width === 32 ? r[name] : integer(r[name], width, name))]);
}
export function envelope(r, signature) {
  return Buffer.concat([signingBytes(r), bytes(signature, 64, 'signature')]);
}
export function parseEnvelope(wire) {
  bytes(wire, ENVELOPE_LENGTH, 'envelope');
  if (!wire.subarray(0, DOMAIN.length).equals(DOMAIN)) throw Error('signing domain');
  let offset = DOMAIN.length;
  const record = {};
  for (const [name, width] of FIELDS) {
    const part = wire.subarray(offset, offset + width);
    record[name] = width === 32 ? Buffer.from(part) : BigInt(`0x${part.toString('hex')}`);
    offset += width;
  }
  validate(record);
  return { record, signature: Buffer.from(wire.subarray(offset)) };
}
export function receiptId(wire) {
  parseEnvelope(wire);
  return hash(wire);
}

// expected MUST come from independently pinned policy/history/accounting evidence.
// The return value is only a decoded, signature- and binding-checked assertion.
export function verifyReceiptBinding(wire, expected) {
  const { record, signature } = parseEnvelope(wire);
  const expectedBytes = signingBytes(expected);
  if (!wire.subarray(0, BODY_LENGTH).equals(expectedBytes)) throw Error('receipt binding');
  const publicKey = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), expected.verifier_key]),
    format: 'der', type: 'spki',
  });
  if (!verify(null, expectedBytes, publicKey, signature)) throw Error('receipt signature');
  return { record, id: hash(wire) };
}

export function verifySuccessorBinding(previousWire, previousExpected, wire, expected) {
  const previous = verifyReceiptBinding(previousWire, previousExpected);
  const current = verifyReceiptBinding(wire, expected);
  const p = previous.record, c = current.record;
  const stable = ['network_id', 'installation_id', 'authority_id', 'scope_id',
    'legal_book_id', 'authority_epoch', 'cluster_id', 'cluster_generation',
    'verifier_key', 'verifier_mandate_digest'];
  for (const field of stable) {
    const same = Buffer.isBuffer(p[field]) ? p[field].equals(c[field]) : p[field] === c[field];
    if (!same) throw Error(`chain context: ${field}`);
  }
  if (p.verification_sequence === MAX64 || c.verification_sequence !== p.verification_sequence + 1n
      || !c.previous_receipt_digest.equals(previous.id)
      || c.source_sequence < p.source_sequence) throw Error('receipt chain');
  if (c.source_sequence === p.source_sequence) {
    for (const field of ['source_hash', 'registry_digest', 'compiler_digest',
      'projection_digest', 'journal_digest', 'accounting_state_digest']) {
      if (!c[field].equals(p[field])) throw Error(`same-prefix conflict: ${field}`);
    }
  }
  return current;
}

// JSON is an interchange view, never a signing input. Numeric JSON values and
// noncanonical strings are refused before Buffer/BigInt conversion.
export function fromJson(value) {
  exactKeys(value, FIELDS.map(([name]) => name));
  const out = {};
  for (const [name, width] of FIELDS) {
    const raw = value[name];
    if (typeof raw !== 'string') throw Error(`JSON string required: ${name}`);
    if (width === 32) {
      if (!/^[0-9a-f]{64}$/.test(raw)) throw Error(`canonical hex required: ${name}`);
      out[name] = Buffer.from(raw, 'hex');
    } else {
      if (!/^(0|[1-9][0-9]*)$/.test(raw) || raw.length > (width === 16 ? 39 : 20)) {
        throw Error(`canonical decimal required: ${name}`);
      }
      out[name] = BigInt(raw);
    }
  }
  validate(out);
  return out;
}
