#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
quint_bin="${QUINT_BIN:-formal/node_modules/.bin/quint}"
"$quint_bin" typecheck formal/quint/coordinatedAdmission.qnt
"$quint_bin" verify formal/quint/coordinatedAdmission.qnt --backend=tlc --invariant=safety
negative_log=$(mktemp)
trace_log=$(mktemp)
trap 'rm "$negative_log" "$trace_log"' EXIT
for mutation in unsafeLegacyStep unsafeSplitStep unsafeAbortStep; do
  if "$quint_bin" verify formal/quint/coordinatedAdmission.qnt --backend=tlc --step="$mutation" --invariant=safety >"$negative_log" 2>&1; then
    cat "$negative_log"
    echo "ERROR: unsafe admission coordination mutation $mutation was not detected"
    exit 1
  fi
  cat "$negative_log"
  grep -Fq '[violation]' "$negative_log"
done
"$quint_bin" test formal/quint/coordinatedConformance.qnt --backend=typescript --seed=1 | tee "$trace_log"
grep -Eq '^  162 passing' "$trace_log"
