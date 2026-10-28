// Golden vectors were independently encoded and signed with Python/PyCA.
// This checker never regenerates them and has no --write mode.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createHash, createPrivateKey, sign } from 'node:crypto';
import { FIELDS, BODY_LENGTH, ENVELOPE_LENGTH, signingBytes, envelope, parseEnvelope,
  receiptId, fromJson, verifyReceiptBinding, verifySuccessorBinding,
} from '../formal/evidence/node-books-verification.mjs';

const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const fixtureBytes = fs.readFileSync(new URL('../formal/evidence/node-books-verification-v1.json', import.meta.url));
assert.equal(sha(fixtureBytes), '318cb8a62fd8453ee5fb27942b96f47b0e82e97c7c505437b71da14cbc7b5b3b', 'independent fixture drift');
const fixture = JSON.parse(fixtureBytes);
const publicTestKey = createPrivateKey({ key: Buffer.concat([
  Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.alloc(32, 81),
]), format: 'der', type: 'pkcs8' });
let checks = 0;
const reject = (fn, reason = /record fields|byte width|zero field|integer range|reserved cluster ID|receipt genesis link|signing domain|receipt binding|receipt signature|receipt chain|chain context|same-prefix conflict|JSON string required|canonical hex required|canonical decimal required/, label) => {
  assert.throws(fn, reason, label); checks++;
};
const clone = r => Object.fromEntries(Object.entries(r).map(([k,v]) => [k, Buffer.isBuffer(v) ? Buffer.from(v) : v]));
const signed = r => envelope(r, sign(null, signingBytes(r), publicTestKey));
const records = fixture.records.map(v => fromJson(v.fields));
const wires = fixture.records.map(v => Buffer.from(v.envelope_hex, 'hex'));
assert.equal(BODY_LENGTH, fixture.body_length);
assert.equal(ENVELOPE_LENGTH, fixture.envelope_length);
assert.equal(records.length, 3);

for (let i = 0; i < records.length; i++) {
  const r = records[i], wire = wires[i], v = fixture.records[i];
  assert.equal(signingBytes(r).toString('hex'), v.signing_hex);
  assert.equal(envelope(r, Buffer.from(v.signature_hex, 'hex')).toString('hex'), v.envelope_hex);
  assert.equal(sha(wire), v.receipt_id_hex);
  assert.equal(receiptId(wire).toString('hex'), v.receipt_id_hex);
  assert.deepEqual(parseEnvelope(wire).record, r);
  assert.deepEqual(verifyReceiptBinding(wire, r).record, r);
  assert.equal(sign(null, signingBytes(r), publicTestKey).toString('hex'), v.signature_hex);
  checks += 7;
}
verifySuccessorBinding(wires[0], records[0], wires[1], records[1]); checks++;
verifySuccessorBinding(wires[1], records[1], wires[2], records[2]); checks++;

// Every signed/envelope byte is covered; byte mutation cannot survive both the
// independent expected-context check and Ed25519 verification.
for (let i = 0; i < wires[1].length; i++) {
  const bad = Buffer.from(wires[1]); bad[i] ^= 1;
  reject(() => verifyReceiptBinding(bad, records[1]), undefined, `tampered-byte-${i}`);
}
for (const [name, width] of FIELDS) {
  const wrongPin = clone(records[1]);
  if (width === 32) wrongPin[name][0] ^= 1;
  else wrongPin[name] += name === 'source_sequence' || name === 'cluster_id' ? -1n : 1n;
  reject(() => verifyReceiptBinding(wires[1], wrongPin), /binding|genesis/);
}
for (let size = 0; size < ENVELOPE_LENGTH; size++) {
  reject(() => parseEnvelope(wires[0].subarray(0, size)), /byte width/);
}
reject(() => parseEnvelope(Buffer.concat([wires[0], Buffer.from([0])])), /byte width/);
reject(() => verifyReceiptBinding(wires[0]), /record fields/);
reject(() => fromJson({ ...fixture.records[0].fields, extra: 'ignored' }), /record fields/);
const missing = { ...fixture.records[0].fields }; delete missing.scope_id;
reject(() => fromJson(missing), /record fields/);
for (const raw of [0, 1, 1.5, true, null, '-1', '+1', '01', ' 1', '1 ', '1e3', '1.0', '', '١']) {
  reject(() => fromJson({ ...fixture.records[0].fields, source_sequence: raw }));
}
for (const raw of ['a'.repeat(63), 'GG'.repeat(32), 'AA'.repeat(32), 'a'.repeat(65), 1]) {
  reject(() => fromJson({ ...fixture.records[0].fields, network_id: raw }));
}
for (const name of ['authority_epoch', 'cluster_generation', 'verification_sequence']) {
  const bad = clone(records[1]); bad[name] = 0n;
  reject(() => signingBytes(bad), /zero field/);
}
for (const name of ['authority_epoch', 'source_sequence', 'cluster_generation', 'verification_sequence']) {
  const bad = clone(records[1]); bad[name] = 1n << 64n;
  reject(() => signingBytes(bad), /integer range/);
}
for (const n of [0n, (1n << 128n) - 1n, 1n << 128n, -1n]) {
  const bad = clone(records[1]); bad.cluster_id = n;
  reject(() => signingBytes(bad), /cluster ID|integer range/);
}
for (const [name, width] of FIELDS.filter(([,width]) => width === 32)) {
  if (name === 'previous_receipt_digest') continue;
  const bad = clone(records[1]); bad[name] = Buffer.alloc(width);
  reject(() => signingBytes(bad), /zero field/);
}
// A different validly signed assertion is still rejected against the original
// pins. Chain checks use fresh matching pins to test continuity independently.
const wrongScope = clone(records[1]); wrongScope.scope_id[0] ^= 1;
reject(() => verifyReceiptBinding(signed(wrongScope), records[1]), /binding/);
reject(() => verifySuccessorBinding(wires[0], records[0], signed(wrongScope), wrongScope), /chain context/, 'scope-transplant');
const rollback = clone(records[2]); rollback.source_sequence = records[1].source_sequence - 1n;
reject(() => verifySuccessorBinding(wires[1], records[1], signed(rollback), rollback), /receipt chain/, 'source-rollback');
const wrongLink = clone(records[1]); wrongLink.previous_receipt_digest[0] ^= 1;
reject(() => verifySuccessorBinding(wires[0], records[0], signed(wrongLink), wrongLink), /receipt chain/, 'wrong-predecessor');
const skipped = clone(records[1]); skipped.verification_sequence = 3n;
reject(() => verifySuccessorBinding(wires[0], records[0], signed(skipped), skipped), /receipt chain/, 'skipped-receipt');
for (const field of ['source_hash', 'registry_digest', 'compiler_digest',
  'projection_digest', 'journal_digest', 'accounting_state_digest']) {
  const conflict = clone(records[2]); conflict[field][0] ^= 1;
  reject(() => verifySuccessorBinding(wires[1], records[1], signed(conflict), conflict), /same-prefix conflict/, `same-prefix-${field}`);
}
const changedCluster = clone(records[2]); changedCluster.cluster_generation += 1n;
reject(() => verifySuccessorBinding(wires[1], records[1], signed(changedCluster), changedCluster), /chain context/);
const zeroPrevious = clone(records[1]); zeroPrevious.previous_receipt_digest = Buffer.alloc(32);
reject(() => signingBytes(zeroPrevious), /genesis/);
const falseGenesis = clone(records[0]); falseGenesis.previous_receipt_digest = Buffer.alloc(32, 1);
reject(() => signingBytes(falseGenesis), /genesis/);
const allMax = clone(records[1]);
for (const name of ['authority_epoch', 'cluster_generation', 'verification_sequence']) {
  allMax[name] = (1n << 64n) - 1n;
}
const maxWire = signed(allMax);
verifyReceiptBinding(maxWire, allMax); checks++;
const exhausted = clone(allMax); exhausted.previous_receipt_digest = receiptId(maxWire);
reject(() => verifySuccessorBinding(maxWire, allMax, signed(exhausted), exhausted), /receipt chain/, 'exhausted-receipt-counter');
assert.equal(records[0].source_sequence, 0n);
assert.equal(records[1].cluster_id, (1n << 128n) - 2n);
assert.equal(records[1].source_sequence, (1n << 64n) - 1n);
assert.equal(records[2].source_sequence, records[1].source_sequence);
console.log(`Node books verification v1: ${checks} checks passed; three independent signed vectors; no book-truth or execution claim.`);
