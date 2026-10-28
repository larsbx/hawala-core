import Std.Tactic

/- Typed evidence SHAPES and consumer gates. The independent signature, mandate,
   custody and finality verifier is an explicit input, NOT proved by this file. -/
namespace FinancialEvidence
abbrev Digest := Fin 32 → UInt8
structure Ref where
  digest : Digest
structure ProposedEvidence where
  manifest : Ref
  customerProposal : Ref
  merchantProposal : Ref
structure CommittedEvidence extends ProposedEvidence where
  preparedL : Ref
  preparedR : Ref
  coordinatorDecision : Ref
  commitAckL : Ref
  commitAckR : Ref
structure ExecutedEvidence extends CommittedEvidence where
  observedL : Ref
  observedR : Ref
structure SettledEvidence extends ExecutedEvidence where
  settledL : Ref
  settledR : Ref
  finalityPolicy : Ref
inductive Progress where
  | proposed | committed | executed | settled
  deriving DecidableEq, Repr
inductive State where
  | proposed (evidence : ProposedEvidence)
  | committed (evidence : CommittedEvidence)
  | executed (evidence : ExecutedEvidence)
  | settled (evidence : SettledEvidence)
  | held (lastProven : Progress) (diagnostic : Ref)
  | aborted (proposal : ProposedEvidence) (abortDecision ackL ackR : Ref)
inductive WorkflowAction where
  | recordProposal | reserveCancelableOrder
  | checkoutFulfill | dispatchPaid | groupBuyRelease | remittanceDelivered
  deriving DecidableEq, Repr

def mayAdvance (independentlyVerified : Bool) (s : State) (a : WorkflowAction) : Bool :=
  independentlyVerified && match s, a with
  | .held .., _ | .aborted .., _ => false
  | _, .recordProposal => true
  | .committed .., .reserveCancelableOrder | .executed .., .reserveCancelableOrder |
      .settled .., .reserveCancelableOrder => true
  | .settled .., _ => true
  | _, _ => false

theorem unverified_cannot_advance (s : State) (a : WorkflowAction) :
    mayAdvance false s a = false := by simp [mayAdvance]
theorem held_cannot_advance (p : Progress) (d : Ref) (a : WorkflowAction) :
    mayAdvance true (.held p d) a = false := by cases a <;> rfl
theorem committed_is_not_paid (e : CommittedEvidence) :
    mayAdvance true (.committed e) .checkoutFulfill = false := rfl
theorem observed_is_not_paid (e : ExecutedEvidence) :
    mayAdvance true (.executed e) .remittanceDelivered = false := rfl
theorem irreversible_requires_settled (v : Bool) (s : State) (a : WorkflowAction)
    (h : a ≠ .recordProposal ∧ a ≠ .reserveCancelableOrder)
    (allowed : mayAdvance v s a = true) :
    ∃ e, s = .settled e := by
  cases s <;> cases a <;> simp_all [mayAdvance]

#print axioms irreversible_requires_settled
end FinancialEvidence
