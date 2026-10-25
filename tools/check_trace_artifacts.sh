#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../formal/lean"
trace_tmp=$(mktemp -d)
trap 'rm "$trace_tmp/lifecycle.tsv" "$trace_tmp/conformance.qnt"; rmdir "$trace_tmp"' EXIT
lake env lean --run Export.lean > "$trace_tmp/lifecycle.tsv"
lake env lean --run Export.lean quint > "$trace_tmp/conformance.qnt"
diff -u ../traces/lifecycle.tsv "$trace_tmp/lifecycle.tsv"
diff -u ../quint/conformance.qnt "$trace_tmp/conformance.qnt"
lake env lean --run ExportObservations.lean > "$trace_tmp/lifecycle.tsv"
lake env lean --run ExportObservations.lean quint > "$trace_tmp/conformance.qnt"
diff -u ../traces/observations.tsv "$trace_tmp/lifecycle.tsv"
diff -u ../quint/observationConformance.qnt "$trace_tmp/conformance.qnt"
