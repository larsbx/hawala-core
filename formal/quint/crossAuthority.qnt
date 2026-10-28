// ONE immutable manifest, two independent durable participants, asynchronous
// authenticated messages and one scoped decision log. No money execution.
module crossAuthority {
  var s: { requested: bool, requestEpoch: int, voteL: int, voteR: int,
    seenL: bool, seenR: bool, decision: int, wire: int, wireEpoch: int,
    localL: int, localR: int, upC: bool, upL: bool, upR: bool,
    netL: bool, netR: bool, epoch: int, installedL: bool, installedR: bool, recoveryWire: bool, fenceC: bool, fenceL: bool,
    fenceR: bool, recovered: bool }
  action init = s' = { requested: false, requestEpoch: 0, voteL: 0, voteR: 0,
    seenL: false, seenR: false, decision: 0, wire: 0, wireEpoch: 0,
    localL: 0, localR: 0, upC: true, upL: true, upR: true,
    netL: true, netR: true, epoch: 0, installedL: false, installedR: false, recoveryWire: false, fenceC: false, fenceL: false,
    fenceR: false, recovered: false }
  action sendPrepare = s' = if (s.upC and not(s.fenceC) and s.decision == 0)
    s.with("requested", true).with("requestEpoch", s.epoch) else s
  action prepareL(ctx, ae, ce) = s' = if (ctx == 0 and ae == 0 and ce == s.epoch and
    s.requested and ce == s.requestEpoch and s.upL and s.netL and not(s.fenceL) and s.localL == 0 and s.voteL == 0)
    s.with("voteL", 1) else s
  action prepareR(ctx, ae, ce) = s' = if (ctx == 0 and ae == 0 and ce == s.epoch and
    s.requested and ce == s.requestEpoch and s.upR and s.netR and not(s.fenceR) and s.localR == 0 and s.voteR == 0)
    s.with("voteR", 1) else s
  action refuseL = s' = if (s.upL and s.voteL == 0) s.with("voteL", 2) else s
  action refuseR = s' = if (s.upR and s.voteR == 0) s.with("voteR", 2) else s
  action receiveL = s' = if (s.upC and s.netL and s.voteL == 1) s.with("seenL", true) else s
  action receiveR = s' = if (s.upC and s.netR and s.voteR == 1) s.with("seenR", true) else s
  action commit = s' = if (s.upC and not(s.fenceC) and s.decision == 0 and s.seenL and s.seenR)
    s.with("decision", 1) else s
  action abort = s' = if (s.upC and not(s.fenceC) and s.decision == 0)
    s.with("decision", 2) else s
  action publish = s' = if (s.upC and not(s.fenceC) and s.wire == 0 and s.decision != 0)
    s.with("wire", s.decision).with("wireEpoch", s.epoch) else s
  // Authentication restricts current-attempt terminal messages to the published
  // record. The foreign controls are otherwise valid signed prior evidence.
  action acceptL(ctx, ae, value, ce) = s' = if (ctx == 0 and ae == 0 and s.upL and s.netL and value != 0 and ce <= s.epoch)
    s.with("localL", value) else s
  action acceptR(ctx, ae, value, ce) = s' = if (ctx == 0 and ae == 0 and s.upR and s.netR and value != 0 and ce <= s.epoch)
    s.with("localR", value) else s
  action deliverL = acceptL(0, 0, s.wire, s.wireEpoch)
  action deliverR = acceptR(0, 0, s.wire, s.wireEpoch)
  action foreignAttempt = acceptL(1, 0, 1, 0)
  action foreignAuthority = acceptL(0, 1, 1, 0)
  action crashC = s' = s.with("upC", false)
  action restartC = s' = if (not(s.fenceC)) s.with("upC", true) else s
  action crashL = s' = s.with("upL", false)
  action restartL = s' = s.with("upL", true)
  action crashR = s' = s.with("upR", false)
  action restartR = s' = s.with("upR", true)
  action partitionL = s' = s.with("netL", false)
  action healL = s' = s.with("netL", true)
  action partitionR = s' = s.with("netR", false)
  action healR = s' = s.with("netR", true)
  action timeout = s' = s
  // Effective external fence ACK; it is NOT inferred from coordinator absence.
  action fenceCoordinator = s' = s.with("fenceC", true).with("upC", false).with("recovered", false)
  action fenceLeft = s' = if (s.fenceC and s.upL and s.netL) s.with("fenceL", true) else s
  action fenceRight = s' = if (s.fenceC and s.upR and s.netR) s.with("fenceR", true) else s
  action recoverLog = s' = if (s.fenceC) s.with("recovered", true) else s
  action publishRecovery = s' = if (s.fenceC and s.fenceL and s.fenceR and s.recovered)
    s.with("recoveryWire", true) else s
  action installL = s' = if (s.recoveryWire and s.upL and s.netL) s.with("installedL", true) else s
  action installR = s' = if (s.recoveryWire and s.upR and s.netR) s.with("installedR", true) else s
  action recover = s' = if (s.fenceC and s.fenceL and s.fenceR and s.recovered and s.installedL and s.installedR and s.epoch == 0)
    s.with("epoch", 1).with("upC", true).with("fenceC", false) else s
  action activateL = s' = if (s.epoch == 1 and s.installedL and s.upL and s.netL) s.with("fenceL", false) else s
  action activateR = s' = if (s.epoch == 1 and s.installedR and s.upR and s.netR) s.with("fenceR", false) else s
  action step = any { sendPrepare, prepareL(0,0,0), prepareR(0,0,0),
    prepareL(0,0,1), prepareR(0,0,1), prepareL(1,0,0), prepareR(0,1,0),
    refuseL, refuseR, receiveL, receiveR, commit, abort, publish,
    deliverL, deliverR, foreignAttempt, foreignAuthority,
    crashC, restartC, crashL, restartL, crashR, restartR,
    partitionL, healL, partitionR, healR, timeout,
    fenceCoordinator, fenceLeft, fenceRight, recoverLog, publishRecovery, installL, installR, recover, activateL, activateR }
  val safety = and {
    s.voteL.in(Set(0,1,2)), s.voteR.in(Set(0,1,2)),
    s.decision.in(Set(0,1,2)), s.wire.in(Set(0,1,2)),
    s.seenL implies (s.voteL == 1), s.seenR implies (s.voteR == 1),
    (s.decision == 1) implies (s.seenL and s.seenR),
    (s.wire != 0) implies (s.wire == s.decision),
    (s.localL != 0) implies (s.localL == s.decision),
    (s.localR != 0) implies (s.localR == s.decision),
    (s.localL != 0) implies (s.localL == s.wire),
    (s.localR != 0) implies (s.localR == s.wire)
  }
  val agreement = (s.localL != 0 and s.localR != 0) implies (s.localL == s.localR)
  val heldL = s.voteL == 1 and s.localL == 0
  val lockedL = s.voteL == 1 and s.localL != 2
  // Mutants intentionally break one obligation. Infrastructure errors are not passes.
  action unsafeTimeout = s' = if (heldL) s.with("localL", 2) else s
  action unsafeSingle = s' = if (s.decision == 0 and s.seenL) s.with("decision", 1) else s
  action unsafeRecovery = s' = if (s.fenceC) s.with("decision", 0).with("wire", 0) else s
  action unsafeReplay = acceptL(0,0,1,0) // foreign signed commit mis-bound to this attempt
  action unsafeZombie = s' = if (s.fenceC and s.decision == 2) s.with("wire", 1) else s
  action unsafeTimeoutStep = any { step, unsafeTimeout }
  action unsafeSingleStep = any { step, unsafeSingle }
  action unsafeRecoveryStep = any { step, unsafeRecovery }
  action unsafeReplayStep = any { step, unsafeReplay }
  action unsafeZombieStep = any { step, unsafeZombie }
}
