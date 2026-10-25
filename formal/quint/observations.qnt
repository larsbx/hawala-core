module observations {
  var context: int
  var seen: int
  var conflict: bool
  var uncertain: bool
  action initWith(c: int): bool = all {
    context' = c, seen' = -1, conflict' = false, uncertain' = (c == 2 or c == 3)
  }
  action init = {
    nondet c = Set(0,1,2,3,4).oneOf()
    initWith(c)
  }
  action restart = all { context' = context, seen' = seen, conflict' = conflict, uncertain' = uncertain }
  action report(verified: bool, delivered: int): bool =
    if (verified and context != 0) all {
      context' = context,
      seen' = if (seen == -1) delivered else seen,
      conflict' = (conflict or (seen != -1 and seen != delivered)),
      uncertain' = true
    } else restart
  action step = any { report(true,0), report(true,1), report(false,0), report(false,1), restart }
  val safety = and {
    context.in(Set(0,1,2,3,4)), seen.in(Set(-1,0,1)),
    (context == 0) implies (seen == -1),
    (seen != -1) implies uncertain,
    conflict implies (seen != -1),
    (context == 2 or context == 3) implies uncertain
  }
  // Negative control: treat a delivery report as permission to clear the hold.
  action unsafeDelivery = if (context != 0) all {
    context' = context, seen' = 1, conflict' = conflict, uncertain' = false
  } else restart
  action unsafeStep = any { report(true,0), unsafeDelivery, restart }
}
