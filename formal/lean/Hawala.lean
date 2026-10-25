import Std.Tactic

/- Single credit lot and permanent attempt. This is an abstract transition
   system, not a proof of the Rust, SQLite, cryptography or real custody. -/
namespace Hawala
structure State where
  available : Nat
  reserved : Nat
  committed : Bool
  nonce : Bool
  claimed : Bool
  uncertain : Bool
  claims : Nat
  expired : Bool
  deriving DecidableEq, Repr

inductive Event where
  | admit (authorized : Bool)
  | claim (currentScope : Bool)
  | hold
  | expire
  | crash
  | replay
  | reject
  deriving DecidableEq, Repr

def initial (funding : Nat) : State :=
  ⟨funding, 0, false, false, false, false, 0, false⟩

def step (amount : Nat) (s : State) (e : Event) : State :=
  match e with
  | .admit authorized =>
    if authorized = true ∧ s.committed = false ∧ amount ≤ s.available then
      {s with available := s.available - amount, reserved := s.reserved + amount,
              committed := true, nonce := true}
    else s
  | .claim currentScope =>
    if currentScope = true ∧ s.committed = true ∧ s.claimed = false ∧ s.uncertain = false then
      {s with claimed := true, uncertain := true, claims := s.claims + 1}
    else s
  | .expire =>
    if s.committed = true ∧ s.claimed = false ∧ s.uncertain = false then
      {s with expired := true, uncertain := true}
    else s
  | .hold => if s.committed then {s with uncertain := true} else s
  | .crash | .replay | .reject => s

def Safe (funding : Nat) (s : State) : Prop :=
  s.available + s.reserved = funding ∧
  s.nonce = s.committed ∧
  s.claims ≤ 1 ∧
  (s.claimed = false → s.claims = 0) ∧
  (s.claimed = true → s.uncertain = true ∧ s.committed = true) ∧
  (s.expired = true → s.uncertain = true ∧ s.committed = true ∧ s.claimed = false)

theorem initial_safe (funding : Nat) : Safe funding (initial funding) := by
  simp [Safe, initial]

theorem step_safe (funding amount : Nat) (s : State) (e : Event)
    (h : Safe funding s) : Safe funding (step amount s e) := by
  rcases h with ⟨balance, nonce, once, zero, held, expiry⟩
  cases e <;> simp only [step]
  all_goals first
    | (split <;> simp_all [Safe] <;> omega)
    | (simp_all [Safe])

inductive Reachable (funding amount : Nat) : State → Prop where
  | init : Reachable funding amount (initial funding)
  | next {s : State} : Reachable funding amount s →
      (e : Event) → Reachable funding amount (step amount s e)

theorem reachable_safe {funding amount : Nat} {s : State}
    (h : Reachable funding amount s) : Safe funding s := by
  induction h with
  | init => exact initial_safe funding
  | next _ e ih => exact step_safe funding amount _ e ih

theorem accounting_conservation {funding amount : Nat} {s : State}
    (h : Reachable funding amount s) : s.available + s.reserved = funding :=
  (reachable_safe h).1

theorem claim_at_most_once {funding amount : Nat} {s : State}
    (h : Reachable funding amount s) : s.claims ≤ 1 :=
  (reachable_safe h).2.2.1

theorem atomic_nonce_and_commit {funding amount : Nat} {s : State}
    (h : Reachable funding amount s) : s.nonce = s.committed :=
  (reachable_safe h).2.1

theorem uncertainty_permanent (amount : Nat) (s : State) (e : Event)
    (h : s.uncertain = true) : (step amount s e).uncertain = true := by
  cases e <;> simp only [step]
  all_goals first | (split <;> simp_all) | exact h

theorem no_reclaim_after_uncertainty (amount : Nat) (s : State) (scope : Bool)
    (h : s.uncertain = true) : step amount s (.claim scope) = s := by
  simp [step, h]

theorem expiry_permanent (amount : Nat) (s : State) (e : Event)
    (h : s.expired = true) : (step amount s e).expired = true := by
  cases e <;> simp only [step]
  all_goals first | (split <;> simp_all) | exact h

theorem no_claim_after_expiry {funding amount : Nat} {s : State} (scope : Bool)
    (h : Reachable funding amount s) (expired : s.expired = true) :
    step amount s (.claim scope) = s := by
  have fence := (reachable_safe h).2.2.2.2.2 expired
  exact no_reclaim_after_uncertainty amount s scope fence.1

#print axioms reachable_safe
#print axioms uncertainty_permanent
end Hawala
