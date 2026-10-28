#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
quint_bin="${QUINT_BIN:-formal/node_modules/.bin/quint}"
"$quint_bin" typecheck formal/quint/crossAuthority.qnt
"$quint_bin" verify formal/quint/crossAuthority.qnt --backend=tlc --invariant=safety,agreement
negative_log=$(mktemp)
trace_log=$(mktemp)
trap 'rm "$negative_log" "$trace_log"' EXIT
for mutation in unsafeTimeoutStep unsafeSingleStep unsafeRecoveryStep unsafeReplayStep unsafeZombieStep; do
  if "$quint_bin" verify formal/quint/crossAuthority.qnt --backend=tlc --step="$mutation" --invariant=safety >"$negative_log" 2>&1; then
    cat "$negative_log"
    echo "ERROR: unsafe cross-authority mutation $mutation was not detected"
    exit 1
  fi
  cat "$negative_log"
  # Parse/type/backend failures MUST NOT count as successful negative controls.
  grep -Fq '[violation]' "$negative_log"
  grep -Fq 'Invariant q_inv is violated' "$negative_log"
done
"$quint_bin" test formal/quint/crossAuthorityConformance.qnt --backend=typescript --seed=1 | tee "$trace_log"
grep -Eq '^  232 passing' "$trace_log"
node tools/check_cross_authority_evidence.mjs
python3 tools/check_cross_authority_counterexamples.py
