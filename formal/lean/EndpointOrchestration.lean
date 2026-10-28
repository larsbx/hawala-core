import Std.Tactic

/- Durable TEST endpoint/outbox protocol. `valid` abstracts independently checked
   request/reply signature, scope, epoch and call binding. No custody/finality. -/
namespace EndpointOrchestration
structure State where
  stage : Nat := 0 -- absent, proposed, prepared, TEST committed intent
  effect : Nat := 0 -- independently queried complete local lab effects
  target : Nat := 0
  pending : Bool := false
  commits : Nat := 0
  client : Nat := 0
  desired : Nat := 0
  outbox : Bool := false
  held : Bool := false
  paid : Bool := false
  deriving DecidableEq, Repr
inductive Event where
  | queue (n : Nat) | admit (n : Nat) (valid fresh : Bool)
  | effect | finish | reply (valid : Bool)
  | timeout | restart | replay
  deriving DecidableEq, Repr

def initial : State := {}
def step (s : State) (e : Event) : State := match e with
  | .queue n => if ¬s.outbox ∧ n = s.client + 1 ∧ n ≤ 3 then
      {s with desired := n, outbox := true, held := true} else s
  | .admit n valid fresh => if valid ∧ fresh ∧ ¬s.pending ∧ n = s.stage + 1 ∧ n ≤ 3 then
      {s with target := n, pending := true} else s
  | .effect => if s.pending ∧ s.effect < s.target then
      {s with effect := s.target, commits := s.commits + if s.target = 3 then 1 else 0} else s
  | .finish => if s.pending ∧ s.effect = s.target then
      {s with stage := s.target, pending := false} else s
  | .reply valid => if valid ∧ s.outbox ∧ ¬s.pending ∧ s.stage ≥ s.desired then
      {s with client := s.stage, outbox := false, held := false} else {s with held := true}
  | .timeout => {s with held := true}
  | .restart | .replay => s

def Safe (s : State) : Prop :=
  s.stage ≤ 3 ∧ s.effect ≤ 3 ∧ s.stage ≤ s.effect ∧
  s.effect ≤ s.stage + 1 ∧ (s.effect > s.stage → s.pending = true) ∧
  (s.pending = true → s.target = s.stage + 1 ∧ s.target ≤ 3) ∧
  s.commits = (if s.effect = 3 then 1 else 0) ∧ s.client ≤ s.stage ∧
  s.desired ≤ 3 ∧ s.paid = false

theorem initial_safe : Safe initial := by simp [Safe, initial]
theorem step_safe (s : State) (e : Event) (h : Safe s) : Safe (step s e) := by
  rcases h with ⟨a,b,c,d,f,g,k,l,m,p⟩
  cases e <;> simp only [step]
  all_goals first | split | skip
  all_goals by_cases hp : s.pending = true <;> by_cases he : s.effect = 3 <;> by_cases ht : s.target = 3 <;>
    simp_all [Safe] <;> omega
inductive Reachable : State → Prop where
  | init : Reachable initial
  | next {s : State} : Reachable s → (e : Event) → Reachable (step s e)
theorem reachable_safe {s : State} (h : Reachable s) : Safe s := by
  induction h with
  | init => exact initial_safe
  | next _ e ih => exact step_safe _ e ih
theorem at_most_one_commit {s : State} (h : Reachable s) : s.commits ≤ 1 := by
  rcases reachable_safe h with ⟨_,_,_,_,_,_,count,_⟩
  by_cases he : s.effect = 3 <;> simp_all <;> omega
theorem client_never_invents_progress {s : State} (h : Reachable s) : s.client ≤ s.stage :=
  (reachable_safe h).2.2.2.2.2.2.2.1
theorem never_paid {s : State} (h : Reachable s) : s.paid = false :=
  (reachable_safe h).2.2.2.2.2.2.2.2.2
theorem invalid_request_stutters (s : State) (n : Nat) (fresh : Bool) :
    step s (.admit n false fresh) = s := by simp [step]
theorem duplicate_request_stutters (s : State) (n : Nat) (valid : Bool) :
    step s (.admit n valid false) = s := by simp [step]
theorem pending_blocks_replacement (s : State) (n : Nat) (h : s.pending = true) :
    step s (.admit n true true) = s := by simp [step,h]
theorem timeout_preserves_effects (s : State) :
    (step s .timeout).effect = s.effect ∧ (step s .timeout).commits = s.commits ∧
    (step s .timeout).outbox = s.outbox := by simp [step]
theorem unresolved_cannot_complete (s : State) (h : s.pending = true) (h' : s.effect ≠ s.target) :
    step s .finish = s := by simp [step,h,h']
#print axioms reachable_safe
#print axioms at_most_one_commit
end EndpointOrchestration
