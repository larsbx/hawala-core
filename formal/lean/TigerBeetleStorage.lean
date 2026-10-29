import Std.Tactic

/- One-operation storage obligations. `checked` abstracts exact signature,
   context, original-plan and complete-prefix checks; it does not prove crypto,
   transport authentication, SQLite or the SDK. Staging is volatile transaction
   state. Durable rows and cursor change only at one SQL COMMIT. -/
namespace TigerBeetleStorage
structure State where
  retained : Bool := false
  committed : Bool := false
  effects : Nat := 0
  rowsA : Bool := false
  cursorA : Bool := false
  stageA : Bool := false
  rowsB : Bool := false
  cursorB : Bool := false
  stageB : Bool := false
  deriving DecidableEq, Repr
inductive Event where
  | retain (checked : Bool)
  | native (original : Bool)
  | stageA (checked : Bool)
  | stageB (checked : Bool)
  | commitA | commitB | crash | unavailable | lostAck
  deriving DecidableEq, Repr

def step (s : State) (e : Event) : State := match e with
  | .retain ok => if ok then {s with retained := true} else s
  | .native original => if s.retained && original && !s.committed then
      {s with committed := true, effects := s.effects + 1} else s
  | .stageA checked => if s.committed && checked then {s with stageA := true} else s
  | .stageB checked => if s.committed && checked then {s with stageB := true} else s
  | .commitA => if s.stageA then {s with rowsA := true, cursorA := true, stageA := false} else s
  | .commitB => if s.stageB then {s with rowsB := true, cursorB := true, stageB := false} else s
  | .crash => {s with stageA := false, stageB := false}
  | .unavailable | .lostAck => s

def Safe (s : State) : Prop :=
  s.effects = (if s.committed then 1 else 0) ∧ s.rowsA = s.cursorA ∧
  s.rowsB = s.cursorB ∧ (s.rowsA = true → s.committed = true) ∧
  (s.rowsB = true → s.committed = true) ∧
  (s.stageA = true → s.committed = true) ∧ (s.stageB = true → s.committed = true)

theorem initial_safe : Safe {} := by simp [Safe]

theorem step_safe (s : State) (e : Event) (h : Safe s) : Safe (step s e) := by
  rcases h with ⟨he, ha, hb, hra, hrb, hsa, hsb⟩
  cases e <;> simp only [step]
  all_goals first | split | skip
  all_goals simp_all [Safe] <;> omega

theorem retry_original_only (s : State) : step s (.native false) = s := by simp [step]
theorem committed_retry_stutters (s : State) (h : s.committed = true) :
    step s (.native true) = s := by simp [step, h]
theorem downstream_failure_keeps_financial_commit (s : State) :
    (step s .crash).committed = s.committed ∧
    (step s .unavailable).effects = s.effects ∧ (step s .lostAck).effects = s.effects := by simp [step]
theorem crash_before_sql_commit_keeps_durable_state (s : State) :
    (step s .crash).rowsA = s.rowsA ∧ (step s .crash).cursorA = s.cursorA := by simp [step]
theorem independent_destination (s : State) :
    (step s .commitA).cursorB = s.cursorB := by
  cases h : s.stageA <;> simp [step, h]
theorem changed_or_partial_evidence_cannot_stage (s : State) :
    step s (.stageA false) = s := by simp [step]
#print axioms step_safe
end TigerBeetleStorage
