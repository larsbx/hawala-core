#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
quint_bin="${QUINT_BIN:-formal/node_modules/.bin/quint}"
"$quint_bin" typecheck formal/quint/authority.qnt
"$quint_bin" verify formal/quint/authority.qnt --backend=tlc --invariant=safety
negative_log=$(mktemp)
trap 'rm "$negative_log"' EXIT
if "$quint_bin" verify formal/quint/authority.qnt --backend=tlc --step=unsafeStep --invariant=safety >"$negative_log" 2>&1; then
  cat "$negative_log"
  echo 'ERROR: duplicate-claim mutation was not detected'
  exit 1
fi
cat "$negative_log"
# A missing tool, parse error or failed download is NOT a passing negative control.
if ! rg -q '\[violation\]' "$negative_log"; then
  echo 'ERROR: expected a model-checker counterexample'
  exit 1
fi
echo 'PASS: TLC rejected the deliberately unsafe duplicate-claim transition'
