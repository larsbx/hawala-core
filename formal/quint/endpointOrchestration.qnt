module endpointOrchestration {
  var s: { stage: int, effect: int, target: int, pending: bool, commits: int,
    client: int, desired: int, outbox: bool, held: bool, paid: bool }
  action init = s' = { stage:0, effect:0, target:0, pending:false, commits:0,
    client:0, desired:0, outbox:false, held:false, paid:false }
  action queue(n) = s' = if (not(s.outbox) and n == s.client+1 and n <= 3)
    s.with("desired",n).with("outbox",true).with("held",true) else s
  action admit(n,valid,fresh) = s' = if (valid and fresh and not(s.pending) and n == s.stage+1 and n <= 3)
    s.with("target",n).with("pending",true) else s
  action effect = s' = if (s.pending and s.effect < s.target)
    s.with("effect",s.target).with("commits",s.commits+(if(s.target==3) 1 else 0)) else s
  action finish = s' = if(s.pending and s.effect == s.target)
    s.with("stage",s.target).with("pending",false) else s
  action reply(valid) = s' = if(valid and s.outbox and not(s.pending) and s.stage >= s.desired)
    s.with("client",s.stage).with("outbox",false).with("held",false) else s.with("held",true)
  action timeout = s' = s.with("held",true)
  action restart = s' = s
  action replay = s' = s
  action step = any { queue(1),queue(2),queue(3),admit(1,true,true),admit(2,true,true),admit(3,true,true),
    admit(3,false,true),admit(3,true,false),effect,finish,reply(true),reply(false),timeout,restart,replay }
  val safety = and { s.stage>=0, s.stage<=3, s.effect<=3, s.stage<=s.effect,
    s.effect<=s.stage+1, (s.effect>s.stage) implies s.pending,
    s.pending implies (s.target==s.stage+1 and s.target<=3),
    s.commits == (if(s.effect==3) 1 else 0), s.client<=s.stage,
    s.desired<=3, not(s.paid) }
  action unsafeReceived = s' = s.with("paid",true)
  action unsafeUnbound = s' = s.with("client",3)
  action unsafeForget = s' = if(s.commits==1) s.with("stage",0).with("effect",0).with("pending",false) else s
  action unsafeRetry = s' = if(s.effect==3) s.with("commits",s.commits+1) else s
  action unsafeReceivedStep = any { step, unsafeReceived }
  action unsafeUnboundStep = any { step, unsafeUnbound }
  action unsafeForgetStep = any { step, unsafeForget }
  action unsafeRetryStep = any { step, unsafeRetry }
}
