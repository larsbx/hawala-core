import FinancialEvidence
open FinancialEvidence
namespace FinancialGatesExport
-- Synthetic references for evidence SHAPE tests; no verifier or financial grant.
def ref : Ref := ⟨fun _ => 0⟩
def proposed : ProposedEvidence := ⟨ref, ref, ref⟩
def committed : CommittedEvidence := ⟨proposed, ref, ref, ref, ref, ref⟩
def executed : ExecutedEvidence := ⟨committed, ref, ref⟩
def settled : SettledEvidence := ⟨executed, ref, ref, ref⟩
def states : List (String × State) := [
  ("proposed", .proposed proposed), ("committed", .committed committed),
  ("executed", .executed executed), ("settled", .settled settled),
  ("held", .held .committed ref), ("aborted", .aborted proposed ref ref ref)]
def actions : List (String × WorkflowAction) := [
  ("recordProposal", .recordProposal), ("reserveCancelableOrder", .reserveCancelableOrder),
  ("checkoutFulfill", .checkoutFulfill), ("dispatchPaid", .dispatchPaid),
  ("groupBuyRelease", .groupBuyRelease), ("remittanceDelivered", .remittanceDelivered)]
end FinancialGatesExport
open FinancialGatesExport
def main : IO Unit := do
  let out ← IO.getStdout
  out.putStrLn "verified\tstate\taction\tallowed"
  for verified in [false, true] do
    for (stateName, s) in states do
      for (actionName, a) in actions do
        out.putStrLn (toString verified ++ "\t" ++ stateName ++ "\t" ++ actionName ++ "\t" ++ toString (mayAdvance verified s a))
