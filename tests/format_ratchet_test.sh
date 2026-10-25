#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# format_ratchet_test.sh — proves the format ratchet can FAIL.
#
# A gate that has never been deliberately tripped is not a gate: it is a
# script that prints "OK". This test drives `tests/format_ratchet.sh` over a
# scratch copy of the repository and plants one defect at a time, asserting
# that the gate goes red and NAMES the defect. The unmutated tree must stay
# green, or the test proves nothing.
#
# The defects planted, one per run:
#
#   1  an unaccounted YAML file          — the inventory is not closed
#   2  a `converted` file that is not KYAML
#   3  a ratchet entry whose file is gone — a stale entry
#   4  an unaccounted JSON file          — the inventory is not closed
#   5  an invalid JSON file (duplicate keys — I-JSON forbids them)
#   6  a `canonical` JSON file whose bytes are not RFC 8785 canonical
#   7  a JSON Lines file with an invalid record   — the records are checked
#                                                     one at a time, so a bad
#                                                     second record is caught
#   8  a `canonical` JSON Lines file whose records are not RFC 8785 canonical
#
# EXIT CODES
#   0  every mutation was caught, and the unmutated tree is green
#   1  a mutation was NOT caught, or the unmutated tree is red
#   2  NO CHECK WAS PERFORMED — yq or ijson-jcs is missing
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

for tool in yq ijson-jcs git tar; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "format-ratchet-test: $tool is not on PATH" >&2
    echo "format-ratchet-test: NO CHECK WAS PERFORMED - a skip is not a pass." >&2
    exit 2
  fi
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
REPO="$WORK/repo"

mkdir -p "$REPO"
(cd "$ROOT" && tar -cf - --exclude=.git --exclude=target .) | (cd "$REPO" && tar -xf -) || {
  echo "format-ratchet-test: could not copy the tree" >&2
  exit 2
}
(cd "$REPO" && git init -q && git add -A && git -c user.name=t -c user.email=t@e \
  commit -qm base) || {
  echo "format-ratchet-test: could not initialise the scratch tree" >&2
  exit 2
}

fail=0

reset() {
  # The index AND the working tree go back to HEAD. `git checkout -- .`
  # alone restores from the INDEX, and run_gate stages what it plants, so a
  # mutation would survive into the next run and mask it.
  (cd "$REPO" && git reset --quiet --hard HEAD && git clean -qfd) || exit 2
}

run_gate() {
  # The gate inventories the tree with `git ls-files`, so a planted file
  # must be tracked in the scratch repository or the gate cannot see it.
  (cd "$REPO" && git add -A >/dev/null 2>&1)
  (cd "$REPO" && bash tests/format_ratchet.sh 2>&1)
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
    echo "format-ratchet-test: FAIL - the gate stayed GREEN for: $label" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  if [ "$rc" -ne 1 ]; then
    echo "format-ratchet-test: FAIL - the gate exited $rc (expected 1) for: $label" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  if ! printf '%s\n' "$output" | grep -qF -- "$diagnostic"; then
    echo "format-ratchet-test: FAIL - the gate went red for: $label, but never named the defect" >&2
    echo "format-ratchet-test:   expected to find: $diagnostic" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  echo "format-ratchet-test: ok - the gate went red for: $label"
}

expect_green() {
  local label="$1"
  local output
  output="$(run_gate)"
  local rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "format-ratchet-test: FAIL - the gate was RED ($rc) for: $label" >&2
    printf '%s\n' "$output" | sed 's/^/    /' >&2
    fail=1
    return
  fi
  echo "format-ratchet-test: ok - the gate is green for: $label"
}

echo "== unmutated tree =="
expect_green "the repository as committed"

echo "== mutation 1: an unaccounted YAML file =="
reset
printf 'not: [accounted, for]\n' > "$REPO/stray.yaml"
expect_red "an unaccounted YAML file" "is a YAML file that no ratchet accounts for"

echo "== mutation 2: a converted file that is not KYAML =="
reset
printf 'converted:\n  - ci/yaml-ratchet.kyaml\n' > "$REPO/ci/yaml-ratchet.kyaml"
expect_red "a converted file that is not KYAML" "is not KYAML"

echo "== mutation 3: a ratchet entry whose file is gone =="
reset
# The ratchet names tests/proven/ledger.kyaml as a converted file. Removing
# it leaves a stale entry: the inventory still describes a file that is not
# there.
rm -f "$REPO/tests/proven/ledger.kyaml"
expect_red "a stale ratchet entry" "is listed in ci/yaml-ratchet.kyaml but does not exist"

echo "== mutation 4: an unaccounted JSON file =="
reset
printf '{"a":1}\n' > "$REPO/stray.json"
expect_red "an unaccounted JSON file" "is a JSON file that no ratchet accounts for"

echo "== mutation 5: an invalid JSON file (duplicate keys) =="
reset
printf '{"a":1,"a":2}\n' > "$REPO/duplicate-key.json"
# The file must be ACCOUNTED FOR, or the gate stops at the inventory check
# and never reaches the validity check: an unaccounted file is skipped
# before its bytes are looked at. The inventory is therefore replaced with
# one that declares this file, so that the defect under test is the one the
# gate is asked about.
printf '{\n  policy: "mutation",\n  canonical: [\n  ],\n  declared_non_canonical: [\n    {\n      paths: [\n        "duplicate-key.json",\n      ],\n      reason: "mutation",\n    },\n  ],\n}\n' > "$REPO/ci/json-ratchet.kyaml"
expect_red "an invalid JSON file" "is not valid I-JSON (RFC 7493)"

echo "== mutation 6: a canonical file that is not RFC 8785 canonical =="
reset
printf '{\n  "b": 2,\n  "a": 1\n}\n' > "$REPO/pretty.json"
# Put it in the canonical list. The list is KYAML, so it is edited as KYAML.
printf '{\n  policy: "mutation",\n  canonical: [\n    "pretty.json",\n  ],\n  declared_non_canonical: [\n  ],\n}\n' > "$REPO/ci/json-ratchet.kyaml"
expect_red "a canonical file that is not RFC 8785 canonical" "is listed as canonical but its bytes are not RFC 8785 canonical"

echo "== mutation 7: a JSON Lines file with an invalid record =="
reset
# A JSON Lines file is a SEQUENCE OF RECORDS, and the tool reads whole
# documents only, so each record is checked on its own. A duplicate key in
# the SECOND record must therefore be caught, which it could not be if the
# file were parsed as one document.
printf '{"a":1}\n{"a":1,"a":2}\n' > "$REPO/records.jsonl"
printf '{\n  policy: "mutation",\n  canonical: [\n  ],\n  declared_non_canonical: [\n    {\n      paths: [\n        "records.jsonl",\n      ],\n      reason: "mutation",\n    },\n  ],\n}\n' > "$REPO/ci/json-ratchet.kyaml"
expect_red "a JSON Lines file with an invalid record" "is not valid I-JSON (RFC 7493)"

echo "== mutation 8: a JSON Lines file whose record is not canonical =="
reset
printf '{"b":2,"a":1}\n' > "$REPO/records.jsonl"
printf '{\n  policy: "mutation",\n  canonical: [\n    "records.jsonl",\n  ],\n  declared_non_canonical: [\n  ],\n}\n' > "$REPO/ci/json-ratchet.kyaml"
expect_red "a JSON Lines file whose record is not canonical" "is listed as canonical but its bytes are not RFC 8785 canonical"

reset
if [ "$fail" -ne 0 ]; then
  echo "format-ratchet-test: FAILED" >&2
  exit 1
fi
echo "format-ratchet-test: OK - every planted defect was caught"
exit 0
