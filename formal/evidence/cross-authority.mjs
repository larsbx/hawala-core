// READ-ONLY SPECIFICATION CODEC. No network, journal, custody or mutation API.
import { createHash, createPublicKey, verify } from 'node:crypto';
export const MANIFEST_DOMAIN = Buffer.from('hawala-cross-authority-manifest/v1\0');
export const DOMAIN = Buffer.from('hawala-cross-authority-evidence/v1\0');
const SET_DOMAIN = Buffer.from('hawala-cross-authority-evidence-set/v1\0');
export const KINDS = Object.freeze({ Proposed: 1, PrepareRequest: 2, Prepared: 3,
  NoVote: 4, CommitDecision: 5, AbortDecision: 6, CommitAck: 7,
  ExecutionObserved: 8, Settled: 9, Held: 10, FenceAck: 11, Recovery: 12, AbortAck: 13, MandateAcceptance: 14, RecoveryLog: 15 });
// Every payload field is a 32-byte digest, except FenceAck's final u64.
const SCHEMA = Object.freeze({ 1: [], 2: [32], 3: [32], 4: [32], 5: [32], 6: [32],
  7: [32,32,32], 8: [32,32,32], 9: [32,32,32,32], 10: [32,32],
  11: [32,32,8], 12: [32,32,32,32], 13: [32,32], 14: [32], 15: [32,32,32] });
export const hash = bytes => createHash('sha256').update(bytes).digest();
export function fixed(value, length) {
  if (!Buffer.isBuffer(value) || value.length !== length) throw Error(`expected ${length} bytes`);
  return value;
}
export function uint(value, bytes = 8) {
  if (typeof value !== 'bigint' || value < 0n || value >= (1n << BigInt(8 * bytes))) throw Error('unsigned integer out of range');
  const out = Buffer.alloc(bytes);
  if (bytes === 8) out.writeBigUInt64BE(value); else if (bytes === 2) out.writeUInt16BE(Number(value));
  else throw Error('unsupported integer width');
  return out;
}
export function money(value) {
  if (typeof value !== 'bigint' || value < 0n || value > 0x7fff_ffff_ffff_ffffn) throw Error('amount out of i64 range');
  return uint(value);
}
function currency(value) {
  if (typeof value !== 'string' || !/^[A-Z]{3}$/.test(value)) throw Error('noncanonical currency');
  return Buffer.from(value, 'ascii');
}
export function manifest(m) {
  if (m.legs.length !== 2) throw Error('v1 is bilateral');
  const keys = m.legs.map(l => Buffer.concat([fixed(l.authority,32), fixed(l.scope,32)]));
  if (Buffer.compare(keys[0], keys[1]) >= 0 || m.legs[0].authority.equals(m.legs[1].authority)) throw Error('distinct authorities in canonical order required');
  if (m.legs[0].allocation.equals(m.legs[1].allocation)) throw Error('allocations must be distinct');
  return Buffer.concat([MANIFEST_DOMAIN, fixed(m.network,32), fixed(m.business,32), uint(m.revision),
    fixed(m.attempt,32), fixed(m.terms,32), fixed(m.consent,32), fixed(m.disclosure,32),
    fixed(m.finality,32), fixed(m.recovery,32), fixed(m.coordinator,32), uint(m.coordinatorEpoch),
    uint(2n,2), ...m.legs.flatMap(l => [fixed(l.authority,32), fixed(l.scope,32), uint(l.epoch),
      fixed(l.allocation,32), fixed(l.obligation,32), fixed(l.funding,32), fixed(l.payee,32),
      currency(l.currency), money(l.amount), money(l.fee), fixed(l.destination,32), fixed(l.custody,32)]),
    uint(m.prepareDeadline)]);
}
export function recoveryMandate(m) {
  if (m.successorEpoch !== m.coordinatorEpoch + 1n) throw Error('successor must advance epoch exactly once');
  if (m.alertAfter !== 300n || m.escalateAfter !== 3600n) throw Error('unsupported response profile');
  if (m.participants.length !== 2) throw Error('bilateral mandate required');
  const order = m.participants.map(p => Buffer.concat([fixed(p.authority,32),fixed(p.scope,32)]));
  if (Buffer.compare(order[0],order[1])>=0 || m.participants[0].authority.equals(m.participants[1].authority)) throw Error('noncanonical mandate participants');
  return Buffer.concat([Buffer.from('hawala-cross-authority-recovery-mandate/v1\0'),
    fixed(m.network,32), fixed(m.coordinator,32), uint(m.coordinatorEpoch),
    fixed(m.successor,32), uint(m.successorEpoch), fixed(m.resolver,32), fixed(m.fenceOperator,32),
    fixed(m.logAuthority,32), uint(2n,2), ...m.participants.flatMap(p=>[
      fixed(p.authority,32),fixed(p.scope,32),uint(p.epoch)]), uint(m.alertAfter), uint(m.escalateAfter)]);
}
export function signingBytes(e) {
  if (!Number.isInteger(e.kind)) throw Error('kind must be an integer');
  const widths = SCHEMA[e.kind];
  if (!widths || e.payload.length !== widths.length) throw Error('unknown kind or payload arity');
  return Buffer.concat([DOMAIN, Buffer.from([e.kind]), fixed(e.binding,32), fixed(e.issuer,32),
    uint(e.signerEpoch), uint(e.decisionEpoch), uint(e.serial),
    ...e.payload.map((v,i) => fixed(v,widths[i]))]);
}
export function envelope(e) { return Buffer.concat([signingBytes(e), fixed(e.signature,64)]); }
export const evidenceId = e => hash(envelope(e));
export function evidenceSet(ids) {
  if (!Array.isArray(ids) || ids.length > 65535) throw Error('bad evidence set');
  const sorted = ids.map(id => fixed(id,32)).toSorted(Buffer.compare);
  for (let i=1; i<sorted.length; i++) if (sorted[i].equals(sorted[i-1])) throw Error('duplicate evidence');
  return hash(Buffer.concat([SET_DOMAIN, uint(BigInt(sorted.length),2), ...sorted]));
}
export function parse(bytes) {
  fixed(bytes.subarray(0,DOMAIN.length),DOMAIN.length);
  if (!bytes.subarray(0,DOMAIN.length).equals(DOMAIN)) throw Error('wrong signing domain');
  let offset = DOMAIN.length;
  const kind = bytes[offset++];
  const widths = SCHEMA[kind];
  if (!widths) throw Error('unknown kind');
  const length = DOMAIN.length + 1 + 64 + 24 + widths.reduce((a,b)=>a+b,0) + 64;
  if (bytes.length !== length) throw Error('noncanonical length');
  const take = n => { const b = bytes.subarray(offset,offset+n); offset += n; return b; };
  const number = () => take(8).readBigUInt64BE();
  const e = { kind, binding: take(32), issuer: take(32), signerEpoch: number(),
    decisionEpoch: number(), serial: number(), payload: widths.map(take), signature: take(64) };
  if (!envelope(e).equals(bytes)) throw Error('noncanonical envelope');
  return e;
}
export function verifyEvidence(bytes, expected) {
  const e = parse(bytes);
  // Trust comes from independently pinned policy, NEVER the embedded issuer.
  for (const field of ['binding','issuer']) if (!e[field].equals(expected[field])) throw Error(`wrong ${field}`);
  for (const field of ['kind','signerEpoch','decisionEpoch']) if (e[field] !== expected[field]) throw Error(`wrong ${field}`);
  const key = createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100','hex'), expected.issuer]), format:'der', type:'spki' });
  if (!verify(null, signingBytes(e), key, e.signature)) throw Error('invalid signature');
  return e;
}
