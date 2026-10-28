// PURE SPECIFICATION TOOLING. No database, signing, admission or execution API.
import { createHash } from 'node:crypto';

const domain = name => Buffer.from(`hawala-node-books-${name}/v1\0`, 'ascii');
const ZERO = Buffer.alloc(32);
const MAX128 = (1n << 128n) - 1n;
const MAX_MONEY = (1n << 63n) - 1n;
const CONTEXT = ['network_id', 'installation_id', 'authority_id', 'scope_id', 'legal_book_id'];
export const COMPILER_RULES = 'hawala-node-books-compiler/v1\n'
  + 'sort-ledger-u32;sort-account-b32;pair-min;positive-i64;max-postings-256;'
  + 'ids-first-plus-index;code-1;event-reference-u128;linked-all-except-last;no-pending\n';
export const digest = bytes => createHash('sha256').update(bytes).digest();
export const compilerDigest = () => digest(Buffer.from(COMPILER_RULES, 'ascii'));

function keys(x, names) {
  if (!x || Object.getPrototypeOf(x) !== Object.prototype
      || Reflect.ownKeys(x).length !== names.length
      || names.some(n => !Object.hasOwn(x, n))) throw Error('fields');
}
function fixed(x, n) {
  if (!Buffer.isBuffer(x) || x.length !== n) throw Error('bytes');
  return x;
}
function b32(x, zero = false) {
  fixed(x, 32);
  if (!zero && x.equals(ZERO)) throw Error('zero digest');
  return x;
}
function uint(x, n) {
  if (typeof x !== 'bigint' || x < 0n || x >= (1n << BigInt(8 * n))) throw Error('integer');
  return Buffer.from(x.toString(16).padStart(n * 2, '0'), 'hex');
}
function id128(x) {
  const b = uint(x, 16);
  if (x === 0n || x === MAX128) throw Error('reserved ID');
  return b;
}
function count(x, min, max) {
  if (!Array.isArray(x) || x.length < min || x.length > max) throw Error('count');
  return uint(BigInt(x.length), 2);
}
function contextBytes(c) {
  keys(c, [...CONTEXT, 'authority_epoch']);
  const epoch = uint(c.authority_epoch, 8);
  if (c.authority_epoch === 0n) throw Error('epoch');
  return Buffer.concat([...CONTEXT.map(n => b32(c[n])), epoch]);
}
const side = name => {
  if (name !== 'debit' && name !== 'credit') throw Error('side');
  return Buffer.from([name === 'debit' ? 1 : 2]);
};
const cmp = (a, b) => a < b ? -1 : a > b ? 1 : 0;

export function registryBytes(r) {
  keys(r, ['context', 'revision', 'previous_registry_digest', 'ledgers', 'accounts']);
  const revision = uint(r.revision, 8), previous = b32(r.previous_registry_digest, true);
  if (r.revision === 0n || (r.revision === 1n) !== previous.equals(ZERO)) throw Error('registry genesis');
  const lc = count(r.ledgers, 1, 65535), ac = count(r.accounts, 2, 65535);
  const ledgers = new Set(), assets = new Set();
  let lastLedger = 0n;
  const ledgerRows = r.ledgers.map(l => {
    keys(l, ['id', 'currency', 'scale']);
    const id = uint(l.id, 4), scale = uint(l.scale, 1);
    if (l.id <= lastLedger || l.scale > 18n || typeof l.currency !== 'string'
        || !/^[A-Z]{3}$/.test(l.currency)) throw Error('ledger registry');
    const asset = `${l.currency}/${l.scale}`;
    if (assets.has(asset)) throw Error('duplicate asset');
    lastLedger = l.id; assets.add(asset); ledgers.add(l.id);
    return Buffer.concat([id, Buffer.from(l.currency, 'ascii'), scale]);
  });
  let lastAccount = ZERO;
  const ids = new Set();
  const accountRows = r.accounts.map(a => {
    keys(a, ['id', 'tb_id', 'ledger', 'normal']);
    const id = b32(a.id), numeric = id128(a.tb_id), ledger = uint(a.ledger, 4);
    if (Buffer.compare(lastAccount, id) >= 0 || ids.has(a.tb_id)
        || !ledgers.has(a.ledger)) throw Error('account registry');
    lastAccount = id; ids.add(a.tb_id);
    return Buffer.concat([id, numeric, ledger, side(a.normal)]);
  });
  return Buffer.concat([domain('registry'), contextBytes(r.context), revision, previous,
    lc, ...ledgerRows, ac, ...accountRows]);
}
export const registryDigest = r => digest(registryBytes(r));

class Reader {
  constructor(wire, name, max) {
    if (!Buffer.isBuffer(wire) || wire.length > max) throw Error('wire bounds');
    this.wire = wire; this.offset = 0;
    if (!this.take(domain(name).length).equals(domain(name))) throw Error('domain');
  }
  take(n) {
    if (n > this.wire.length - this.offset) throw Error('truncated');
    const out = Buffer.from(this.wire.subarray(this.offset, this.offset + n));
    this.offset += n; return out;
  }
  integer(n) { return BigInt(`0x${this.take(n).toString('hex')}`); }
  context() { return Object.fromEntries([...CONTEXT.map(n => [n, this.take(32)]), ['authority_epoch', this.integer(8)]]); }
  rows(min, width, read) {
    const n = Number(this.integer(2));
    if (n < min || n * width > this.wire.length - this.offset) throw Error('count bounds');
    return Array.from({ length: n }, read);
  }
  finish() { if (this.offset !== this.wire.length) throw Error('trailing bytes'); }
}
const readSide = r => {
  const n = r.integer(1);
  if (n !== 1n && n !== 2n) throw Error('side');
  return n === 1n ? 'debit' : 'credit';
};
export function parseRegistry(wire) {
  const r = new Reader(wire, 'registry', 4000000);
  const out = { context: r.context(), revision: r.integer(8), previous_registry_digest: r.take(32) };
  out.ledgers = r.rows(1, 8, () => ({ id: r.integer(4), currency: r.take(3).toString('latin1'), scale: r.integer(1) }));
  out.accounts = r.rows(2, 53, () => ({ id: r.take(32), tb_id: r.integer(16), ledger: r.integer(4), normal: readSide(r) }));
  r.finish();
  if (!registryBytes(out).equals(wire)) throw Error('noncanonical registry');
  return out;
}
export function verifyRegistryBinding(wire, expectedContext, expectedDigest) {
  const r = parseRegistry(wire);
  if (!contextBytes(r.context).equals(contextBytes(expectedContext))
      || !digest(wire).equals(b32(expectedDigest))) throw Error('registry binding');
  return r;
}
export function verifyRegistrySuccessor(previous, next) {
  registryBytes(previous); registryBytes(next);
  if (!contextBytes(previous.context).equals(contextBytes(next.context))
      || next.revision !== previous.revision + 1n
      || !next.previous_registry_digest.equals(registryDigest(previous))) throw Error('registry succession');
  const ledgers = new Map(next.ledgers.map(l => [l.id, l]));
  const accounts = new Map(next.accounts.map(a => [a.id.toString('hex'), a]));
  for (const l of previous.ledgers) {
    const n = ledgers.get(l.id);
    if (!n || n.currency !== l.currency || n.scale !== l.scale) throw Error('changed ledger');
  }
  for (const a of previous.accounts) {
    const n = accounts.get(a.id.toString('hex'));
    if (!n || n.tb_id !== a.tb_id || n.ledger !== a.ledger || n.normal !== a.normal) throw Error('changed account');
  }
  return registryDigest(next);
}

function validatedJournal(registry, j) {
  registryBytes(registry);
  keys(j, ['context', 'registry_digest', 'source_sequence', 'source_hash', 'event_id', 'postings']);
  uint(j.source_sequence, 8);
  if (j.source_sequence === 0n || !contextBytes(j.context).equals(contextBytes(registry.context))
      || !b32(j.registry_digest).equals(registryDigest(registry))) throw Error('journal binding');
  b32(j.source_hash); b32(j.event_id); count(j.postings, 2, 256);
  const accounts = new Map(registry.accounts.map(a => [a.id.toString('hex'), a]));
  const seen = new Set(), totals = new Map();
  const posts = j.postings.map(p => {
    keys(p, ['account', 'side', 'amount']); b32(p.account); side(p.side); uint(p.amount, 8);
    const key = p.account.toString('hex'), a = accounts.get(key);
    if (!a || seen.has(key) || p.amount === 0n || p.amount > MAX_MONEY) throw Error('posting');
    seen.add(key);
    const t = totals.get(a.ledger) ?? [0n, 0n];
    t[p.side === 'debit' ? 0 : 1] += p.amount; totals.set(a.ledger, t);
    return { ...p, mapped: a };
  }).toSorted((a, b) => Buffer.compare(a.account, b.account));
  for (const [d, c] of totals.values()) if (d !== c) throw Error('unbalanced ledger');
  return posts;
}
export function journalBytes(registry, j) {
  const posts = validatedJournal(registry, j);
  return Buffer.concat([domain('journal'), contextBytes(j.context), j.registry_digest,
    uint(j.source_sequence, 8), j.source_hash, j.event_id, uint(BigInt(posts.length), 2),
    ...posts.map(p => Buffer.concat([p.account, side(p.side), uint(p.amount, 8)]))]);
}
export function parseJournal(registry, wire) {
  const r = new Reader(wire, 'journal', 11000);
  const j = { context: r.context(), registry_digest: r.take(32), source_sequence: r.integer(8),
    source_hash: r.take(32), event_id: r.take(32) };
  j.postings = r.rows(2, 41, () => ({ account: r.take(32), side: readSide(r), amount: r.integer(8) }));
  r.finish();
  if (!journalBytes(registry, j).equals(wire)) throw Error('noncanonical journal');
  return j;
}

export function compileJournal(registry, j, firstTransferId, eventReference) {
  const posts = validatedJournal(registry, j);
  id128(firstTransferId); id128(eventReference);
  const ledgers = [...new Set(posts.map(p => p.mapped.ledger))].toSorted(cmp);
  const transfers = [];
  for (const ledger of ledgers) {
    const debits = posts.filter(p => p.mapped.ledger === ledger && p.side === 'debit');
    const credits = posts.filter(p => p.mapped.ledger === ledger && p.side === 'credit');
    let di = 0, ci = 0, dr = debits[0].amount, cr = credits[0].amount;
    while (di < debits.length && ci < credits.length) {
      const amount = dr < cr ? dr : cr;
      const id = firstTransferId + BigInt(transfers.length); id128(id);
      transfers.push({ id, ledger, debit_account_id: debits[di].mapped.tb_id,
        credit_account_id: credits[ci].mapped.tb_id, amount, code: 1n,
        user_data_128: eventReference, linked: true });
      dr -= amount; cr -= amount;
      if (dr === 0n) { di++; if (di < debits.length) dr = debits[di].amount; }
      if (cr === 0n) { ci++; if (ci < credits.length) cr = credits[ci].amount; }
    }
  }
  if (transfers.length < 1 || transfers.length > 255) throw Error('transfer count');
  transfers.at(-1).linked = false;
  return transfers;
}
export function projectionBytes(registry, j, firstTransferId, eventReference) {
  const transfers = compileJournal(registry, j, firstTransferId, eventReference);
  return Buffer.concat([domain('projection'), digest(journalBytes(registry, j)), compilerDigest(),
    id128(eventReference), uint(BigInt(transfers.length), 2),
    ...transfers.map(t => Buffer.concat([id128(t.id), uint(t.ledger, 4), id128(t.debit_account_id),
      id128(t.credit_account_id), uint(t.amount, 8), uint(t.code, 2), Buffer.from([t.linked ? 1 : 0])]))]);
}
export function verifyProjection(registry, j, firstTransferId, eventReference, wire) {
  const expected = projectionBytes(registry, j, firstTransferId, eventReference);
  if (!Buffer.isBuffer(wire) || !wire.equals(expected)) throw Error('projection mismatch');
  return compileJournal(registry, j, firstTransferId, eventReference);
}
export function chainGenesis(kind, context) {
  if (kind !== 'journal' && kind !== 'projection') throw Error('chain kind');
  return digest(Buffer.concat([domain(`${kind}-chain-genesis`), contextBytes(context)]));
}
function chainStep(kind, previous, validatedBytes) {
  return digest(Buffer.concat([domain(`${kind}-chain-step`), b32(previous), digest(validatedBytes)]));
}
export const advanceJournalDigest = (previous, registry, j) =>
  chainStep('journal', previous, journalBytes(registry, j));
export const advanceProjectionDigest = (previous, registry, j, firstTransferId, eventReference) =>
  chainStep('projection', previous, projectionBytes(registry, j, firstTransferId, eventReference));
