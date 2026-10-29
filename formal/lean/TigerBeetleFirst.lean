import Std.Tactic

/- Abstract TB financial commit and two independent complete-prefix sinks.
   `valid` assumes the trusted adapter has checked authorization and exact body.
   Neither native storage, signatures, completeness nor fencing is proved here. -/
namespace TigerBeetleFirst

structure State where
  committed : Nat := 0
  effects : Nat := 0
  sinkA : Nat := 0
  sinkB : Nat := 0
  acknowledged : Nat := 0
  deriving DecidableEq, Repr

inductive Event where
  | accept (operation : Nat) (valid : Bool)
  | persistA (next : Nat)
  | persistB (next : Nat)
  | acknowledge (operation : Nat) (valid : Bool)
  | timeout | crash | replay
  deriving DecidableEq, Repr

def initial : State := {}

def step (s : State) (e : Event) : State := match e with
  | .accept n valid => if valid ∧ n = s.committed + 1 then
      {s with committed := n, effects := s.effects + 1} else s
  | .persistA n => if n = s.sinkA + 1 ∧ n ≤ s.committed then
      {s with sinkA := n} else s
  | .persistB n => if n = s.sinkB + 1 ∧ n ≤ s.committed then
      {s with sinkB := n} else s
  | .acknowledge n valid => if valid ∧ n ≤ s.committed then
      {s with acknowledged := max s.acknowledged n} else s
  | .timeout | .crash | .replay => s

def Safe (s : State) : Prop :=
  s.effects = s.committed ∧ s.sinkA ≤ s.committed ∧
  s.sinkB ≤ s.committed ∧ s.acknowledged ≤ s.committed

theorem initial_safe : Safe initial := by simp [Safe, initial]

theorem step_safe (s : State) (e : Event) (h : Safe s) : Safe (step s e) := by
  rcases h with ⟨he, ha, hb, hk⟩
  cases e <;> simp only [step]
  all_goals first | split | skip
  all_goals simp_all [Safe] <;> omega

inductive Reachable : State → Prop where
  | init : Reachable initial
  | next {s : State} : Reachable s → (e : Event) → Reachable (step s e)

theorem reachable_safe {s : State} (h : Reachable s) : Safe s := by
  induction h with
  | init => exact initial_safe
  | next _ e ih => exact step_safe _ e ih

theorem sinks_cannot_invent_commit {s : State} (h : Reachable s) :
    s.sinkA ≤ s.committed ∧ s.sinkB ≤ s.committed := by
  exact ⟨(reachable_safe h).2.1, (reachable_safe h).2.2.1⟩

theorem duplicate_commit_stutters (s : State) (n : Nat) (h : n ≤ s.committed) :
    step s (.accept n true) = s := by
  simp [step, show n ≠ s.committed + 1 by omega]

theorem invalid_body_stutters (s : State) (n : Nat) :
    step s (.accept n false) = s := by simp [step]

theorem commit_without_sink_ack (s : State) :
    (step s (.accept (s.committed + 1) true)).committed = s.committed + 1 ∧
    (step s (.accept (s.committed + 1) true)).sinkA = s.sinkA ∧
    (step s (.accept (s.committed + 1) true)).sinkB = s.sinkB := by simp [step]

theorem sink_failure_cannot_undo_commit (s : State) :
    step s .crash = s ∧ step s .timeout = s ∧ step s .replay = s := by simp [step]

theorem sinkA_independent_of_sinkB (s : State) (n : Nat) :
    (step s (.persistA n)).sinkB = s.sinkB := by
  unfold step
  split <;> rfl

theorem sinkB_independent_of_sinkA (s : State) (n : Nat) :
    (step s (.persistB n)).sinkA = s.sinkA := by
  unfold step
  split <;> rfl

theorem no_sink_gap (s : State) (n : Nat) (h : n ≠ s.sinkA + 1) :
    step s (.persistA n) = s := by simp [step, h]

theorem commit_monotone (s : State) (e : Event) :
    s.committed ≤ (step s e).committed := by
  cases e <;> simp only [step]
  all_goals first | split | skip
  all_goals simp_all <;> omega

#print axioms reachable_safe
#print axioms commit_monotone
#print axioms duplicate_commit_stutters
end TigerBeetleFirst
