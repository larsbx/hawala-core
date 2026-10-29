// Storage transaction projection, one original financial operation.
// `checked` is a trusted signature/context/body/completeness predicate.
module tigerBeetleStorage {
  var s: { retained:bool, committed:bool, effects:int,
    rowsA:bool, cursorA:bool, stageA:bool, rowsB:bool, cursorB:bool, stageB:bool }
  action init = s' = {retained:false,committed:false,effects:0,
    rowsA:false,cursorA:false,stageA:false,rowsB:false,cursorB:false,stageB:false}
  action retain(checked) = s' = if(checked) s.with("retained",true) else s
  action native(original) = s' = if(s.retained and original and not(s.committed))
    s.with("committed",true).with("effects",s.effects+1) else s
  action stageA(checked) = s' = if(s.committed and checked) s.with("stageA",true) else s
  action stageB(checked) = s' = if(s.committed and checked) s.with("stageB",true) else s
  action commitA = s' = if(s.stageA) s.with("rowsA",true).with("cursorA",true).with("stageA",false) else s
  action commitB = s' = if(s.stageB) s.with("rowsB",true).with("cursorB",true).with("stageB",false) else s
  action crash = s' = s.with("stageA",false).with("stageB",false)
  action unavailable = s' = s
  action lostAck = s' = s
  action step = any {retain(true),retain(false),native(true),native(false),stageA(true),stageA(false),
    stageB(true),stageB(false),commitA,commitB,crash,unavailable,lostAck}
  val safety = and {s.effects == (if(s.committed) 1 else 0),s.rowsA==s.cursorA,s.rowsB==s.cursorB,
    s.rowsA implies s.committed,s.rowsB implies s.committed,
    s.stageA implies s.committed,s.stageB implies s.committed}
  action unsafeSplit = s' = s.with("rowsA",true)
  action unsafeRepeat = s' = if(s.committed) s.with("effects",s.effects+1) else s
  action unsafeSplitStep = any {step,unsafeSplit}
  action unsafeRepeatStep = any {step,unsafeRepeat}
  run crashBeforeCommitTest = init.then(retain(true)).then(native(true)).then(stageA(true)).then(crash)
    .expect(s.effects==1 and not(s.rowsA) and not(s.cursorA) and not(s.stageA))
  run unavailableDestinationTest = init.then(retain(true)).then(native(true)).then(unavailable)
    .then(stageB(true)).then(commitB).expect(s.effects==1 and s.rowsB and not(s.rowsA))
  run lostReplyAndDuplicateTest = init.then(retain(true)).then(native(true)).then(stageA(true)).then(commitA)
    .then(lostAck).then(native(true)).then(stageA(true)).then(commitA).expect(s.effects==1 and s.rowsA and s.cursorA)
  run changedBodyPartialAndBadSignatureTest = init.then(retain(true)).then(native(false)).then(stageA(false))
    .then(commitA).expect(s.effects==0 and not(s.rowsA))
}
