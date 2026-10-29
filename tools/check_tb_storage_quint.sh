#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
quint_bin="${QUINT_BIN:-formal/node_modules/.bin/quint}"
"$quint_bin" typecheck formal/quint/tigerBeetleStorage.qnt
"$quint_bin" verify formal/quint/tigerBeetleStorage.qnt --backend=tlc --invariant=safety
"$quint_bin" test formal/quint/tigerBeetleStorage.qnt --backend=typescript --seed=1
negative=$(mktemp)
trap 'rm -f "$negative"' EXIT
for mutation in unsafeSplitStep unsafeRepeatStep; do
  if "$quint_bin" verify formal/quint/tigerBeetleStorage.qnt --backend=tlc --step="$mutation" --invariant=safety >"$negative" 2>&1; then
    cat "$negative"; echo "ERROR: unsafe storage step escaped"; exit 1
  fi
  grep -Fq '[violation]' "$negative"
  grep -Fq 'Invariant q_inv is violated' "$negative"
  echo "PASS: TLC found intended counterexample for $mutation"
done
