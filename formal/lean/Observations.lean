import Hawala
namespace Observations
open Hawala
structure Snapshot where
  intent : State
  primary : Option Bool
  conflict : Bool
  deriving DecidableEq, Repr
inductive Event where
  | report (verified delivered : Bool)
  | restart
  deriving DecidableEq, Repr

def initial (intent : State) : Snapshot := ⟨intent, none, false⟩
def step (amount : Nat) (s : Snapshot) (e : Event) : Snapshot :=
  match e with
  | .restart => s
  | .report verified delivered =>
    if verified = true ∧ s.intent.committed = true then
      { intent := Hawala.step amount s.intent .hold
        primary := s.primary.or (some delivered)
        conflict := s.conflict || (match s.primary with
          | none => false | some old => old != delivered) }
    else s

def financialProjection (s : State) :=
  (s.available, s.reserved, s.committed, s.nonce, s.claimed, s.claims,
   s.expired, s.epochCurrent, s.fencedClaims)

theorem protected_unchanged (amount : Nat) (s : Snapshot) (e : Event) :
    financialProjection (step amount s e).intent = financialProjection s.intent := by
  cases e <;> simp only [step]
  all_goals first
    | (split <;> simp_all [financialProjection, Hawala.step])
    | rfl

theorem safety_preserved (funding amount : Nat) (s : Snapshot) (e : Event)
    (safe : Safe funding s.intent) : Safe funding (step amount s e).intent := by
  cases e <;> simp only [step]
  case restart => exact safe
  case report verified delivered =>
    split
    · exact step_safe funding amount s.intent .hold safe
    · exact safe

theorem accepted_is_held (amount : Nat) (s : Snapshot) (delivered : Bool)
    (committed : s.intent.committed = true) :
    (step amount s (.report true delivered)).intent.uncertain = true := by
  simp [step, Hawala.step, committed]

theorem invalid_stutters (amount : Nat) (s : Snapshot) (delivered : Bool) :
    step amount s (.report false delivered) = s := by simp [step]

theorem conflict_permanent (amount : Nat) (s : Snapshot) (e : Event)
    (h : s.conflict = true) : (step amount s e).conflict = true := by
  cases e <;> simp only [step]
  all_goals first | (split <;> simp_all) | exact h

#print axioms protected_unchanged
#print axioms safety_preserved
end Observations
