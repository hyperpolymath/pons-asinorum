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
# WHY THE PREDICATES CHECK THE FINDING, NOT THE ENVELOPE
# -------------------------------------------------------
# A predicate that only asks "does the output contain a schema_version?" is
# satisfied by an empty report. The scan workloads therefore require the
# finding the positive fixture MUST produce: `throw.php` is the positive
# control for the `unreachable-after-jump` rule, and
# crates/pons-rules/tests/falsifier.rs asserts that every positive fixture
# produces a finding from its own rule. A scan that reports nothing is
# therefore a BROKEN SCAN, and the harness must say so rather than record a
# green timing for it.
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
      # The header comment is the usage text; print it, up to the code.
      awk 'NR > 1 && /^set -uo/ { exit } NR > 1 { print }' "$0"
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

# ---------------------------------------------------------------------------
# The workload set. FROZEN. Each entry is:
#
#   name|iterations|expected exit code|runner|checker
#
# The runner and the checker are NAMES, dispatched through the explicit
# allowlists below. They are not shell snippets, and nothing here is
# `eval`ed: a workload string can only ever name a command and a predicate
# that exist in this file, so a string in the table can never become shell
# code. (The exit codes are ADR-0005's contract: `pons scan` exits 0 even
# when it reports findings, and 2 only for an operational error at the
# root.)
# ---------------------------------------------------------------------------
workloads=(
  "scan-fixtures|$REPS|0|run_scan_fixtures|check_scan_fixtures"
  "scan-single-file|$REPS|0|run_scan_single_file|check_scan_single_file"
  "catalogue|$REPS|0|run_catalogue|check_catalogue"
  "startup|$REPS|0|run_startup|check_startup"
)

# The runners. Each writes the workload's output to STDOUT; diagnostics go
# to stderr and are not merged into the output, because the checkers parse
# the output as JSON.
run_scan_fixtures() {
  "$BIN" scan fixtures --format json
}

run_scan_single_file() {
  "$BIN" scan fixtures/unreachable-after-jump/positive/throw.php --format json
}

run_catalogue() {
  "$BIN" catalogue
}

run_startup() {
  "$BIN" --version
}

# expect_finding JSON RULE PATH_SUFFIX — the envelope must carry a finding
# for RULE whose location names the file whose path ends with PATH_SUFFIX.
# The path is matched by SUFFIX because the scan root differs between the
# two scan workloads: one scans a directory, the other a single file.
expect_finding() {
  local json="$1" rule="$2" suffix="$3"
  jq -e --arg rule "$rule" --arg suffix "$suffix" '
    [ .findings[]?
      | select(.rule_id == $rule)
      | select((.location.file // "") | endswith($suffix)) ]
    | length > 0
  ' <<<"$json" >/dev/null 2>&1
}

# The checkers. Each returns 0 when the output is what the workload claims
# to measure, and non-zero otherwise.
check_scan_fixtures() {
  expect_finding "$1" "unreachable-after-jump" "positive/throw.php"
}

check_scan_single_file() {
  expect_finding "$1" "unreachable-after-jump" "throw.php"
}

check_catalogue() {
  grep -q 'unreachable-after-jump' <<<"$1"
}

check_startup() {
  grep -q '^pons ' <<<"$1"
}

# The dispatch. An unknown name is an error, not a silent no-op: a workload
# whose runner or checker does not exist would otherwise measure nothing.
run_workload() {
  case "$1" in
    run_scan_fixtures) run_scan_fixtures ;;
    run_scan_single_file) run_scan_single_file ;;
    run_catalogue) run_catalogue ;;
    run_startup) run_startup ;;
    *) echo "bench: unknown workload runner '$1'" >&2; return 2 ;;
  esac
}

run_check() {
  case "$1" in
    check_scan_fixtures) check_scan_fixtures "$2" ;;
    check_scan_single_file) check_scan_single_file "$2" ;;
    check_catalogue) check_catalogue "$2" ;;
    check_startup) check_startup "$2" ;;
    *) echo "bench: unknown workload checker '$1'" >&2; return 2 ;;
  esac
}

results_json='[]'
human_lines=''
checksum_failed=''

for workload in "${workloads[@]}"; do
  IFS='|' read -r name iterations expected runner checker <<<"$workload"
  samples=''
  checksum=true
  for _ in $(seq 1 "$iterations"); do
    start="$(now_ns)"
    output="$(run_workload "$runner")"
    rc=$?
    end="$(now_ns)"
    elapsed=$(( end - start ))
    [ "$elapsed" -gt 0 ] || elapsed=1
    samples="${samples}${samples:+,}${elapsed}"
    if [ "$rc" != "$expected" ]; then
      checksum=false
      echo "bench: $name: iteration exited $rc, expected $expected" >&2
      break
    fi
    if ! run_check "$checker" "$output"; then
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
