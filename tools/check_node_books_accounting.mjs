// Golden byte vectors were independently encoded in Python with hand-paired legs.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { COMPILER_RULES, digest, compilerDigest, registryBytes, registryDigest,
  parseRegistry, verifyRegistryBinding, verifyRegistrySuccessor, journalBytes,
  parseJournal, compileJournal, projectionBytes, verifyProjection,
  chainGenesis, advanceJournalDigest, advanceProjectionDigest,
} from '../formal/evidence/node-books-accounting.mjs';
const raw = fs.readFileSync(new URL('../formal/evidence/node-books-accounting-v1.json', import.meta.url));
assert.equal(digest(raw).toString('hex'), '7f68830dcc7fc2575489e67c72766203ddfcf2cd5ac57d6987a491d9fe78eb97', 'independent accounting fixture drift');
const f = JSON.parse(raw), wire = Buffer.from(f.registry_hex, 'hex');
const reg = parseRegistry(wire), firstCase = f.cases[0];
const j = parseJournal(reg, Buffer.from(firstCase.journal_hex, 'hex'));
const clone = x => {
  if (Buffer.isBuffer(x)) return Buffer.from(x);
  if (Array.isArray(x)) return x.map(clone);
  if (x !== null && typeof x === 'object') return Object.fromEntries(Object.entries(x).map(([k,v]) => [k, clone(v)]));
  return x;
};
let checks = 0;
const reject = (fn, label) => {
  assert.throws(fn, /fields|bytes|zero digest|integer|reserved ID|count|epoch|registry genesis|ledger registry|duplicate asset|account registry|wire bounds|domain|truncated|side|trailing bytes|noncanonical|registry binding|registry succession|changed ledger|changed account|journal binding|posting|unbalanced ledger|transfer count|projection mismatch|chain kind/, label);
  checks++;
};
const b32 = n => Buffer.from(BigInt(n).toString(16).padStart(64, '0'), 'hex');
const MAX128 = (1n << 128n) - 1n;
assert.equal(registryBytes(reg).toString('hex'), f.registry_hex); checks++;
assert.equal(registryDigest(reg).toString('hex'), f.registry_digest_hex); checks++;
verifyRegistryBinding(wire, reg.context, registryDigest(reg)); checks++;
assert.equal(Buffer.from(COMPILER_RULES, 'ascii').toString('hex'), f.compiler_rules_hex); checks++;
assert.equal(compilerDigest().toString('hex'), f.compiler_digest_hex); checks++;
let jc = chainGenesis('journal', reg.context), pc = chainGenesis('projection', reg.context);
assert.equal(jc.toString('hex'), f.journal_genesis_hex); checks++;
assert.equal(pc.toString('hex'), f.projection_genesis_hex); checks++;
for (const c of f.cases) {
  const input = parseJournal(reg, Buffer.from(c.journal_hex, 'hex'));
  const first = BigInt(c.first_transfer_id), event = BigInt(c.event_reference);
  const plan = compileJournal(reg, input, first, event);
  const json = plan.map(t => Object.fromEntries(Object.entries(t).map(([k,v]) => [k, typeof v === 'bigint' ? v.toString() : v])));
  assert.deepEqual(json, c.transfers, 'golden-transfer-plan'); checks++;
  assert.equal(journalBytes(reg, input).toString('hex'), c.journal_hex); checks++;
  assert.equal(digest(journalBytes(reg, input)).toString('hex'), c.journal_digest_hex); checks++;
  const projection = projectionBytes(reg, input, first, event);
  assert.equal(projection.toString('hex'), c.projection_hex); checks++;
  assert.equal(digest(projection).toString('hex'), c.projection_digest_hex); checks++;
  assert.deepEqual(verifyProjection(reg, input, first, event, projection), plan); checks++;
  const reversed = clone(input); reversed.postings.reverse();
  assert(journalBytes(reg, reversed).equals(journalBytes(reg, input))); checks++;
  jc = advanceJournalDigest(jc, reg, input);
  pc = advanceProjectionDigest(pc, reg, input, first, event);
  assert.equal(jc.toString('hex'), c.journal_chain_hex); checks++;
  assert.equal(pc.toString('hex'), c.projection_chain_hex); checks++;
}

for (let i = 0; i < wire.length; i++) {
  const bad = Buffer.from(wire); bad[i] ^= 1;
  reject(() => verifyRegistryBinding(bad, reg.context, registryDigest(reg)), `registry-byte-${i}`);
}
const projection = Buffer.from(firstCase.projection_hex, 'hex');
for (let i = 0; i < projection.length; i++) {
  const bad = Buffer.from(projection); bad[i] ^= 1;
  reject(() => verifyProjection(reg, j, 1000n, 77n, bad), `projection-byte-${i}`);
}
for (const bytes of [wire.subarray(0, -1), Buffer.concat([wire, Buffer.from([0])])]) reject(() => parseRegistry(bytes));
const jwire = journalBytes(reg, j);
for (let n = 0; n < jwire.length; n++) reject(() => parseJournal(reg, jwire.subarray(0, n)), `journal-truncation-${n}`);
reject(() => parseJournal(reg, Buffer.concat([jwire, Buffer.from([0])])));
const unsortedJournal = Buffer.from(jwire), rowStart = jwire.length - j.postings.length * 41;
const firstRow = Buffer.from(unsortedJournal.subarray(rowStart, rowStart + 41));
unsortedJournal.copy(unsortedJournal, rowStart, rowStart + 41, rowStart + 82);
firstRow.copy(unsortedJournal, rowStart + 41);
reject(() => parseJournal(reg, unsortedJournal), 'noncanonical-posting-order');

let invalidRegistry = 0;
for (const mutate of [
  r => r.ledgers.reverse(),
  r => r.accounts.reverse(),
  r => { r.accounts[1].tb_id = r.accounts[0].tb_id; },
  r => { r.accounts[1].id = r.accounts[0].id; },
  r => { r.accounts[0].ledger = 10n; },
  r => { r.accounts[0].normal = 'ignored'; },
  r => { r.ledgers[0].scale = 19n; },
  r => { r.ledgers[0].currency = 'eur'; },
  r => { r.ledgers[1].currency = 'EUR'; },
  r => { r.accounts[0].tb_id = MAX128; },
  r => { r.accounts[0].tb_id = 0n; },
  r => { r.context.authority_epoch = 0n; },
  r => { r.extra = 1; },
]) { const bad = clone(reg); mutate(bad); reject(() => registryBytes(bad), `registry-invalid-${invalidRegistry++}`); }
const wrongContext = clone(reg.context); wrongContext.scope_id[0] ^= 1;
reject(() => verifyRegistryBinding(wire, wrongContext, registryDigest(reg)));
const next = clone(reg); next.revision = 2n; next.previous_registry_digest = registryDigest(reg);
next.accounts.push({ id: b32(9), tb_id: 509n, ledger: 9n, normal: 'credit' });
verifyRegistrySuccessor(reg, next); checks++;
for (const mutate of [
  r => { r.previous_registry_digest[0] ^= 1; },
  r => { r.context.scope_id[0] ^= 1; },
  r => { r.revision = 3n; },
  r => { r.accounts.shift(); },
  r => { r.accounts[0].tb_id = 900n; },
  r => { r.accounts[0].normal = 'credit'; },
  r => { r.ledgers[0].scale = 3n; },
]) { const bad = clone(next); mutate(bad); reject(() => verifyRegistrySuccessor(reg, bad), 'registry-immutable-history'); }

let invalidJournal = 0;
for (const mutate of [
  x => { x.context.authority_id[0] ^= 1; },
  x => { x.registry_digest[0] ^= 1; },
  x => { x.source_sequence = 0n; },
  x => { x.postings[0].amount = 0n; },
  x => { x.postings[0].amount = -1n; },
  x => { x.postings[0].amount = 1n << 63n; },
  x => { x.postings[0].amount = 1; },
  x => { x.postings[0].account = b32(999); },
  x => { x.postings[1].account = x.postings[0].account; },
  x => { x.postings[0].side = 'both'; },
  x => { x.postings[0].amount += 1n; },
  x => { x.postings[0].extra = 1; },
]) { const bad = clone(j); mutate(bad); reject(() => compileJournal(reg, bad, 1000n, 77n), `journal-invalid-${invalidJournal++}`); }
// Total debit equals total credit across currencies, while each currency is wrong.
const cross = clone(j); cross.postings[0].amount += 1n; cross.postings[6].amount += 1n;
reject(() => compileJournal(reg, cross, 1000n, 77n), 'cross-currency-netting');
for (const n of [0n, -1n, MAX128, MAX128 + 1n]) {
  reject(() => compileJournal(reg, j, n, 77n));
  reject(() => compileJournal(reg, j, 1000n, n));
}
reject(() => compileJournal(reg, j, MAX128 - 1n, 77n), 'transfer-id-exhaustion');

// Independent per-account delta oracle and synthetic nonnegative starting books.
function checkDeltas(r, input, first = 1000n) {
  const plan = compileJournal(r, input, first, 77n);
  const expected = new Map(r.accounts.map(a => [a.tb_id, 0n]));
  for (const p of input.postings) {
    const a = r.accounts.find(a => a.id.equals(p.account));
    expected.set(a.tb_id, expected.get(a.tb_id) + (p.side === 'debit' ? p.amount : -p.amount));
  }
  const actual = new Map(r.accounts.map(a => [a.tb_id, 0n]));
  const natural = new Map(r.accounts.map(a => {
    const delta = expected.get(a.tb_id) * (a.normal === 'debit' ? 1n : -1n);
    return [a.tb_id, delta < 0n ? -delta : 0n];
  }));
  for (const t of plan) {
    for (const [id, change] of [[t.debit_account_id, t.amount], [t.credit_account_id, -t.amount]]) {
      actual.set(id, actual.get(id) + change);
      const a = r.accounts.find(a => a.tb_id === id);
      natural.set(id, natural.get(id) + change * (a.normal === 'debit' ? 1n : -1n));
      assert(natural.get(id) >= 0n && natural.get(id) <= (1n << 63n) - 1n);
    }
  }
  assert.deepEqual(actual, expected); checks++;
  assert(plan.every((t, i) => t.linked === (i + 1 < plan.length)), 'one linked event'); checks++;
}
for (let n = 1; n <= 200; n++) {
  const input = clone(j);
  for (const [offset, total] of [[0, 1000n], [4, 700n]]) {
    const left = BigInt(n) + 1n, right = BigInt((n * 7) % 600) + 1n;
    const amounts = [left, total - left, right, total - right];
    amounts.forEach((amount, i) => { input.postings[offset + i].amount = amount; });
  }
  checkDeltas(reg, input);
}
const bigReg = clone(reg); bigReg.ledgers = [reg.ledgers[1]];
bigReg.accounts = Array.from({length:256}, (_,i) => ({ id:b32(i+1), tb_id:BigInt(i+501), ledger:9n, normal:i%2 ? 'credit' : 'debit' }));
const big = clone(j); big.registry_digest = registryDigest(bigReg);
big.postings = bigReg.accounts.map((a,i) => ({ account:a.id, side:i<128 ? 'debit' : 'credit',
  amount:i<128 ? 2n : i===128 ? 1n : i===255 ? 3n : 2n }));
assert.equal(compileJournal(bigReg, big, MAX128 - 255n, 77n).length, 255); checks++;
checkDeltas(bigReg, big, MAX128 - 255n);
reject(() => compileJournal(bigReg, big, MAX128 - 254n, 77n), 'maximum-chain-id-exhaustion');
const tooMany = clone(big); tooMany.postings.push(clone(tooMany.postings[0]));
reject(() => compileJournal(bigReg, tooMany, 1000n, 77n));
console.log(`Node books accounting v1: ${checks} counted checks passed; 200 differential journals and maximum 255-leg chain; no database or admission claim.`);
