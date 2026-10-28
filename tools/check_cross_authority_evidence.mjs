// Deterministic, public TEST seeds. These keys have no operational mandate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createPrivateKey, createPublicKey, sign } from 'node:crypto';
import { hash, uint, money, manifest, recoveryMandate, DOMAIN, KINDS, signingBytes, envelope, evidenceId, evidenceSet, parse, verifyEvidence } from '../formal/evidence/cross-authority.mjs';
const root = new URL('../', import.meta.url);
const label = s => hash(Buffer.from(s));
const key = n => {
  const privateKey = createPrivateKey({ key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.alloc(32,n)]), format:'der', type:'pkcs8' });
  return { privateKey, publicKey: createPublicKey(privateKey).export({format:'der',type:'spki'}).subarray(-32) };
};
const devices = [key(1),key(2)], coord = key(3);
const legs = [4,5].map(n=>({ signer:key(n), executor:key(n+2), settler:key(n+4) })).toSorted((a,b)=>Buffer.compare(a.signer.publicKey,b.signer.publicKey));
const m = { network:label('TEST network'), business:label('TEST order'), revision:1n,
  attempt:label('TEST attempt 1'), terms:label('accepted exact TEST terms'), consent:label('TEST dual signatures'),
  disclosure:label('TEST disclosure'), finality:label('TEST pinned finality policy'), recovery:label('TEST recovery mandate'),
  coordinator:coord.publicKey, coordinatorEpoch:7n, prepareDeadline:2000000000n,
  legs:legs.map((l,i)=>({authority:l.signer.publicKey,scope:label(`scope${i}`),epoch:3n,
    allocation:label(`allocation${i}`),obligation:label(`obligation${i}`),funding:label(`funding${i}`),
    payee:devices[i].publicKey,currency:'TST',amount:i===0?0n:0x7fff_ffff_ffff_ffffn,fee:0n,
    destination:label(`destination${i}`),custody:label(`TEST custody${i}`)})) };
const mandate = {network:m.network,coordinator:m.coordinator,coordinatorEpoch:7n,
  successor:key(11).publicKey,successorEpoch:8n,resolver:key(12).publicKey,
  fenceOperator:key(13).publicKey,logAuthority:key(14).publicKey,
  participants:m.legs.map(({authority,scope,epoch})=>({authority,scope,epoch})),
  alertAfter:300n,escalateAfter:3600n};
m.recovery=hash(recoveryMandate(mandate));
const binding = hash(manifest(m));
let serial=0n;
const make = (kind,k,payload=[],epoch=3n) => {
  const e = {kind:KINDS[kind],binding,issuer:k.publicKey,signerEpoch:epoch,decisionEpoch:7n,serial:serial++,payload};
  return {...e,signature:sign(null,signingBytes(e),k.privateKey)};
};
const proposed = devices.map(k=>make('Proposed',k,[],0n));
const request = make('PrepareRequest',coord,[m.recovery],7n);
const prepared = legs.map((l,i)=>make('Prepared',l.signer,[label(`durable TEST reservation${i}`)]));
const decision = make('CommitDecision',coord,[evidenceSet(prepared.map(evidenceId))],7n);
const acks = legs.map((l,i)=>make('CommitAck',l.signer,[evidenceId(decision),label(`TEST posting${i}`),prepared[i].payload[0]]));
const observed = legs.map((l,i)=>make('ExecutionObserved',l.executor,[evidenceId(acks[i]),label(`TEST execution${i}`),label(`TEST observation${i}`)]));
const settled = legs.map((l,i)=>make('Settled',l.settler,[evidenceId(acks[i]),evidenceId(observed[i]),m.finality,label(`TEST finality evidence${i}`)]));
const other = [make('NoVote',legs[0].signer,[label('refusal')]),
  make('AbortDecision',coord,[label('abort reason')],7n),
  make('Held',legs[0].signer,[evidenceId(acks[0]),label('uncertainty')]),
  make('FenceAck',legs[0].signer,[m.recovery,label('complete history'),uint(8n)]),
  make('Recovery',coord,[m.recovery,label('all fence ACKs'),label('complete history'),evidenceId(decision)],8n),
  make('AbortAck',legs[0].signer,[label('different TEST abort'),prepared[0].payload[0]]),
  make('MandateAcceptance',legs[0].signer,[m.recovery]),
  make('RecoveryLog',coord,[label('complete history'),evidenceId(decision),label('TEST checkpoint')],7n)];
const all = [...proposed,request,...prepared,decision,...acks,...observed,...settled,...other];
const fixture = { profile:'TEST-ONLY cross-authority evidence v1; no custody/finality authorization',
  recovery_mandate_hex:recoveryMandate(mandate).toString('hex'),
  manifest_hex:manifest(m).toString('hex'),binding_hex:binding.toString('hex'),
  money:[0n,0x7fff_ffff_ffff_ffffn].map(n=>({decimal:n.toString(),hex:money(n).toString('hex')})),
  records:all.map(e=>({kind:Object.keys(KINDS).find(k=>KINDS[k]===e.kind),
    signing_hex:signingBytes(e).toString('hex'),envelope_hex:envelope(e).toString('hex'),id_hex:evidenceId(e).toString('hex')})) };
const path = new URL('formal/evidence/cross-authority-v1.json',root);
if (process.argv.includes('--write')) fs.writeFileSync(path,JSON.stringify(fixture,null,2)+'\n');
assert.deepEqual(JSON.parse(fs.readFileSync(path)),fixture,'fixture drift: independently review before --write');
const expected = e => ({kind:e.kind,binding,issuer:e.issuer,signerEpoch:e.signerEpoch,decisionEpoch:e.decisionEpoch});
for (const e of all) {
  const bytes=envelope(e);
  assert.deepEqual(parse(bytes),e);
  verifyEvidence(bytes,expected(e));
  assert.throws(()=>verifyEvidence(bytes,{...expected(e),issuer:coord.publicKey.equals(e.issuer)?devices[0].publicKey:coord.publicKey}));
  assert.throws(()=>verifyEvidence(bytes,{...expected(e),binding:label('OTHER attempt')}));
  assert.throws(()=>verifyEvidence(bytes,{...expected(e),signerEpoch:e.signerEpoch+1n}));
  assert.throws(()=>verifyEvidence(bytes,{...expected(e),decisionEpoch:e.decisionEpoch+1n}));
  assert.throws(()=>verifyEvidence(bytes,{...expected(e),kind:0}));
  const corrupted=Buffer.from(bytes); corrupted[corrupted.length-1]^=1;
  assert.throws(()=>verifyEvidence(corrupted,expected(e)));
  assert.throws(()=>parse(Buffer.concat([bytes,Buffer.from([0])])));
  assert.throws(()=>parse(bytes.subarray(0,-1)));
  const domain=Buffer.from(bytes); domain[0]^=1;
  assert.throws(()=>parse(domain));
}
assert.equal(DOMAIN.subarray(0,6).toString(),'hawala');
assert.throws(()=>recoveryMandate({...mandate,successorEpoch:9n}));
assert.throws(()=>recoveryMandate({...mandate,coordinatorEpoch:0xffff_ffff_ffff_ffffn,successorEpoch:1n<<64n}));
assert.throws(()=>recoveryMandate({...mandate,participants:mandate.participants.toReversed()}));
assert.throws(()=>manifest({...m,legs:[m.legs[0],{...m.legs[1],allocation:m.legs[0].allocation}]}));
assert.throws(()=>money(-1n)); assert.throws(()=>money(1n<<63n)); assert.throws(()=>money(1));
assert.throws(()=>manifest({...m,legs:m.legs.toReversed()}));
assert.throws(()=>evidenceSet([evidenceId(decision),evidenceId(decision)]));
// Signature-graph checks on synthetic fixtures; real finality evidence remains external.
const checked = (e,role,kind) => verifyEvidence(envelope(e),{...expected(e),issuer:role.publicKey,kind:KINDS[kind],signerEpoch:role===coord?7n:devices.includes(role)?0n:3n,decisionEpoch:7n});
const checkGraph = bundle => {
  if (bundle.proposed.length!==2) throw Error('HOLD: both device consents required');
  if (bundle.observed.length!==2 || bundle.settled.length!==2) throw Error('HOLD: incomplete settlement graph');
  bundle.proposed.forEach((e,i)=>checked(e,devices[i],'Proposed'));
  if (bundle.prepared.length!==2 || bundle.acks.length!==2) throw Error('HOLD: incomplete intent evidence');
  bundle.prepared.forEach((e,i)=>checked(e,legs[i].signer,'Prepared'));
  checked(bundle.decision,coord,'CommitDecision');
  assert(bundle.decision.payload[0].equals(evidenceSet(bundle.prepared.map(evidenceId))));
  bundle.acks.forEach((e,i)=>{
    checked(e,legs[i].signer,'CommitAck');
    assert(e.payload[0].equals(evidenceId(bundle.decision)));
    assert(e.payload[2].equals(bundle.prepared[i].payload[0]));
  });
  bundle.observed.forEach((e,i)=>{
    checked(e,legs[i].executor,'ExecutionObserved');
    assert(e.payload[0].equals(evidenceId(bundle.acks[i])));
  });
  bundle.settled.forEach((e,i)=>{
    checked(e,legs[i].settler,'Settled');
    assert(e.payload[0].equals(evidenceId(bundle.acks[i])));
    assert(e.payload[1].equals(evidenceId(bundle.observed[i])));
    assert(e.payload[2].equals(m.finality));
  });
};
checkGraph({proposed,prepared,decision,acks,observed,settled});
assert.throws(()=>checkGraph({proposed,prepared,decision,acks:acks.slice(0,1),observed,settled}));
assert.throws(()=>checkGraph({proposed,prepared,decision,acks:acks.toReversed(),observed,settled}));
assert.throws(()=>checkGraph({proposed,prepared,decision,acks,observed:settled,settled}));
console.log(`PASS: ${all.length} signed TEST vectors, complete intent evidence graph, role/attempt/epoch/domain/replay negatives and i64 boundaries`);
