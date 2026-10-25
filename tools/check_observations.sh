#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
quint_bin="${QUINT_BIN:-formal/node_modules/.bin/quint}"
"$quint_bin" typecheck formal/quint/observations.qnt
"$quint_bin" verify formal/quint/observations.qnt --backend=tlc --invariant=safety
negative_log=$(mktemp)
trace_log=$(mktemp)
trap 'rm "$negative_log" "$trace_log"' EXIT
if "$quint_bin" verify formal/quint/observations.qnt --backend=tlc --step=unsafeStep --invariant=safety >"$negative_log" 2>&1; then
  cat "$negative_log"
  echo 'ERROR: unsafe report cleared hold without detection'
  exit 1
fi
cat "$negative_log"
grep -Fq '[violation]' "$negative_log"
"$quint_bin" test formal/quint/observationConformance.qnt --backend=typescript --seed=1 | tee "$trace_log"
grep -Eq '^  105 passing' "$trace_log"
