import Std.Tactic

/- Two independently confirmed TEST reservations and one durable decision.
   No model event releases a committed reservation or executes a payout. -/
namespace Coordination
structure State where
  decision : Nat -- 0 undecided, 1 committed, 2 final abort
  leftLocked : Bool
  rightLocked : Bool
  leftReleased : Bool
  rightReleased : Bool
  deriving DecidableEq, Repr

inductive Event where
  | reserveLeft | reserveRight | commit | abort
  | releaseLeft | releaseRight | timeout | restart | denied
  deriving DecidableEq, Repr

def initial : State := ⟨0, false, false, false, false⟩
def step (s : State) (e : Event) : State :=
  match e with
  -- Late confirmations after abort are retained until explicitly released.
  | .reserveLeft => {s with leftLocked := true}
  | .reserveRight => {s with rightLocked := true}
  | .commit =>
    if s.decision = 0 ∧ s.leftLocked = true ∧ s.rightLocked = true then
      {s with decision := 1} else s
  | .abort => if s.decision = 0 then {s with decision := 2} else s
  | .releaseLeft =>
    if s.decision = 2 ∧ s.leftLocked = true then {s with leftReleased := true} else s
  | .releaseRight =>
    if s.decision = 2 ∧ s.rightLocked = true then {s with rightReleased := true} else s
  | .timeout | .restart | .denied => s

def Safe (s : State) : Prop :=
  s.decision ≤ 2 ∧
  (s.decision = 1 → s.leftLocked = true ∧ s.rightLocked = true ∧
    s.leftReleased = false ∧ s.rightReleased = false) ∧
  (s.leftReleased = true → s.decision = 2 ∧ s.leftLocked = true) ∧
  (s.rightReleased = true → s.decision = 2 ∧ s.rightLocked = true)

theorem initial_safe : Safe initial := by simp [Safe, initial]
theorem step_safe (s : State) (e : Event) (h : Safe s) : Safe (step s e) := by
  rcases h with ⟨bound, committed, left, right⟩
  cases e <;> simp only [step]
  all_goals first
    | (split <;> simp_all [Safe] <;> omega)
    | (simp_all [Safe])

inductive Reachable : State → Prop where
  | init : Reachable initial
  | next {s : State} : Reachable s → (e : Event) → Reachable (step s e)

theorem reachable_safe {s : State} (h : Reachable s) : Safe s := by
  induction h with
  | init => exact initial_safe
  | next _ e ih => exact step_safe _ e ih

theorem terminal_permanent (s : State) (e : Event) (h : s.decision ≠ 0) :
    (step s e).decision = s.decision := by
  cases e <;> simp [step, h]
  all_goals split <;> rfl

theorem abort_fences_commit (s : State) (h : s.decision = 2) :
    step s .commit = s := by simp [step, h]

theorem commit_requires_both {s : State} (h : Reachable s) (committed : s.decision = 1) :
    s.leftLocked = true ∧ s.rightLocked = true ∧
    s.leftReleased = false ∧ s.rightReleased = false :=
  (reachable_safe h).2.1 committed

theorem timeout_never_releases (s : State) : step s .timeout = s := rfl

#print axioms reachable_safe
#print axioms terminal_permanent
end Coordination
