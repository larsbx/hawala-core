#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
quint_bin="${QUINT_BIN:-formal/node_modules/.bin/quint}"
trace_log=$(mktemp)
trap 'rm "$trace_log"' EXIT
"$quint_bin" test formal/quint/conformance.qnt --backend=typescript --seed=1 | tee "$trace_log"
# Quint can exit successfully if no test names are selected. Require all 32.
grep -Eq '^  32 passing' "$trace_log"
