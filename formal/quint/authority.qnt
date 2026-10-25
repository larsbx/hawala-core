module authority {
  // Single-lot model: authorization combines signed scope, epoch and expiry.
  // Crash/replay/rejection are stuttering, matching durable committed state.
  type State = { available: int, reserved: int, committed: bool, nonce: bool,
    claimed: bool, uncertain: bool, claims: int }
  var s: State
  pure val funding = 1000
  pure val amount = 900
  action init = s' = { available: funding, reserved: 0, committed: false,
    nonce: false, claimed: false, uncertain: false, claims: 0 }
  action admit(authorized: bool): bool =
    if (authorized and not(s.committed) and s.available >= amount)
      s' = { ...s, available: s.available - amount, reserved: s.reserved + amount,
        committed: true, nonce: true }
    else s' = s
  action claim(currentScope: bool): bool =
    if (currentScope and s.committed and not(s.claimed) and not(s.uncertain))
      s' = { ...s, claimed: true, uncertain: true, claims: s.claims + 1 }
    else s' = s
  action hold = if (s.committed) s' = { ...s, uncertain: true } else s' = s
  action crashReplayReject = s' = s
  action step = any { admit(true), admit(false), claim(true), claim(false), hold, crashReplayReject }
  val safety = and {
    s.available >= 0, s.reserved >= 0,
    s.available + s.reserved == funding,
    s.nonce == s.committed,
    s.claims <= 1,
    not(s.claimed) implies s.claims == 0,
    s.claimed implies (s.uncertain and s.committed)
  }
  // Negative control: removes the no-reclaim guard after a valid claim.
  action unsafeClaim = if (s.committed)
    s' = { ...s, claimed: true, uncertain: true, claims: s.claims + 1 }
    else s' = s
  action unsafeStep = any { admit(true), unsafeClaim, hold, crashReplayReject }
}
