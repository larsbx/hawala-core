// Finite projection: three native operations and two independent destinations.
module tigerBeetleFirst {
  var s: { committed: int, effects: int, sinkA: int, sinkB: int, acknowledged: int }
  action init = s' = { committed:0, effects:0, sinkA:0, sinkB:0, acknowledged:0 }
  action accept(n, valid) = s' = if (valid and n == s.committed+1 and n <= 3)
    s.with("committed",n).with("effects",s.effects+1) else s
  action persistA(n) = s' = if (n == s.sinkA+1 and n <= s.committed)
    s.with("sinkA",n) else s
  action persistB(n) = s' = if (n == s.sinkB+1 and n <= s.committed)
    s.with("sinkB",n) else s
  action acknowledge(n, valid) = s' = if (valid and n <= s.committed)
    s.with("acknowledged", if(n > s.acknowledged) n else s.acknowledged) else s
  action timeout = s' = s
  action crash = s' = s
  action replay = s' = s
  action step = any {
    accept(1,true), accept(2,true), accept(3,true), accept(1,false),
    persistA(1), persistA(2), persistA(3), persistB(1), persistB(2), persistB(3),
    acknowledge(1,true), acknowledge(2,true), acknowledge(3,true), acknowledge(3,false),
    timeout, crash, replay
  }
  val safety = and {
    s.effects == s.committed, s.sinkA <= s.committed, s.sinkB <= s.committed,
    s.acknowledged <= s.committed, s.committed >= 0, s.committed <= 3,
    s.sinkA >= 0, s.sinkB >= 0, s.acknowledged >= 0
  }
  action unsafeMirror = s' = s.with("sinkA",s.committed+1)
  action unsafeDuplicate = s' = if(s.committed>0) s.with("effects",s.effects+1) else s
  action unsafeForget = s' = if(s.committed>0) s.with("committed",0) else s
  action unsafeAck = s' = s.with("acknowledged",s.committed+1)
  action unsafeMirrorStep = any { step, unsafeMirror }
  action unsafeDuplicateStep = any { step, unsafeDuplicate }
  action unsafeForgetStep = any { step, unsafeForget }
  action unsafeAckStep = any { step, unsafeAck }
  run financialCommitWithoutAnySinkTest = init.then(accept(1,true)).then(accept(2,true))
    .then(accept(3,true)).expect(s.committed==3 and s.sinkA==0 and s.sinkB==0)
  run sinksIndependentTest = init.then(accept(1,true)).then(persistA(1))
    .expect(s.sinkA==1 and s.sinkB==0)
  run crashBeforePersistenceTest = init.then(accept(1,true)).then(crash)
    .then(persistA(1)).expect(s.committed==1 and s.effects==1 and s.sinkA==1)
  run gapRefusedTest = init.then(accept(1,true)).then(accept(2,true)).then(persistA(2))
    .expect(s.sinkA==0)
  run exactReplayTest = init.then(accept(1,true)).then(accept(1,true)).then(replay)
    .expect(s.effects==1 and s.committed==1)
}
