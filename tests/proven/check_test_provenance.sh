#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# check_test_provenance.sh — the provenance gate.
#
# This is the port of hyperpolymath/proven-tests-and-benches to this
# repository. The framework's load-bearing property is that a coverage cell
# of the category x aspect lattice is occupied only by a test that ACTUALLY
# RAN AND PASSED:
#
#     coveredFrom : List (ZigzagCoord, TestMetadata, TestResult) -> List ZigzagCoord
#     pick (co, _, Passed) = Just co
#     pick _               = Nothing
#
# Nothing else is allowed to write a `#` into the map. The port keeps that
# property exactly: the declared ledger says what is CLAIMED; the executed
# results say what is COVERED; the two are compared, and the gate fails when
# they disagree.
#
# The gate also enforces the framework's tier discipline:
#
#   * Actually-Proven requires a machine-checked proof artefact, and the
#     artefact must be registered in the ledger's `proof_artifacts`. This
#     repository has none, so the tier is currently unreachable — and the
#     gate says so rather than accepting the claim.
#   * Provisionally-Proven requires the suite to have run and passed, and
#     requires a recorded `reason` (the assumption that keeps it provisional).
#   * Unproven is a declared gap. It contributes NO coverage and is not a
#     failure, but it must say why.
#
# And the inventory is CLOSED: every test and bench script in the tree must
# be an entry or be named as harness. A new test that nobody classified is a
# failure, because an unclassified test is a test whose evidence nobody has
# read.
#
# EXIT CODES
#   0  the ledger is sound, every declared suite ran and passed, the derived
#      coverage contains every declared cell, and the dashboard is current
#   1  a violation; every violation is named
#   2  NO CHECK WAS PERFORMED — a tool is missing or the ledger is unreadable
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT" || exit 2

LEDGER="tests/proven/ledger.kyaml"
DASHBOARD="tests/DASHBOARD.adoc"
REPORT="${PROVEN_REPORT:-build/proven-report.json}"

fail=0

die() {
  echo "provenance: $*" >&2
  echo "provenance: NO CHECK WAS PERFORMED - a skip is not a pass." >&2
  exit 2
}

for tool in yq jq git; do
  command -v "$tool" >/dev/null 2>&1 || die "$tool is not on PATH"
done
[ -f "$LEDGER" ] || die "$LEDGER is missing"

# cargo is required only when a declared suite needs it. Checking for it
# unconditionally would make the gate inert on a machine that has no Rust
# toolchain, which is a skip, not a pass.
if grep -q 'cargo' "$LEDGER"; then
  command -v cargo >/dev/null 2>&1 || die "a declared suite needs cargo, and cargo is not on PATH"
fi

mkdir -p "$(dirname "$REPORT")"

# json_str_array — a JSON array of the given strings, empty array for none.
json_str_array() {
  if [ "$#" -eq 0 ]; then printf '[]'; else printf '%s\n' "$@" | jq -R . | jq -s .; fi
}

echo "== ledger =="

# The ledger is converted ONCE, with yq, into a single JSON document, and
# every query below is made against that with jq. `yq -o=json '.entries[]'`
# would emit one JSON document PER ENTRY, and whether yq separates those
# documents with `---` is not a property this gate should depend on.
LEDGER_JSON="$(yq -p yaml -o=json '.' "$LEDGER")"

mapfile -t CATEGORIES < <(jq -r '.taxonomy.categories[]' <<<"$LEDGER_JSON")
mapfile -t ASPECTS < <(jq -r '.taxonomy.aspects[]' <<<"$LEDGER_JSON")
TOTAL_CELLS="$(jq -r '.taxonomy.cells' <<<"$LEDGER_JSON")"
declare -A KNOWN_CATEGORY=()
for c in "${CATEGORIES[@]}"; do KNOWN_CATEGORY["$c"]=1; done
declare -A KNOWN_ASPECT=()
for a in "${ASPECTS[@]}"; do KNOWN_ASPECT["$a"]=1; done

# The declared cell count must agree with the lattice the taxonomy declares.
expected_cells=$(( ${#CATEGORIES[@]} * ${#ASPECTS[@]} ))
if [ "$TOTAL_CELLS" != "$expected_cells" ]; then
  echo "::error::the ledger says the lattice has $TOTAL_CELLS cells, but $expected_cells categories x ${#ASPECTS[@]} aspects = $expected_cells." >&2
  fail=1
fi

mapfile -t PROOF_ARTIFACTS < <(jq -r '.proof_artifacts[]?' <<<"$LEDGER_JSON")
declare -A KNOWN_PROOF=()
for p in "${PROOF_ARTIFACTS[@]}"; do
  [ -n "$p" ] || continue
  KNOWN_PROOF["$p"]=1
  if [ ! -f "$p" ]; then
    echo "::error file=$p::provenance: the ledger registers $p as a machine-checked proof artefact, but no such file exists." >&2
    fail=1
  fi
done

mapfile -t ENTRY_IDS < <(jq -r '.entries[].id' <<<"$LEDGER_JSON")
if [ "${#ENTRY_IDS[@]}" -eq 0 ]; then
  echo "::error::the ledger declares no entries; the gate would check nothing." >&2
  fail=1
fi

# The closed inventory.
mapfile -t TREE_SCRIPTS < <(git ls-files -- 'tests/*.sh' 'benches/*.sh' 'scripts/proven-bench/*.sh')
mapfile -t HARNESS < <(jq -r '.harness[]?' <<<"$LEDGER_JSON")
declare -A ACCOUNTED=()
for h in "${HARNESS[@]}"; do
  [ -n "$h" ] || continue
  ACCOUNTED["$h"]=1
done
while IFS= read -r p; do
  [ -n "$p" ] || continue
  ACCOUNTED["$p"]=1
done < <(jq -r '.entries[].paths[]?' <<<"$LEDGER_JSON")

for s in "${TREE_SCRIPTS[@]}"; do
  if [ -z "${ACCOUNTED["$s"]+x}" ]; then
    echo "::error file=$s::provenance: $s is a test or bench script that the ledger does not account for. Add it to an entry's paths, or to the harness list if it is machinery rather than a test." >&2
    fail=1
  fi
done
for p in "${!ACCOUNTED[@]}"; do
  [ -n "$p" ] || continue
  if ! git ls-files --error-unmatch -- "$p" >/dev/null 2>&1; then
    echo "::error file=$p::provenance: the ledger accounts for $p, but no such tracked file exists. A stale entry is how an inventory stops describing the tree." >&2
    fail=1
  fi
done

echo "  categories: ${#CATEGORIES[@]}; aspects: ${#ASPECTS[@]}; cells: $TOTAL_CELLS"
echo "  entries: ${#ENTRY_IDS[@]}; proof artefacts: ${#PROOF_ARTIFACTS[@]}"

echo "== suites =="

entries_json='[]'
declare -A DERIVED=()
derived_cells=0
passed_count=0
unproven_count=0

ENTRY_FILE="$(mktemp)"
trap 'rm -f "$ENTRY_FILE"' EXIT

while IFS= read -r entry; do
  [ -n "$entry" ] || continue
  printf '%s\n' "$entry" > "$ENTRY_FILE"
  id="$(yq -p json -r '.id' "$ENTRY_FILE")"
  command="$(yq -p json -r '.command' "$ENTRY_FILE")"
  category="$(yq -p json -r '.category' "$ENTRY_FILE")"
  provenance="$(yq -p json -r '.provenance' "$ENTRY_FILE")"
  reason="$(yq -p json -r '.reason // ""' "$ENTRY_FILE")"
  proof="$(yq -p json -r '.proof // ""' "$ENTRY_FILE")"
  mapfile -t aspects < <(yq -p json -r '.aspects[]' "$ENTRY_FILE")

  # The tier rules, checked before anything runs.
  case "$provenance" in
    Actually-Proven)
      if [ -z "$proof" ]; then
        echo "::error::$id claims Actually-Proven but names no proof artefact." >&2
        fail=1
        continue
      fi
      if [ -z "${KNOWN_PROOF["$proof"]+x}" ]; then
        echo "::error::$id claims Actually-Proven with proof '$proof', which is not registered in the ledger's proof_artifacts. A proof artefact that is not registered is not a proof artefact." >&2
        fail=1
        continue
      fi
      ;;
    Provisionally-Proven|Unproven)
      if [ -z "$reason" ]; then
        echo "::error::$id claims $provenance but records no reason. The reason is what keeps the claim honest." >&2
        fail=1
        continue
      fi
      ;;
    *)
      echo "::error::$id has an unknown provenance tier '$provenance'. The three tiers are Actually-Proven, Provisionally-Proven and Unproven." >&2
      fail=1
      continue
      ;;
  esac

  if [ -z "${KNOWN_CATEGORY["$category"]+x}" ]; then
    echo "::error::$id declares category '$category', which is not in the ledger's taxonomy." >&2
    fail=1
    continue
  fi
  for a in "${aspects[@]}"; do
    if [ -z "${KNOWN_ASPECT["$a"]+x}" ]; then
      echo "::error::$id declares aspect '$a', which is not in the ledger's taxonomy." >&2
      fail=1
    fi
  done

  if [ "$provenance" = "Unproven" ]; then
    # A declared gap. It contributes no coverage and is not a failure.
    unproven_count=$((unproven_count + 1))
    echo "  $id: $provenance (declared gap, not run, contributes no coverage)"
    entries_json="$(jq -c --arg id "$id" --arg prov "$provenance" --arg cat "$category" \
      --arg reason "$reason" --argjson aspects "$(json_str_array "${aspects[@]}")" \
      '. + [{id: $id, provenance: $prov, category: $cat, aspects: $aspects, result: "NotRun", cases: null, reason: $reason}]' <<<"$entries_json")"
    continue
  fi

  log="$(mktemp)"
  rc=0
  bash -c "$command" >"$log" 2>&1 || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "::error::the suite '$id' FAILED (exit $rc). A suite that ran and failed is a failure of the build, not a coverage claim." >&2
    tail -n 40 "$log" | sed 's/^/    /' >&2
    rm -f "$log"
    entries_json="$(jq -c --arg id "$id" --arg prov "$provenance" --arg cat "$category" \
      --argjson aspects "$(json_str_array "${aspects[@]}")" \
      '. + [{id: $id, provenance: $prov, category: $cat, aspects: $aspects, result: "Failed", cases: null}]' <<<"$entries_json")"
    continue
  fi
  if [ ! -s "$log" ]; then
    echo "::error::the suite '$id' produced no output. A suite that says nothing tested nothing; that is not evidence." >&2
    fail=1
    rm -f "$log"
    continue
  fi

  # The case count, when the tool reports one. `cargo test` prints
  # "test result: ok. N passed; ...". A shell suite may print
  # "PROVEN-CASES: N". Where neither is available the count is recorded as
  # null rather than invented.
  cases="null"
  if grep -qE '^test result: ok\.' "$log"; then
    n="$(grep -E '^test result: ok\.' "$log" | tail -1 | sed -E 's/.*ok\. ([0-9]+) passed.*/\1/')"
    [ -n "$n" ] && cases="$n"
  elif grep -qE '^PROVEN-CASES: [0-9]+' "$log"; then
    n="$(grep -E '^PROVEN-CASES: [0-9]+' "$log" | tail -1 | sed -E 's/^PROVEN-CASES: ([0-9]+)$/\1/')"
    [ -n "$n" ] && cases="$n"
  fi
  if [ "$cases" = "null" ]; then
    echo "::notice::the suite '$id' reports no machine-readable case count; the report records null rather than an invented number." >&2
  fi

  passed_count=$((passed_count + 1))
  for a in "${aspects[@]}"; do
    if [ -z "${DERIVED["$category|$a"]+x}" ]; then
      DERIVED["$category|$a"]=1
      derived_cells=$((derived_cells + 1))
    fi
  done
  echo "  $id: passed${cases:+, $cases case(s)}"
  rm -f "$log"
  entries_json="$(jq -c --arg id "$id" --arg prov "$provenance" --arg cat "$category" \
    --argjson aspects "$(json_str_array "${aspects[@]}")" \
    --argjson cases "$cases" \
    '. + [{id: $id, provenance: $prov, category: $cat, aspects: $aspects, result: "Passed", cases: $cases}]' <<<"$entries_json")"
done < <(jq -c '.entries[]' <<<"$LEDGER_JSON")

echo "  suites passed: $passed_count; declared gaps: $unproven_count; derived cells: $derived_cells"

echo "== derived coverage =="

# Every cell the ledger CLAIMS must be occupied by a suite that ran and
# passed. A claim with no passing suite behind it is a faked cell.
claimed_missing=0
while IFS= read -r row; do
  [ -n "$row" ] || continue
  cat="$(jq -r '.category' <<<"$row")"
  asp="$(jq -r '.aspect' <<<"$row")"
  [ -n "$cat" ] || continue
  if [ -z "${DERIVED["$cat|$asp"]+x}" ]; then
    echo "::error::the ledger claims ($cat, $asp), but no suite that RAN and PASSED occupies it. The declared cell is not evidence." >&2
    claimed_missing=$((claimed_missing + 1))
    fail=1
  fi
done < <(jq -c '.entries[] | select(.provenance != "Unproven") | {category: .category, aspect: .aspects[]}' <<<"$LEDGER_JSON")

echo "  claimed cells without a passing suite: $claimed_missing"

# The report. The coverage map is the DERIVED one: a `#` requires an
# executed, passing suite.
lattice="$(for cat in "${CATEGORIES[@]}"; do
  printf '%-38s' "$cat"
  for asp in "${ASPECTS[@]}"; do
    if [ -n "${DERIVED["$cat|$asp"]+x}" ]; then printf '#'; else printf '.'; fi
    printf ' '
  done
  printf '\n'
done)"

jq -n \
  --argjson entries "$entries_json" \
  --argjson categories "$(json_str_array "${CATEGORIES[@]}")" \
  --argjson aspects "$(json_str_array "${ASPECTS[@]}")" \
  --argjson total_cells "$TOTAL_CELLS" \
  --argjson derived_cells "$derived_cells" \
  --arg lattice "$lattice" \
  '{
     schema_version: 1,
     suite: "pons-provenance",
     generated_by: "tests/proven/check_test_provenance.sh",
     note: "Coverage is DERIVED: a cell is occupied only by a suite that ran and passed. The ledger says what is claimed; this file says what was witnessed.",
     taxonomy: {categories: $categories, aspects: $aspects, total_cells: $total_cells},
     entries: $entries,
     coverage: {derived_cells: $derived_cells, total_cells: $total_cells, lattice: $lattice}
   }' > "$REPORT"

echo "  report written: $REPORT"

echo "== dashboard =="

rendered="$(mktemp)"
bash "$SCRIPT_DIR/render_dashboard.sh" "$rendered" || die "the dashboard could not be rendered"
if [ ! -f "$DASHBOARD" ]; then
  echo "::error file=$DASHBOARD::provenance: the dashboard is missing. Render it with: tests/proven/render_dashboard.sh $DASHBOARD" >&2
  fail=1
elif ! cmp -s "$rendered" "$DASHBOARD"; then
  echo "::error file=$DASHBOARD::provenance: the dashboard does not match the ledger. Render it with: tests/proven/render_dashboard.sh $DASHBOARD" >&2
  diff -u "$DASHBOARD" "$rendered" | head -n 40 >&2 || true
  fail=1
else
  echo "  the dashboard matches the ledger"
fi
rm -f "$rendered"

if [ "$fail" -ne 0 ]; then
  echo "provenance: FAILED" >&2
  exit 1
fi
echo "provenance: OK"
exit 0
