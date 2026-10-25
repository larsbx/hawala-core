module coordination {
  var s: { decision: int, leftLocked: bool, rightLocked: bool,
    leftReleased: bool, rightReleased: bool }
  action init = s' = { decision: 0, leftLocked: false, rightLocked: false,
    leftReleased: false, rightReleased: false }
  action reserveLeft = s' = s.with("leftLocked", true)
  action reserveRight = s' = s.with("rightLocked", true)
  action commit = s' = if (s.decision == 0 and s.leftLocked and s.rightLocked)
    s.with("decision", 1) else s
  action abort = s' = if (s.decision == 0) s.with("decision", 2) else s
  action releaseLeft = s' = if (s.decision == 2 and s.leftLocked)
    s.with("leftReleased", true) else s
  action releaseRight = s' = if (s.decision == 2 and s.rightLocked)
    s.with("rightReleased", true) else s
  action stutter = s' = s
  action step = any { reserveLeft, reserveRight, commit, abort, releaseLeft, releaseRight, stutter }
  val safety = and {
    s.decision.in(Set(0,1,2)),
    (s.decision == 1) implies (s.leftLocked and s.rightLocked and not(s.leftReleased) and not(s.rightReleased)),
    s.leftReleased implies (s.decision == 2 and s.leftLocked),
    s.rightReleased implies (s.decision == 2 and s.rightLocked)
  }
  // Mutations must be caught even when no second financial posting occurs.
  action unsafeCommit = s' = if (s.leftLocked and s.rightLocked) s.with("decision", 1) else s
  action unsafeRelease = s' = if (s.leftLocked) s.with("leftReleased", true) else s
  action unsafeSingle = s' = if (s.decision == 0 and s.leftLocked) s.with("decision", 1) else s
  action unsafeAbortStep = any { step, unsafeCommit }
  action unsafeTimeoutStep = any { step, unsafeRelease }
  action unsafeSingleStep = any { step, unsafeSingle }
}
