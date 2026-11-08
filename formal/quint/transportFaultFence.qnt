// Independent finite transition model. "valid" is an abstract, trusted verifier
// result, not a signature verification performed by this model.
module transportFaultFence {
  var s: { queued: bool, sent: bool, held: bool, statusVerified: bool,
           dispatches: int, paid: bool }

  action init = s' = { queued: false, sent: false, held: false,
    statusVerified: false, dispatches: 0, paid: false }

  action enqueue = s' = if (not(s.queued))
    s.with("queued", true).with("held", true) else s
  action dispatch = s' = if (s.queued and not(s.sent))
    s.with("sent", true).with("dispatches", 1) else s
  action outage = s' = if (s.queued) s.with("held", true) else s
  action timeout = s' = if (s.queued) s.with("held", true) else s
  action status(valid, complete) = s' = if (valid and complete and s.sent)
    s.with("statusVerified", true).with("held", false) else s
  action reconnect = s' = s
  action restart = s' = s
  action replay = s' = s

  action step = any { enqueue, dispatch, outage, timeout, status(true,true),
    status(false,true), status(true,false), status(false,false),
    reconnect, restart, replay }

  val safety = and {
    s.dispatches >= 0, s.dispatches <= 1,
    s.sent implies (s.queued and s.dispatches == 1),
    not(s.sent) implies (s.dispatches == 0),
    (s.queued and not(s.statusVerified)) implies s.held,
    s.statusVerified implies s.sent,
    not(s.paid)
  }

  // Each mutation must generate a real TLC safety counterexample.
  action unsafeRetry = s' = if (s.sent)
    s.with("dispatches", s.dispatches + 1) else s
  action unsafePrematureAck = s' = if (s.queued)
    s.with("held", false) else s
  action unsafeTimeoutPays = s' = s.with("paid", true)
  action unsafeRetryStep = any { step, unsafeRetry }
  action unsafePrematureAckStep = any { step, unsafePrematureAck }
  action unsafeTimeoutPaysStep = any { step, unsafeTimeoutPays }
}
