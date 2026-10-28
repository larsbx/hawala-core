import Std.Tactic

/- An asynchronous, crash/recovery safety model for ONE immutable attempt.
   Signatures are abstract authenticated messages, not a cryptographic theorem.
   Durable record recovery and effective signing-boundary fencing are assumptions.
   No transition executes money or establishes custody/finality. -/
namespace CrossAuthority
structure State where
  requested : Bool := false
  requestEpoch : Nat := 0
  voteL : Nat := 0 -- 0 undecided, 1 durable prepare, 2 durable refusal
  voteR : Nat := 0
  seenL : Bool := false
  seenR : Bool := false
  decision : Nat := 0 -- 0 pending, 1 commit, 2 abort
  wire : Nat := 0 -- immutable published terminal certificate, delivered separately
  wireEpoch : Nat := 0
  localL : Nat := 0
  localR : Nat := 0
  upC : Bool := true
  upL : Bool := true
  upR : Bool := true
  netL : Bool := true
  netR : Bool := true
  epoch : Nat := 0
  installedL : Bool := false
  installedR : Bool := false
  recoveryWire : Bool := false
  fenceC : Bool := false
  fenceL : Bool := false
  fenceR : Bool := false
  recovered : Bool := false
  deriving DecidableEq, Repr

def initial : State := {}

-- ctx and ae stand for the full manifest binding and the pinned authority epoch.
-- Values other than zero model valid evidence from a DIFFERENT attempt/authority.
inductive Event where
  | prepareL (ctx ae ce : Nat) | prepareR (ctx ae ce : Nat)
  | sendPrepare | refuseL | refuseR | receiveL | receiveR | commit | abort | publish
  | deliverL (ctx ae : Nat) | deliverR (ctx ae : Nat)
  | crashC | restartC | crashL | restartL | crashR | restartR
  | partitionL | healL | partitionR | healR | timeout
  | fenceCoordinator | fenceLeft | fenceRight | recoverLog | publishRecovery | installL | installR | activateL | activateR | recover
  deriving DecidableEq, Repr

def step (s : State) (e : Event) : State := match e with
  | .prepareL ctx ae ce =>
    if ctx = 0 ∧ ae = 0 ∧ ce = s.epoch ∧ s.requested ∧ ce = s.requestEpoch ∧ s.upL ∧ s.netL ∧
        ¬s.fenceL ∧ s.localL = 0 ∧ s.voteL = 0 then {s with voteL := 1} else s
  | .prepareR ctx ae ce =>
    if ctx = 0 ∧ ae = 0 ∧ ce = s.epoch ∧ s.requested ∧ ce = s.requestEpoch ∧ s.upR ∧ s.netR ∧
        ¬s.fenceR ∧ s.localR = 0 ∧ s.voteR = 0 then {s with voteR := 1} else s
  | .sendPrepare => if s.upC ∧ ¬s.fenceC ∧ s.decision = 0 then
      {s with requested := true, requestEpoch := s.epoch} else s
  | .refuseL => if s.upL ∧ s.voteL = 0 then {s with voteL := 2} else s
  | .refuseR => if s.upR ∧ s.voteR = 0 then {s with voteR := 2} else s
  | .receiveL => if s.upC ∧ s.netL ∧ s.voteL = 1 then {s with seenL := true} else s
  | .receiveR => if s.upC ∧ s.netR ∧ s.voteR = 1 then {s with seenR := true} else s
  | .commit => if s.upC ∧ ¬s.fenceC ∧ s.decision = 0 ∧ s.seenL ∧ s.seenR then
      {s with decision := 1} else s
  | .abort => if s.upC ∧ ¬s.fenceC ∧ s.decision = 0 then {s with decision := 2} else s
  | .publish => if s.upC ∧ ¬s.fenceC ∧ s.wire = 0 ∧ s.decision ≠ 0 then
      {s with wire := s.decision, wireEpoch := s.epoch} else s
  -- Messages are retained: repeated delivery models duplication/reordering.
  -- A historical terminal certificate is still evidence after safe succession.
  | .deliverL ctx ae =>
    if ctx = 0 ∧ ae = 0 ∧ s.upL ∧ s.netL ∧ s.wire ≠ 0 ∧ s.wireEpoch ≤ s.epoch then
      {s with localL := s.wire} else s
  | .deliverR ctx ae =>
    if ctx = 0 ∧ ae = 0 ∧ s.upR ∧ s.netR ∧ s.wire ≠ 0 ∧ s.wireEpoch ≤ s.epoch then
      {s with localR := s.wire} else s
  | .crashC => {s with upC := false}
  | .restartC => if ¬s.fenceC then {s with upC := true} else s
  | .crashL => {s with upL := false}
  | .restartL => {s with upL := true}
  | .crashR => {s with upR := false}
  | .restartR => {s with upR := true}
  | .partitionL => {s with netL := false}
  | .healL => {s with netL := true}
  | .partitionR => {s with netR := false}
  | .healR => {s with netR := true}
  | .timeout => s
  -- fenceCoordinator means an EFFECTIVE boundary fence ACK, not a timeout.
  | .fenceCoordinator => {s with fenceC := true, upC := false, recovered := false}
  | .fenceLeft => if s.fenceC ∧ s.upL ∧ s.netL then {s with fenceL := true} else s
  | .fenceRight => if s.fenceC ∧ s.upR ∧ s.netR then {s with fenceR := true} else s
  -- Complete authenticated durable history recovered, including decision absence.
  | .recoverLog => if s.fenceC then {s with recovered := true} else s
  | .publishRecovery => if s.fenceC ∧ s.fenceL ∧ s.fenceR ∧ s.recovered then
      {s with recoveryWire := true} else s
  | .installL => if s.recoveryWire ∧ s.upL ∧ s.netL then {s with installedL := true} else s
  | .installR => if s.recoveryWire ∧ s.upR ∧ s.netR then {s with installedR := true} else s
  | .recover => if s.fenceC ∧ s.fenceL ∧ s.fenceR ∧ s.recovered ∧ s.installedL ∧ s.installedR ∧ s.epoch = 0 then
      {s with epoch := 1, upC := true, fenceC := false} else s
  | .activateL => if s.epoch = 1 ∧ s.installedL ∧ s.upL ∧ s.netL then {s with fenceL := false} else s
  | .activateR => if s.epoch = 1 ∧ s.installedR ∧ s.upR ∧ s.netR then {s with fenceR := false} else s

def Safe (s : State) : Prop :=
  s.voteL ≤ 2 ∧ s.voteR ≤ 2 ∧ s.decision ≤ 2 ∧ s.wire ≤ 2 ∧
  (s.seenL = true → s.voteL = 1) ∧ (s.seenR = true → s.voteR = 1) ∧
  (s.decision = 1 → s.seenL = true ∧ s.seenR = true) ∧
  (s.wire ≠ 0 → s.wire = s.decision) ∧
  (s.localL ≠ 0 → s.localL = s.decision) ∧
  (s.localR ≠ 0 → s.localR = s.decision) ∧
  (s.localL ≠ 0 → s.localL = s.wire) ∧
  (s.localR ≠ 0 → s.localR = s.wire)

theorem initial_safe : Safe initial := by simp [Safe, initial]
theorem step_safe (s : State) (e : Event) (h : Safe s) : Safe (step s e) := by
  rcases h with ⟨vl, vr, d, w, sl, sr, c, wire, ll, lr, lw, rw⟩
  cases e <;> simp only [step]
  all_goals first
    | (split <;> simp_all [Safe] <;> omega)
    | (simp_all [Safe] <;> omega)

inductive Reachable : State → Prop where
  | init : Reachable initial
  | next {s : State} : Reachable s → (e : Event) → Reachable (step s e)

theorem reachable_safe {s : State} (h : Reachable s) : Safe s := by
  induction h with
  | init => exact initial_safe
  | next _ e ih => exact step_safe _ e ih

theorem agreement {s : State} (h : Reachable s) (l : s.localL ≠ 0) (r : s.localR ≠ 0) :
    s.localL = s.localR := by
  rcases reachable_safe h with ⟨_, _, _, _, _, _, _, _, hl, hr, _, _⟩
  exact (hl l).trans (hr r).symm

theorem commit_requires_both {s : State} (h : Reachable s) (c : s.decision = 1) :
    s.voteL = 1 ∧ s.voteR = 1 := by
  rcases reachable_safe h with ⟨_, _, _, _, l, r, prepared, _⟩
  exact ⟨l (prepared c).1, r (prepared c).2⟩

theorem terminal_permanent (s : State) (e : Event) (h : s.decision ≠ 0) :
    (step s e).decision = s.decision := by
  cases e <;> simp only [step]
  all_goals first | (split <;> simp_all) | rfl

def HeldL (s : State) : Prop := s.voteL = 1 ∧ s.localL = 0
def LockedL (s : State) : Prop := s.voteL = 1 ∧ s.localL ≠ 2

theorem held_is_locked {s : State} (h : HeldL s) : LockedL s := by
  simp_all [HeldL, LockedL]
theorem timeout_preserves_hold (s : State) : step s .timeout = s := rfl
theorem coordinator_crash_preserves_hold (s : State) (h : HeldL s) :
    HeldL (step s .crashC) := h
theorem partition_preserves_hold (s : State) (h : HeldL s) :
    HeldL (step s .partitionL) := h

theorem prepared_cannot_unilaterally_abort (s : State) (h : s.voteL = 1) :
    step s .refuseL = s := by simp [step, h]
theorem wrong_attempt_rejected (s : State) (ctx ae ce : Nat) (h : ctx ≠ 0) :
    step s (.prepareL ctx ae ce) = s ∧ step s (.deliverL ctx ae) = s := by
  simp [step, h]
theorem wrong_authority_epoch_rejected (s : State) (ae ce : Nat) (h : ae ≠ 0) :
    step s (.prepareL 0 ae ce) = s ∧ step s (.deliverL 0 ae) = s := by
  simp [step, h]
theorem old_coordinator_cannot_prepare (s : State) (ce : Nat) (h : ce ≠ s.epoch) :
    step s (.prepareL 0 0 ce) = s := by simp [step, h]
theorem incomplete_recovery_holds (s : State)
    (h : ¬(s.fenceC ∧ s.fenceL ∧ s.fenceR ∧ s.recovered ∧ s.installedL ∧ s.installedR ∧ s.epoch = 0)) :
    step s .recover = s := by simp [step, h]
theorem recovery_keeps_terminal_history (s : State) :
    (step s .recover).decision = s.decision ∧
    (step s .recover).wire = s.wire ∧ (step s .recover).voteL = s.voteL ∧
    (step s .recover).voteR = s.voteR := by
  simp only [step]; split <;> simp

theorem local_commit_requires_both {s : State} (h : Reachable s) (c : s.localL = 1) :
    s.voteL = 1 ∧ s.voteR = 1 := by
  rcases reachable_safe h with ⟨_, _, _, _, _, _, _, _, localEvidence, _⟩
  have d : s.decision = 1 := by have := localEvidence (by omega); omega
  exact commit_requires_both h d

theorem local_abort_requires_published_abort {s : State} (h : Reachable s) (a : s.localL = 2) :
    s.wire = 2 ∧ s.decision = 2 := by
  rcases reachable_safe h with ⟨_, _, _, _, _, _, _, _, localEvidence, _, published, _⟩
  have nz : s.localL ≠ 0 := by omega
  exact ⟨by have := published nz; omega, by have := localEvidence nz; omega⟩

theorem abort_fences_commit (s : State) (a : s.decision = 2) :
    (step s .commit).decision = 2 := by
  rw [terminal_permanent s .commit (by omega)]; exact a

def HeldR (s : State) : Prop := s.voteR = 1 ∧ s.localR = 0
def LockedR (s : State) : Prop := s.voteR = 1 ∧ s.localR ≠ 2
theorem held_right_is_locked {s : State} (h : HeldR s) : LockedR s := by
  simp_all [HeldR, LockedR]
theorem coordinator_crash_preserves_hold_right (s : State) (h : HeldR s) :
    HeldR (step s .crashC) := h
theorem partition_preserves_hold_right (s : State) (h : HeldR s) :
    HeldR (step s .partitionR) := h
theorem prepared_right_cannot_unilaterally_abort (s : State) (h : s.voteR = 1) :
    step s .refuseR = s := by simp [step, h]
theorem wrong_attempt_rejected_right (s : State) (ctx ae ce : Nat) (h : ctx ≠ 0) :
    step s (.prepareR ctx ae ce) = s ∧ step s (.deliverR ctx ae) = s := by
  simp [step, h]
theorem wrong_authority_epoch_rejected_right (s : State) (ae ce : Nat) (h : ae ≠ 0) :
    step s (.prepareR 0 ae ce) = s ∧ step s (.deliverR 0 ae) = s := by
  simp [step, h]
theorem old_coordinator_cannot_prepare_right (s : State) (ce : Nat) (h : ce ≠ s.epoch) :
    step s (.prepareR 0 0 ce) = s := by simp [step, h]

#print axioms reachable_safe
#print axioms agreement
#print axioms terminal_permanent
end CrossAuthority
