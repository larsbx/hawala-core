#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
quint_bin="${QUINT_BIN:-formal/node_modules/.bin/quint}"
"$quint_bin" typecheck formal/quint/endpointOrchestration.qnt
"$quint_bin" verify formal/quint/endpointOrchestration.qnt --backend=tlc --invariant=safety
negative=$(mktemp)
traces=$(mktemp)
trap 'rm "$negative" "$traces"' EXIT
for mutation in unsafeReceivedStep unsafeUnboundStep unsafeForgetStep unsafeRetryStep; do
  if "$quint_bin" verify formal/quint/endpointOrchestration.qnt --backend=tlc --step="$mutation" --invariant=safety >"$negative" 2>&1; then
    cat "$negative"
    echo "ERROR: unsafe endpoint mutation escaped"
    exit 1
  fi
  grep -Fq '[violation]' "$negative"
  grep -Fq 'Invariant q_inv is violated' "$negative"
done
"$quint_bin" test formal/quint/endpointOrchestrationConformance.qnt --backend=typescript --seed=1 | tee "$traces"
grep -Eq '^  1155 passing' "$traces"
python3 tools/check_endpoint_counterexamples.py

node tools/check_test_endpoint_vectors.mjs
