#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# pons_bench.sh — Pons's benchmark harness.
#
# This is the shell harness for the benchmark contract defined by
# hyperpolymath/proven-tests-and-benches (benchmarks/Benchmark.idr, and the
# `proven-bench` JSON v1 document it writes). The gate that reads this
# document is the framework's own
# `scripts/proven-bench/check-bench-regression.sh`, vendored byte-for-byte
# at the pinned revision in scripts/proven-bench/PIN.
#
# THE MEASUREMENT DISCIPLINE, COPIED FROM THE FRAMEWORK
# -----------------------------------------------------
#   * Each workload runs REPS times; every sample is recorded and the MEDIAN
#     is the headline number. A hosted runner is noisy; a single sample is
#     not a measurement.
#   * The workload set is FROZEN. Baselines are per-workload, so changing a
#     workload's definition invalidates its history and requires a new
#     baseline (the estate taxonomy: baselines are versioned).
#   * Every iteration is checked against a PREDICATE, and the workload's
#     `checksum` is true only when every iteration of every sample held. A
#     workload whose predicate failed is measuring nothing, and the harness
#     exits non-zero after writing the artefact, so the red survives.
#   * The JSON is written with numbers in it. Numbers ship with artefacts,
#     never merely asserted.
#
# USAGE
#   benches/pons_bench.sh [--json PATH]
#
#   --json PATH   write the proven-bench JSON v1 document to PATH
#                 (default: build/bench-current.json)
#
# EXIT CODES
#   0  every workload's checksum held
#   2  a workload's predicate failed, the binary would not build, or the
#      artefact could not be written
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

JSON_OUT="build/bench-current.json"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      JSON_OUT="${2:-}"
      [ -n "$JSON_OUT" ] || { echo "bench: --json needs a path" >&2; exit 2; }
      shift 2
      ;;
    -h|--help)
      sed -n '2,30p' "$0"
      exit 0
      ;;
    *)
      echo "bench: unknown argument '$1'" >&2
      exit 2
      ;;
  esac
done

REPS=5
BIN="target/release/pons"

echo "bench: building pons-cli (release)"
if ! cargo build --release --locked -p pons-cli; then
  echo "bench: could not build pons-cli; no measurement is possible" >&2
  exit 2
fi
if [ ! -x "$BIN" ]; then
  echo "bench: $BIN is not executable; no measurement is possible" >&2
  exit 2
fi

# now_ns — nanoseconds from the monotonic-ish wall clock. `date +%s%N` is
# the finest timer available without a toolchain dependency, and every
# sample here is milliseconds-scale, so the resolution is not the limit.
now_ns() {
  date +%s%N
}

# The workload set. FROZEN. Each entry is:
#   name|iterations|shell snippet|expected exit code|output predicate
#
# The predicate is written as a shell snippet that must succeed; a workload
# whose output changes shape fails its predicate, and its timings are then
# measurements of a broken workload rather than of Pons.
#
# The exit codes are ADR-0005's contract: `pons scan` exits 0 even when it
# reports findings, and 2 only for an operational error at the root.
workloads=(
  "scan-fixtures|$REPS|\"\$BIN\" scan fixtures --format json|0|grep -q '\"schema_version\"'"
  "scan-single-file|$REPS|\"\$BIN\" scan fixtures/unreachable-after-jump/positive/throw.php --format json|0|grep -q '\"schema_version\"'"
  "catalogue|$REPS|\"\$BIN\" catalogue|0|grep -q 'unreachable-after-jump'"
  "startup|$REPS|\"\$BIN\" --version|0|grep -q '^pons '"
)

results_json='[]'
human_lines=''
checksum_failed=''

for workload in "${workloads[@]}"; do
  IFS='|' read -r name iterations snippet expected predicate <<<"$workload"
  samples=''
  checksum=true
  for _ in $(seq 1 "$iterations"); do
    start="$(now_ns)"
    set +e
    output="$(eval "$snippet" 2>&1)"
    rc=$?
    set -e
    end="$(now_ns)"
    elapsed=$(( end - start ))
    [ "$elapsed" -gt 0 ] || elapsed=1
    samples="${samples}${samples:+,}${elapsed}"
    if [ "$rc" != "$expected" ]; then
      checksum=false
      echo "bench: $name: iteration exited $rc, expected $expected" >&2
      break
    fi
    if ! eval "$predicate" <<<"$output"; then
      checksum=false
      echo "bench: $name: the output did not satisfy its predicate" >&2
      break
    fi
  done

  if [ "$checksum" = true ]; then
    # The median of an odd-length sample list is the middle value.
    median="$(tr ',' '\n' <<<"$samples" | sort -n | awk -v n="$iterations" 'NR==int((n+1)/2)')"
  else
    median=1
    checksum_failed="${checksum_failed} ${name}"
  fi
  [ -n "$median" ] || median=1

  echo "bench: ${name}: ${iterations} iters, ${median} ns median of ${iterations} samples [checksum=${checksum}]"
  human_lines="${human_lines}${name}: ${iterations} iters, ${median} ns median of ${iterations} samples [checksum=${checksum}]"$'\n'

  results_json="$(jq -c \
    --arg name "$name" \
    --argjson iterations "$iterations" \
    --argjson samples "[${samples}]" \
    --argjson median "$median" \
    --argjson checksum "$checksum" \
    '. + [{name: $name, iterations: $iterations, samples_ns: $samples, median_ns: $median, checksum: $checksum}]' \
    <<<"$results_json")"
done

COMMIT="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
RUNNER="$(uname -s)-$(uname -m)"

mkdir -p "$(dirname "$JSON_OUT")"
jq -n \
  --argjson results "$results_json" \
  --arg commit "$COMMIT" \
  --arg runner "$RUNNER" \
  --argjson reps "$REPS" \
  '{
     schema_version: 1,
     suite: "proven-bench",
     reps: $reps,
     commit: $commit,
     runner: $runner,
     results: $results
   }' > "$JSON_OUT"

echo "bench: wrote $JSON_OUT"

if [ -n "$checksum_failed" ]; then
  echo "bench: CHECKSUM FAILED for${checksum_failed} - a workload predicate returned false; its timings measure a broken workload" >&2
  exit 2
fi
echo "bench: checksum OK - every iteration of every workload held"
exit 0
