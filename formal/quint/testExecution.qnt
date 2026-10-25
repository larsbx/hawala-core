module testExecution {
  var s: { ticket: bool, terminal: int, effects: int }
  var fenced: bool
  action init = all { s' = { ticket: false, terminal: 0, effects: 0 }, fenced' = false }
  action ticket(valid: bool): bool = all { s' = if (valid) s.with("ticket",true) else s, fenced' = fenced }
  action execute(scope: bool): bool = all { s' = if (scope and s.ticket and s.terminal == 0)
    s.with("terminal",1).with("effects",s.effects+1) else s, fenced' = fenced }
  action fence = all { s' = if (s.terminal == 0) s.with("terminal",2) else s, fenced' = (fenced or s.terminal == 0) }
  action stutter = all { s' = s, fenced' = fenced }
  action step = any { ticket(true), ticket(false), execute(true), execute(false), fence, stutter }
  val safety = and {
    s.terminal.in(Set(0,1,2)), s.effects.in(Set(0,1)),
    (s.terminal == 1) == (s.effects == 1),
    (s.terminal != 1) implies (s.effects == 0),
    (s.effects == 1) implies s.ticket,
    fenced implies (s.terminal == 2 and s.effects == 0)
  }
  // Mutations: repeated effect; execute despite no-effect fence; unsigned ticket.
  action unsafeDuplicate = all { s' = if (s.ticket and s.terminal != 2) s.with("terminal",1).with("effects",s.effects+1) else s, fenced' = fenced }
  action unsafeFence = all { s' = if (s.ticket and s.terminal == 2) s.with("terminal",1).with("effects",1) else s, fenced' = fenced }
  action unsafeTicket = all { s' = if (s.terminal == 0) s.with("terminal",1).with("effects",1) else s, fenced' = fenced }
  action unsafeDuplicateStep = any { step, unsafeDuplicate }
  action unsafeFenceStep = any { step, unsafeFence }
  action unsafeTicketStep = any { step, unsafeTicket }
}
