import Std.Tactic

/-!
  An abstract transport-failure fence for ONE TEST request/outbox item.
  This is not Iroh, Rust, cryptographic verification, custody, or settlement.
  The .status event's verified argument ASSUMES independently checked
  signature, exact attempt/request digest, epoch, authority, and freshness.
-/
namespace TransportFaultFence

structure State where
  queued : Bool := false
  sent : Bool := false
  held : Bool := false
  statusVerified : Bool := false
  dispatches : Nat := 0
  paid : Bool := false
  deriving DecidableEq, Repr

inductive Event where
  | enqueue | dispatch | outage | timeout | reconnect | restart | replay
  | status (verified complete : Bool)
  deriving DecidableEq, Repr

def initial : State := {}

def step (s : State) (e : Event) : State := match e with
  | .enqueue => if s.queued then s else {s with queued := true, held := true}
  | .dispatch =>
      if s.queued && !s.sent then {s with sent := true, dispatches := 1} else s
  | .outage | .timeout =>
      if s.queued then {s with held := true} else s
  | .status verified complete =>
      if verified && complete && s.sent then
        {s with statusVerified := true, held := false}
      else s
  | .reconnect | .restart | .replay => s

theorem reconnect_stutters (s : State) : step s .reconnect = s := rfl
theorem restart_stutters (s : State) : step s .restart = s := rfl
theorem replay_stutters (s : State) : step s .replay = s := rfl

theorem no_dispatch_before_enqueue (s : State) (h : s.queued = false) :
    step s .dispatch = s := by simp [step, h]

theorem sent_cannot_dispatch_again (s : State) (h : s.sent = true) :
    step s .dispatch = s := by simp [step, h]

theorem invalid_status_stutters (s : State) (complete : Bool) :
    step s (.status false complete) = s := by simp [step]

theorem incomplete_status_stutters (s : State) (verified : Bool) :
    step s (.status verified false) = s := by simp [step]

theorem timeout_keeps_dispatches (s : State) :
    (step s .timeout).dispatches = s.dispatches := by
  simp [step]
  split <;> rfl

theorem outage_keeps_dispatches (s : State) :
    (step s .outage).dispatches = s.dispatches := by
  simp [step]
  split <;> rfl

theorem step_preserves_paid (s : State) (e : Event) :
    (step s e).paid = s.paid := by
  cases e <;> simp [step] <;> split <;> rfl

theorem step_dispatches_bounded (s : State) (e : Event)
    (h : s.dispatches ≤ 1) : (step s e).dispatches ≤ 1 := by
  cases e <;> simp [step] at * <;> split <;> omega

inductive Reachable : State → Prop where
  | init : Reachable initial
  | next {s : State} : Reachable s → (e : Event) → Reachable (step s e)

theorem reachable_dispatches_bounded {s : State} (h : Reachable s) :
    s.dispatches ≤ 1 := by
  induction h with
  | init => simp [initial]
  | next _ e ih => exact step_dispatches_bounded _ e ih

theorem reachable_never_paid {s : State} (h : Reachable s) :
    s.paid = false := by
  induction h with
  | init => rfl
  | next _ e ih => rw [step_preserves_paid, ih]

#print axioms reachable_dispatches_bounded
#print axioms reachable_never_paid
end TransportFaultFence
