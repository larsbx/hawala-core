import Hawala
import Coordination

namespace CoordinatedAdmission
structure State where
  custody : Coordination.State
  financial : Hawala.State
  deriving DecidableEq, Repr
inductive Event where
  | left | right | admit (authorized : Bool) | abort | fence | restart | replay | legacy
  deriving DecidableEq, Repr
def initial (funding : Nat) : State := ⟨Coordination.initial, Hawala.initial funding⟩
def step (amount : Nat) (s : State) (e : Event) : State :=
  match e with
  | .left => if s.financial.epochCurrent then
      {s with custody := Coordination.step s.custody .reserveLeft} else s
  | .right => if s.financial.epochCurrent then
      {s with custody := Coordination.step s.custody .reserveRight} else s
  | .abort => if s.financial.epochCurrent then
      {s with custody := Coordination.step s.custody .abort} else s
  | .admit authorized =>
    if s.custody.decision = 0 ∧ s.custody.leftLocked = true ∧ s.custody.rightLocked = true ∧
        authorized = true ∧ s.financial.epochCurrent = true ∧
        s.financial.committed = false ∧ amount ≤ s.financial.available then
      ⟨Coordination.step s.custody .commit, Hawala.step amount s.financial (.admit authorized)⟩
    else s
  | .fence => {s with financial := Hawala.step amount s.financial .fence}
  | .restart | .replay | .legacy => s
def Safe (funding : Nat) (s : State) : Prop :=
  Coordination.Safe s.custody ∧ Hawala.Safe funding s.financial ∧
  (s.custody.decision = 1 ↔ s.financial.committed = true)

theorem initial_safe (funding : Nat) : Safe funding (initial funding) := by
  exact ⟨Coordination.initial_safe, Hawala.initial_safe funding, by
    simp [initial, Coordination.initial, Hawala.initial]⟩

theorem step_safe (funding amount : Nat) (s : State) (e : Event)
    (h : Safe funding s) : Safe funding (step amount s e) := by
  rcases h with ⟨hc, hf, link⟩
  cases e with
  | left =>
    simp only [step]; split
    · exact ⟨Coordination.step_safe _ _ hc, hf, by simpa [Coordination.step] using link⟩
    · exact ⟨hc, hf, link⟩
  | right =>
    simp only [step]; split
    · exact ⟨Coordination.step_safe _ _ hc, hf, by simpa [Coordination.step] using link⟩
    · exact ⟨hc, hf, link⟩
  | abort =>
    simp only [step]; split
    · refine ⟨Coordination.step_safe _ _ hc, hf, ?_⟩
      by_cases zero : s.custody.decision = 0
      · have absent : s.financial.committed ≠ true := by
          intro yes
          have one := link.mpr yes
          omega
        simp [Coordination.step, zero, absent]
      · simpa [Coordination.step, zero] using link
    · exact ⟨hc, hf, link⟩
  | admit authorized =>
    simp only [step]; split
    · rename_i guard
      rcases guard with ⟨zero, left, right, auth, current, absent, enough⟩
      refine ⟨Coordination.step_safe _ _ hc, Hawala.step_safe funding amount _ _ hf, ?_⟩
      simp [Coordination.step, Hawala.step, zero, left, right, auth, current, absent, enough]
    · exact ⟨hc, hf, link⟩
  | fence =>
    exact ⟨hc, Hawala.step_safe funding amount _ _ hf, by simpa [step, Hawala.step] using link⟩
  | restart | replay | legacy => exact ⟨hc, hf, link⟩

inductive Reachable (funding amount : Nat) : State → Prop where
  | init : Reachable funding amount (initial funding)
  | next {s : State} : Reachable funding amount s → (e : Event) →
      Reachable funding amount (step amount s e)
theorem reachable_safe {funding amount : Nat} {s : State}
    (h : Reachable funding amount s) : Safe funding s := by
  induction h with
  | init => exact initial_safe funding
  | next _ e ih => exact step_safe funding amount _ e ih

theorem commit_is_atomic {funding amount : Nat} {s : State}
    (h : Reachable funding amount s) :
    s.custody.decision = 1 ↔ s.financial.committed = true := (reachable_safe h).2.2
theorem abort_blocks_financial_admission (amount : Nat) (s : State) (authorized : Bool)
    (h : s.custody.decision = 2) : step amount s (.admit authorized) = s := by
  simp [step, h]
theorem legacy_cannot_commit (amount : Nat) (s : State) : step amount s .legacy = s := rfl

theorem abort_permanent (amount : Nat) (s : State) (e : Event)
    (h : s.custody.decision = 2) : (step amount s e).custody.decision = 2 := by
  cases e <;> simp only [step]
  all_goals first | (split <;> simp_all [Coordination.step]) | exact h

theorem exact_conservation {funding amount : Nat} {s : State}
    (h : Reachable funding amount s) :
    s.financial.available + s.financial.reserved = funding := (reachable_safe h).2.1.1

#print axioms reachable_safe
end CoordinatedAdmission
