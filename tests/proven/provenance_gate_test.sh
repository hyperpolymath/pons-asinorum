#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# provenance_gate_test.sh — proves the provenance gate can FAIL.
#
# The framework's load-bearing claim is that a coverage cell is occupied only
# by a test that ran and passed. A gate that has never been deliberately
# tripped is not a gate: it is a script that prints "OK". This test drives
# `tests/proven/check_test_provenance.sh` over a scratch copy of the
# repository and plants one defect at a time, asserting that the gate goes
# red and NAMES the defect.
#
# The scratch copy carries a MINIMAL ledger whose suites are trivial, so the
# test does not need a Rust toolchain, a built binary or a network. What is
# under test is the gate's LOGIC — the tier rules, the derived coverage and
# the closed inventory — not the suites themselves.
#
# The defects planted, one per run:
#
#   1  a suite that fails                       — a failure is a failure
#   2  a claimed cell with no passing suite     — a faked cell
#   3  an Actually-Proven claim with no proof   — an unregistered proof
#   4  a Provisionally-Proven claim with no reason
#   5  an unknown provenance tier
#   6  a category outside the taxonomy
#   7  a test script the ledger does not account for
#   8  a stale ledger entry (the file it names is gone)
#
# EXIT CODES
#   0  every mutation was caught, and the unmutated scratch tree is green
#   1  a mutation was NOT caught, or the unmutated scratch tree is red
#   2  NO CHECK WAS PERFORMED — yq or jq is missing
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

for tool in yq jq git tar; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "provenance-gate-test: $tool is not on PATH" >&2
    echo "provenance-gate-test: NO CHECK WAS PERFORMED - a skip is not a pass." >&2
    exit 2
  fi
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
REPO="$WORK/repo"

mkdir -p "$REPO"
(cd "$ROOT" && tar -cf - --exclude=.git --exclude=target .) | (cd "$REPO" && tar -xf -) || {
  echo "provenance-gate-test: could not copy the tree" >&2
  exit 2
}
(cd "$REPO" && git init -q && git add -A && git -c user.name=t -c user.email=t@e \
  commit -qm base) || {
  echo "provenance-gate-test: could not initialise the scratch tree" >&2
  exit 2
}

fail=0

# The minimal ledger. One passing suite that occupies one cell, one declared
# gap, and nothing else. The taxonomy is the real one, so a category outside
# it is still a category outside it.
write_ledger() {
  cat > "$REPO/tests/proven/ledger.kyaml" <<'LEDGER'
{
  policy: "scratch ledger for the gate self-test",
  provenance_tiers: {
    Actually-Proven: "scratch",
    Provisionally-Proven: "scratch",
    Unproven: "scratch",
  },
  taxonomy: {
    categories: [
      "Unit Tests",
      "Mutation Tests",
    ],
    aspects: [
      "Functionality",
      "Maintainability",
    ],
    cells: 4,
  },
  proof_artifacts: [
  ],
  harness: [
    "tests/proven/check_test_provenance.sh",
    "tests/proven/render_dashboard.sh",
    "tests/proven/provenance_gate_test.sh",
    "scripts/proven-bench/check-bench-regression.sh",
    "scripts/proven-bench/compute-bench-baseline.sh",
    "scripts/proven-bench/verify-upstream.sh",
    "tests/aspect_tests.sh",
    "tests/build_provenance.sh",
    "tests/doc_drift.sh",
    "tests/e2e.sh",
    "tests/format_ratchet.sh",
    "tests/format_ratchet_test.sh",
    "tests/sonar_fixture_gate_test.sh",
    "tests/mutation_smoke.sh",
    "tests/workflows/validate_workflows_regression.sh",
    "tests/workflows/validate_workflows_test.sh",
    "benches/pons_bench.sh",
  ],
  entries: [
    {
      id: "scratch-pass",
      command: "printf 'PROVEN-CASES: 1\\n'",
      paths: [
        "tests/scratch-pass.sh",
      ],
      category: "Unit Tests",
      aspects: [
        "Functionality",
      ],
      provenance: "Provisionally-Proven",
      reason: "scratch",
    },
    {
      id: "scratch-gap",
      command: "true",
      paths: [
      ],
      category: "Mutation Tests",
      aspects: [
        "Functionality",
      ],
      provenance: "Unproven",
      reason: "scratch",
    },
  ],
}
LEDGER
  printf '#!/usr/bin/env bash\n# SPDX-License-Identifier: MPL-2.0\nprintf "PROVEN-CASES: 1\\n"\n' \
    > "$REPO/tests/scratch-pass.sh"
  chmod +x "$REPO/tests/scratch-pass.sh"
  # The dashboard is derived from the ledger, so it must be re-rendered for
  # the scratch ledger or the gate would be red for a reason that has
  # nothing to do with the defect under test.
  bash "$REPO/tests/proven/render_dashboard.sh" "$REPO/tests/DASHBOARD.adoc" >/dev/null 2>&1 || {
    echo "provenance-gate-test: could not render the scratch dashboard" >&2
    exit 2
  }
  (cd "$REPO" && git add -A >/dev/null 2>&1)
}

reset() {
  (cd "$REPO" && git checkout -q -- . && git clean -qfd) || exit 2
  write_ledger
}

run_gate() {
  (cd "$REPO" && PROVEN_REPORT=build/proven-report.json bash tests/proven/check_test_provenance.sh 2>&1)
}

expect_red() {
  local label="$1"
  local output rc
  output="$(run_gate)"
  rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "provenance-gate-test: FAIL - the gate stayed GREEN for: $label" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  if [ "$rc" -ne 1 ]; then
    echo "provenance-gate-test: FAIL - the gate exited $rc (expected 1) for: $label" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  echo "provenance-gate-test: ok - the gate went red for: $label"
}

expect_green() {
  local label="$1"
  local output rc
  output="$(run_gate)"
  rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "provenance-gate-test: FAIL - the gate was RED ($rc) for: $label" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  echo "provenance-gate-test: ok - the gate is green for: $label"
}

reset
echo "== unmutated scratch tree =="
expect_green "the scratch ledger"

echo "== mutation 1: a suite that fails =="
reset
sed -i 's|command: "printf .*"|command: "false"|' "$REPO/tests/proven/ledger.kyaml"
expect_red "a suite that fails"

echo "== mutation 2: a claimed cell with no passing suite =="
reset
# The Unproven entry declares (Mutation Tests, Functionality). Unproven
# entries contribute no coverage, and the gate must not treat the claim as
# evidence.
sed -i 's|provenance: "Unproven"|provenance: "Provisionally-Proven"|' "$REPO/tests/proven/ledger.kyaml"
expect_red "a claimed cell with no passing suite"

echo "== mutation 3: an Actually-Proven claim with no registered proof =="
reset
sed -i '0,/provenance: "Provisionally-Proven"/s|provenance: "Provisionally-Proven"|provenance: "Actually-Proven"|' "$REPO/tests/proven/ledger.kyaml"
expect_red "an Actually-Proven claim with no proof"

echo "== mutation 4: a Provisionally-Proven claim with no reason =="
reset
sed -i '/reason: "scratch",/d' "$REPO/tests/proven/ledger.kyaml"
expect_red "a claim with no recorded reason"

echo "== mutation 5: an unknown provenance tier =="
reset
sed -i '0,/provenance: "Provisionally-Proven"/s|provenance: "Provisionally-Proven"|provenance: "Mostly-Proven"|' "$REPO/tests/proven/ledger.kyaml"
expect_red "an unknown provenance tier"

echo "== mutation 6: a category outside the taxonomy =="
reset
sed -i 's|category: "Unit Tests"|category: "Vibes Tests"|' "$REPO/tests/proven/ledger.kyaml"
expect_red "a category outside the taxonomy"

echo "== mutation 7: a test script the ledger does not account for =="
reset
printf '#!/usr/bin/env bash\n# SPDX-License-Identifier: MPL-2.0\nprintf "x\\n"\n' \
  > "$REPO/tests/stray-test.sh"
chmod +x "$REPO/tests/stray-test.sh"
(cd "$REPO" && git add -A >/dev/null 2>&1)
expect_red "a test script the ledger does not account for"

echo "== mutation 8: a stale ledger entry =="
reset
rm -f "$REPO/tests/scratch-pass.sh"
(cd "$REPO" && git add -A >/dev/null 2>&1)
expect_red "a stale ledger entry"

reset
if [ "$fail" -ne 0 ]; then
  echo "provenance-gate-test: FAILED" >&2
  exit 1
fi
echo "provenance-gate-test: OK - every planted defect was caught"
exit 0
