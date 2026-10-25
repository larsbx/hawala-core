module coordinatedAdmission {
  var s: { decision: int, leftLocked: bool, rightLocked: bool,
    available: int, reserved: int, committed: bool, nonce: bool, current: bool }
  var aborted: bool
  action init = all {
    s' = { decision: 0, leftLocked: false, rightLocked: false,
      available: 1000, reserved: 0, committed: false, nonce: false, current: true },
    aborted' = false
  }
  action left = all { s' = if (s.current) s.with("leftLocked",true) else s, aborted' = aborted }
  action right = all { s' = if (s.current) s.with("rightLocked",true) else s, aborted' = aborted }
  action abort = all {
    s' = if (s.current and s.decision == 0) s.with("decision",2) else s,
    aborted' = (aborted or (s.current and s.decision == 0))
  }
  pure def committedState(x) = x.with("decision",1).with("available",100).with("reserved",900)
    .with("committed",true).with("nonce",true)
  action admit(authorized: bool): bool = all {
    s' = if (authorized and s.current and s.decision == 0 and s.leftLocked and s.rightLocked)
      committedState(s) else s, aborted' = aborted
  }
  action fence = all { s' = s.with("current",false), aborted' = aborted }
  action stutter = all { s' = s, aborted' = aborted }
  action step = any { left, right, abort, admit(true), admit(false), fence, stutter }
  val safety = and {
    s.available + s.reserved == 1000,
    s.available.in(Set(100,1000)), s.reserved.in(Set(0,900)),
    s.decision.in(Set(0,1,2)),
    (s.decision == 1) == s.committed, s.nonce == s.committed,
    s.committed implies (s.leftLocked and s.rightLocked and s.reserved == 900),
    not(s.committed) implies (s.reserved == 0),
    aborted implies (s.decision == 2 and not(s.committed))
  }
  action unsafeFinancial = all {
    s' = s.with("available",100).with("reserved",900).with("committed",true).with("nonce",true), aborted' = aborted
  }
  action unsafeDecision = all { s' = s.with("decision",1), aborted' = aborted }
  action unsafeAfterAbort = all {
    s' = if (s.decision == 2 and s.leftLocked and s.rightLocked) committedState(s) else s,
    aborted' = aborted
  }
  action unsafeLegacyStep = any { step, unsafeFinancial }
  action unsafeSplitStep = any { step, unsafeDecision }
  action unsafeAbortStep = any { step, unsafeAfterAbort }
}
