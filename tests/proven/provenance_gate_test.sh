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
    "tests/sonar_fixture_gate_mutation_test.sh",
    "tests/mutation_smoke.sh",
    "tests/workflows/validate_workflows_regression.sh",
    "tests/workflows/validate_workflows_test.sh",
    "benches/pons_bench.sh",
  ],
  entries: [
    {
      id: "scratch-pass",
      command: ["bash", "tests/scratch-pass.sh"],
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
      command: ["true"],
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
  # The index AND the working tree go back to HEAD. `git checkout -- .`
  # alone leaves a staged file staged, and a staged file from one mutation
  # would still be there for the next one, masking it.
  (cd "$REPO" && git reset --quiet --hard HEAD && git clean -qfd) || exit 2
  write_ledger
}

run_gate() {
  (cd "$REPO" && PROVEN_REPORT=build/proven-report.json bash tests/proven/check_test_provenance.sh 2>&1)
}

# expect_red LABEL DIAGNOSTIC: the gate must exit 1 AND name the defect.
# The exit status alone proves nothing. A gate that goes red for the wrong
# reason is as useless as one that never goes red, so the diagnostic the
# defect should produce has to appear in the output.
expect_red() {
  local label="$1" diagnostic="$2"
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
  if ! printf '%s\n' "$output" | grep -qF -- "$diagnostic"; then
    echo "provenance-gate-test: FAIL - the gate went red for: $label, but never named the defect" >&2
    echo "provenance-gate-test:   expected to find: $diagnostic" >&2
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
sed -i '/id: "scratch-pass"/,/^    },/ s|command: \["bash", "tests/scratch-pass.sh"\]|command: ["false"]|' "$REPO/tests/proven/ledger.kyaml"
expect_red "a suite that fails" "the suite 'scratch-pass' FAILED"

echo "== mutation 2: a claimed cell with no passing suite =="
reset
# The Unproven entry declares (Mutation Tests, Functionality). Unproven
# entries contribute no coverage, and the gate must not treat the claim as
# evidence: promoting it to a claim, with no suite behind it, must be
# caught by the coverage check and not merely reported.
sed -i '/id: "scratch-gap"/,/^    },/ s|provenance: "Unproven"|provenance: "Provisionally-Proven"|' "$REPO/tests/proven/ledger.kyaml"
expect_red "a claimed cell with no passing suite" "the ledger claims (Mutation Tests, Functionality), but no suite that RAN and PASSED occupies it"

echo "== mutation 3: an Actually-Proven claim with no registered proof =="
reset
sed -i '/id: "scratch-pass"/,/^    },/ s|provenance: "Provisionally-Proven"|provenance: "Actually-Proven"|' "$REPO/tests/proven/ledger.kyaml"
expect_red "an Actually-Proven claim with no proof" "claims Actually-Proven but names no proof artefact"

echo "== mutation 4: a Provisionally-Proven claim with no reason =="
reset
sed -i '0,/reason: "scratch",/ { /reason: "scratch",/d }' "$REPO/tests/proven/ledger.kyaml"
expect_red "a claim with no recorded reason" "claims Provisionally-Proven but records no reason"

echo "== mutation 5: an unknown provenance tier =="
reset
sed -i '/id: "scratch-pass"/,/^    },/ s|provenance: "Provisionally-Proven"|provenance: "Mostly-Proven"|' "$REPO/tests/proven/ledger.kyaml"
expect_red "an unknown provenance tier" "has an unknown provenance tier 'Mostly-Proven'"

echo "== mutation 6: a category outside the taxonomy =="
reset
sed -i '/id: "scratch-pass"/,/^    },/ s|category: "Unit Tests"|category: "Vibes Tests"|' "$REPO/tests/proven/ledger.kyaml"
expect_red "a category outside the taxonomy" "declares category 'Vibes Tests', which is not in the ledger's taxonomy"

echo "== mutation 7: a test script the ledger does not account for =="
reset
printf '#!/usr/bin/env bash\n# SPDX-License-Identifier: MPL-2.0\nprintf "x\\n"\n' \
  > "$REPO/tests/stray-test.sh"
chmod +x "$REPO/tests/stray-test.sh"
(cd "$REPO" && git add -A >/dev/null 2>&1)
expect_red "a test script the ledger does not account for" "is a test or bench script that the ledger does not account for"

echo "== mutation 8: a stale ledger entry =="
reset
rm -f "$REPO/tests/scratch-pass.sh"
(cd "$REPO" && git add -A >/dev/null 2>&1)
expect_red "a stale ledger entry" "the ledger accounts for tests/scratch-pass.sh, but no such tracked file exists"

reset
if [ "$fail" -ne 0 ]; then
  echo "provenance-gate-test: FAILED" >&2
  exit 1
fi
echo "provenance-gate-test: OK - every planted defect was caught"
exit 0
