import Std.Tactic

/- A synthetic rail for one already committed operation. Valid ticket abstracts
   checking the full scoped dispatch, not proof of cash or custody control. -/
namespace TestExecution
structure State where
  ticket : Bool
  terminal : Nat -- 0 pending, 1 mock effect, 2 fenced no effect
  effects : Nat
  deriving DecidableEq, Repr
inductive Event where
  | ticket (valid : Bool) | execute (scope : Bool) | fence | partition | restart
  deriving DecidableEq, Repr
def initial : State := ⟨false, 0, 0⟩
def step (s : State) (e : Event) : State :=
  match e with
  | .ticket valid => if valid then {s with ticket := true} else s
  | .execute scope => if scope = true ∧ s.ticket = true ∧ s.terminal = 0 then
      {s with terminal := 1, effects := s.effects + 1} else s
  | .fence => if s.terminal = 0 then {s with terminal := 2} else s
  | .partition | .restart => s
def Safe (s : State) : Prop :=
  s.terminal ≤ 2 ∧ s.effects ≤ 1 ∧
  (s.terminal = 1 ↔ s.effects = 1) ∧
  (s.terminal ≠ 1 → s.effects = 0) ∧
  (s.effects = 1 → s.ticket = true)
theorem initial_safe : Safe initial := by simp [Safe, initial]
theorem step_safe (s : State) (e : Event) (h : Safe s) : Safe (step s e) := by
  rcases h with ⟨bound, once, delivered, zero, ticket⟩
  cases e <;> simp only [step]
  all_goals first | (split <;> simp_all [Safe] <;> omega) | simp_all [Safe]
inductive Reachable : State → Prop where
  | init : Reachable initial
  | next {s : State} : Reachable s → (e : Event) → Reachable (step s e)
theorem reachable_safe {s : State} (h : Reachable s) : Safe s := by
  induction h with
  | init => exact initial_safe
  | next _ e ih => exact step_safe _ e ih
theorem terminal_permanent (s : State) (e : Event) (h : s.terminal ≠ 0) :
    (step s e).terminal = s.terminal := by
  cases e <;> simp only [step]
  all_goals first | (split <;> simp_all) | rfl
theorem fence_blocks_late_execution (s : State) (scope : Bool) (h : s.terminal = 2) :
    step s (.execute scope) = s := by simp [step, h]
theorem terminal_effects_permanent (s : State) (e : Event) (h : s.terminal ≠ 0) :
    (step s e).effects = s.effects := by
  cases e <;> simp only [step]
  all_goals first | (split <;> simp_all) | rfl
theorem invalid_scope_stutters (s : State) : step s (.execute false) = s := by simp [step]
theorem no_second_effect {s : State} (h : Reachable s) : s.effects ≤ 1 := (reachable_safe h).2.1
#print axioms reachable_safe
end TestExecution
