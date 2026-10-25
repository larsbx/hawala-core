#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
quint_bin="${QUINT_BIN:-formal/node_modules/.bin/quint}"
"$quint_bin" typecheck formal/quint/authority.qnt
"$quint_bin" verify formal/quint/authority.qnt --backend=tlc --invariant=safety
negative_log=$(mktemp)
trap 'rm "$negative_log"' EXIT
for mutation in unsafeStep unsafeExpiryStep unsafeFenceStep; do
  if "$quint_bin" verify formal/quint/authority.qnt --backend=tlc --step="$mutation" --invariant=safety >"$negative_log" 2>&1; then
    cat "$negative_log"
    echo "ERROR: unsafe mutation $mutation was not detected"
    exit 1
  fi
  cat "$negative_log"
  # Infrastructure failures are not valid negative controls.
  if ! grep -Fq '[violation]' "$negative_log"; then
    echo "ERROR: expected a counterexample for $mutation"
    exit 1
  fi
  echo "PASS: TLC rejected $mutation"
done
