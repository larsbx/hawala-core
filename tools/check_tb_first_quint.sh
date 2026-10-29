#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
quint_bin="${QUINT_BIN:-formal/node_modules/.bin/quint}"
"$quint_bin" typecheck formal/quint/tigerBeetleFirst.qnt
"$quint_bin" verify formal/quint/tigerBeetleFirst.qnt --backend=tlc --invariant=safety
negative=$(mktemp)
traces=$(mktemp)
trap 'rm -f "$negative" "$traces"' EXIT
for mutation in unsafeMirrorStep unsafeDuplicateStep unsafeForgetStep unsafeAckStep; do
  if "$quint_bin" verify formal/quint/tigerBeetleFirst.qnt --backend=tlc --step="$mutation" --invariant=safety >"$negative" 2>&1; then
    cat "$negative"
    echo "ERROR: unsafe TB-first mutation $mutation escaped"
    exit 1
  fi
  grep -Fq '[violation]' "$negative"
  grep -Fq 'Invariant q_inv is violated' "$negative"
  echo "PASS: TLC rejected $mutation with an invariant counterexample"
done
"$quint_bin" test formal/quint/tigerBeetleFirst.qnt --backend=typescript --seed=1
"$quint_bin" test formal/quint/tigerBeetleFirstConformance.qnt --backend=typescript --seed=1 >"$traces"
test "$(grep -Ec '^    ok tbFirstEdge[0-9]+Test passed 1 test\(s\)$' "$traces")" -eq 1700
# Quint also executes the five witnesses in the imported source module.
grep -Eq '^  1705 passing' "$traces"
tail -3 "$traces"
python3 tools/generate_tb_first_certificate.py
