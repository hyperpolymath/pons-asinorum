#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# sonar_fixture_gate_mutation_test.sh — proves the fixture-aware Sonar gate
# can FAIL.
#
# A gate that has never been deliberately tripped is not a gate: it is a
# script that prints "OK". This test drives `tests/sonar_fixture_gate_test.sh`
# over a scratch copy of the repository and plants one defect at a time,
# asserting that the gate goes red AND names the defect it was supposed to
# catch. The unmutated tree must stay green, or the test proves nothing.
#
# The defects planted, one per run:
#
#   1  a waiver with no reason                  — a suppression without a why
#   2  a waiver with no owner                   — a suppression nobody owns
#   3  an expired waiver                        — an inherited waiver
#   4  a malformed expiry                       — an expiry that cannot be read
#   5  a wildcard rule key                      — a whole rule waived everywhere
#   6  a glob path                              — a whole subtree waived
#   7  a waiver naming a file that is not there — a waiver for nothing
#   8  a waiver outside the fixture corpus      — product code waived
#   9  a declaration with no Sonar criterion    — a promise not kept
#  10  a Sonar criterion with no declaration    — a silent suppression
#  11  the criterion list assigned twice        — only the last list survives
#
# EXIT CODES
#   0  every mutation was caught, and the unmutated scratch tree is green
#   1  a mutation was NOT caught, or the unmutated scratch tree is red
#   2  NO CHECK WAS PERFORMED — yq, jq or git is missing
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

for tool in yq jq git tar; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "sonar-fixture-gate-mutation-test: $tool is not on PATH" >&2
    echo "sonar-fixture-gate-mutation-test: NO CHECK WAS PERFORMED - a skip is not a pass." >&2
    exit 2
  fi
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
REPO="$WORK/repo"

mkdir -p "$REPO"
(cd "$ROOT" && tar -cf - --exclude=.git --exclude=target .) | (cd "$REPO" && tar -xf -) || {
  echo "sonar-fixture-gate-mutation-test: could not copy the tree" >&2
  exit 2
}
(cd "$REPO" && git init -q && git add -A && git -c user.name=t -c user.email=t@e \
  commit -qm base) || {
  echo "sonar-fixture-gate-mutation-test: could not initialise the scratch tree" >&2
  exit 2
}

fail=0
DECL="$REPO/ci/sonar-fixture-diagnostics.kyaml"
PROPS="$REPO/sonar-project.properties"

reset() {
  (cd "$REPO" && git reset --quiet --hard HEAD && git clean -qfd) || exit 2
}

run_gate() {
  (cd "$REPO" && bash tests/sonar_fixture_gate_test.sh 2>&1)
}

# expect_red LABEL DIAGNOSTIC: the gate must exit 1 AND name the defect.
# The exit status alone proves nothing: a gate that goes red for the wrong
# reason is as useless as one that never goes red.
expect_red() {
  local label="$1" diagnostic="$2"
  local output rc
  output="$(run_gate)"
  rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "sonar-fixture-gate-mutation-test: FAIL - the gate stayed GREEN for: $label" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  if [ "$rc" -ne 1 ]; then
    echo "sonar-fixture-gate-mutation-test: FAIL - the gate exited $rc (expected 1) for: $label" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  if ! printf '%s\n' "$output" | grep -qF -- "$diagnostic"; then
    echo "sonar-fixture-gate-mutation-test: FAIL - the gate went red for: $label, but never named the defect" >&2
    echo "sonar-fixture-gate-mutation-test:   expected to find: $diagnostic" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  echo "sonar-fixture-gate-mutation-test: ok - the gate went red for: $label"
}

expect_green() {
  local label="$1"
  local output rc
  output="$(run_gate)"
  rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "sonar-fixture-gate-mutation-test: FAIL - the gate was RED ($rc) for: $label" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  echo "sonar-fixture-gate-mutation-test: ok - the gate is green for: $label"
}

echo "== unmutated scratch tree =="
expect_green "the repository as committed"

echo "== mutation 1: a waiver with no reason =="
reset
sed -i '0,/reason: "[^"]*"/s|reason: "[^"]*"|reason: ""|' "$DECL"
expect_red "a waiver with no reason" "has no reason"

echo "== mutation 2: a waiver with no owner =="
reset
# The range matters: the declaration file has a top-level `owner:` too, and
# blanking that one would change nothing the gate reads.
sed -i '/^      rule: "/,/^    },/ s|owner: "[^"]*"|owner: ""|' "$DECL"
expect_red "a waiver with no owner" "has no owner"

echo "== mutation 3: an expired waiver =="
reset
sed -i '0,/expires: "[^"]*"/s|expires: "[^"]*"|expires: "2001-01-01"|' "$DECL"
expect_red "an expired waiver" "expired on 2001-01-01"

echo "== mutation 4: a malformed expiry =="
reset
sed -i '0,/expires: "[^"]*"/s|expires: "[^"]*"|expires: "next tuesday"|' "$DECL"
expect_red "a malformed expiry" "is not a date in YYYY-MM-DD form"

echo "== mutation 5: a wildcard rule key =="
reset
sed -i '0,/rule: "[^"]*"/s|rule: "[^"]*"|rule: "php:*"|' "$DECL"
expect_red "a wildcard rule key" "is a WILDCARD"

echo "== mutation 6: a glob path =="
reset
sed -i '0,/path: "[^"]*"/s|path: "[^"]*"|path: "fixtures/**/*.php"|' "$DECL"
expect_red "a glob path" "the waiver path is a PATTERN"

echo "== mutation 7: a waiver naming a file that is not there =="
reset
sed -i '0,/path: "[^"]*"/s|path: "[^"]*"|path: "fixtures/unreachable-after-jump/positive/ghost.php"|' "$DECL"
expect_red "a waiver naming a file that is not there" "does not exist"

echo "== mutation 8: a waiver outside the fixture corpus =="
reset
sed -i '0,/path: "[^"]*"/s|path: "[^"]*"|path: "crates/pons-core/src/lib.rs"|' "$DECL"
expect_red "a waiver outside the fixture corpus" "names a file outside the fixture corpus"

echo "== mutation 9: a declaration with no Sonar criterion =="
reset
# Drop the criterion that matches the first declaration. The declaration
# survives; the configuration does not. That is a promise the configuration
# does not keep, and the fixture would still fail the SonarCloud check.
sed -i '0,/^sonar\.issue\.ignore\.multicriteria\.fixture-01\.ruleKey=/d' "$PROPS"
expect_red "a declaration with no Sonar criterion" "is declared but SonarCloud is not told to ignore it"

echo "== mutation 10: a Sonar criterion with no declaration =="
reset
sed -i '0,/^    rule: "php:S1763"/d' "$DECL"
expect_red "a Sonar criterion with no declaration" "no waiver is declared for it"

echo "== mutation 11: the criterion list assigned twice =="
reset
# A properties file keeps only the LAST assignment of a key. Declaring the
# list twice therefore drops every criterion but the last, silently.
sed -i 's|^sonar\.issue\.ignore\.multicriteria=.*|sonar.issue.ignore.multicriteria=fixture-01\nsonar.issue.ignore.multicriteria=fixture-06|' "$PROPS"
expect_red "the criterion list assigned twice" "assigns 'sonar.issue.ignore.multicriteria' 2 times"

reset
if [ "$fail" -ne 0 ]; then
  echo "sonar-fixture-gate-mutation-test: FAILED" >&2
  exit 1
fi
echo "sonar-fixture-gate-mutation-test: OK - every planted defect was caught"
exit 0
