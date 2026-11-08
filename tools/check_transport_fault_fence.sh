#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
quint_bin="${QUINT_BIN:-formal/node_modules/.bin/quint}"
"$quint_bin" typecheck formal/quint/transportFaultFence.qnt
"$quint_bin" verify formal/quint/transportFaultFence.qnt --backend=tlc --invariant=safety
negative_log=$(mktemp)
trap 'rm -f "$negative_log"' EXIT
for mutant in unsafeRetryStep unsafePrematureAckStep unsafeTimeoutPaysStep; do
  if "$quint_bin" verify formal/quint/transportFaultFence.qnt --backend=tlc --step="$mutant" --invariant=safety >"$negative_log" 2>&1; then
    cat "$negative_log"
    echo "ERROR: $mutant escaped the transport fence"
    exit 1
  fi
  grep -Fq '[violation]' "$negative_log"
  grep -Fq 'Invariant q_inv is violated' "$negative_log"
done
