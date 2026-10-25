#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# render_dashboard.sh — render `tests/DASHBOARD.adoc` from the provenance
# ledger.
#
# The dashboard is a DERIVED ARTEFACT. It is rendered from
# `tests/proven/ledger.kyaml` and nothing else, so it cannot drift from the
# inventory it describes: `check_test_provenance.sh` re-renders it and fails
# when the committed copy differs.
#
# WHAT THE DASHBOARD DOES AND DOES NOT CLAIM
# The lattice it prints is the DECLARED lattice: the cells the ledger's
# entries occupy. The DERIVED lattice — the cells occupied by tests that
# actually ran and passed — is computed by `check_test_provenance.sh` at run
# time, because it depends on what the suite did on this machine, and it is
# published in the CI job summary and written to `build/proven-report.json`.
# The dashboard says which is which, so the declared map is never mistaken for
# evidence.
#
# The ledger is read with yq (YAML-POLICY Y-1: a gate that needs a value out
# of a YAML file asks the parser) and converted once to JSON; every
# formatting decision below is then made by jq, so the rendering does not
# depend on a particular yq's string formatting.
#
# Usage: tests/proven/render_dashboard.sh [OUTPUT]
#   OUTPUT defaults to stdout.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LEDGER="$ROOT/tests/proven/ledger.kyaml"
OUT="${1:-}"

for tool in yq jq; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "render-dashboard: $tool is not on PATH" >&2
    exit 2
  }
done
[ -f "$LEDGER" ] || {
  echo "render-dashboard: $LEDGER is missing" >&2
  exit 2
}

LEDGER_JSON="$(yq -p yaml -o=json '.' "$LEDGER")"

# Aspect abbreviations: the first two letters, as the framework's grid does.
abbr() {
  printf '%s' "$1" | cut -c1-2
}

mapfile -t CATEGORIES < <(jq -r '.taxonomy.categories[]' <<<"$LEDGER_JSON")
mapfile -t ASPECTS < <(jq -r '.taxonomy.aspects[]' <<<"$LEDGER_JSON")

# The declared cells: for each (category, aspect), is there an entry that
# declares that category and lists that aspect?
declare -A DECLARED=()
while IFS=$'\t' read -r cat asp; do
  [ -n "$cat" ] || continue
  DECLARED["$cat|$asp"]=1
done < <(jq -r '.entries[] | .category as $c | .aspects[] | [$c, .] | @tsv' <<<"$LEDGER_JSON")

{
  echo "// SPDX-License-Identifier: CC-BY-SA-4.0"
  echo "//"
  echo "// GENERATED FILE - DO NOT EDIT BY HAND."
  echo "//"
  echo "// Rendered from tests/proven/ledger.kyaml by tests/proven/render_dashboard.sh."
  echo "// \`tests/proven/check_test_provenance.sh\` re-renders it and fails when the"
  echo "// committed copy differs, so this file cannot drift from the inventory it"
  echo "// describes."
  echo "= Tests and benchmarks dashboard"
  echo ":toc: preamble"
  echo ":toclevels: 3"
  echo ""
  echo "Pons's test and benchmark evidence, described the way"
  echo "https://github.com/hyperpolymath/proven-tests-and-benches[proven-tests-and-benches]"
  echo "describes it: every test carries a *provenance tier*, and coverage over the"
  echo "estate's category x aspect lattice is derived from the tests that actually"
  echo "ran and passed, never from a claim."
  echo ""
  echo "== The three provenance tiers"
  echo ""
  echo "[cols=\"1,4\"]"
  echo "|==="
  echo "| Tier | Meaning"
  echo ""
  jq -r '.provenance_tiers | to_entries[] | "| \(.key)\n| \(.value)"' <<<"$LEDGER_JSON"
  echo "|==="
  echo ""
  echo "== The declared lattice"
  echo ""
  echo "Columns are the first two letters of each aspect; \`#\` means some entry in the"
  echo "ledger occupies that (category, aspect) cell."
  echo ""
  echo "[source,text]"
  echo "----"
  printf '%-38s' ""
  for asp in "${ASPECTS[@]}"; do
    printf '%-3s' "$(abbr "$asp")"
  done
  printf '\n'
  for cat in "${CATEGORIES[@]}"; do
    printf '%-38s' "$cat"
    for asp in "${ASPECTS[@]}"; do
      if [ -n "${DECLARED["$cat|$asp"]+x}" ]; then
        printf '%-3s' '#'
      else
        printf '%-3s' '.'
      fi
    done
    printf '\n'
  done
  echo "----"
  echo ""
  declared_cells="$(printf '%s\n' "${!DECLARED[@]}" | grep -c . || true)"
  total_cells="$(jq -r '.taxonomy.cells' <<<"$LEDGER_JSON")"
  echo "Declared cells: *${declared_cells}* of *${total_cells}*"
  echo "(${#CATEGORIES[@]} categories x ${#ASPECTS[@]} aspects)."
  echo ""
  echo "!!! NOTE"
  echo "This is the DECLARED map. The DERIVED map — the cells occupied by tests"
  echo "that actually ran and passed — is computed at run time by"
  echo "\`tests/proven/check_test_provenance.sh\`, written to"
  echo "\`build/proven-report.json\` and published in the CI job summary. A suite"
  echo "that is declared but did not run contributes nothing to the derived map,"
  echo "and the gate fails the build when a suite that was supposed to run failed."
  echo ""
  echo "== The inventory"
  echo ""
  echo "[cols=\"2,3,2,2,1\"]"
  echo "|==="
  echo "| Test | Command | Category | Aspects | Tier"
  echo ""
  jq -r '.entries[] | "| \(.id)\n| `\(.command)`\n| \(.category)\n| \(.aspects | join(", "))\n| \(.provenance)"' <<<"$LEDGER_JSON"
  echo "|==="
  echo ""
  echo "== Machine-checked proofs"
  echo ""
  proof_count="$(jq -r '.proof_artifacts | length' <<<"$LEDGER_JSON")"
  if [ "$proof_count" -eq 0 ]; then
    echo "None. This repository has no machine-checked proof artefact, so no entry"
    echo "may claim the Actually-Proven tier: the gate fails such a claim rather"
    echo "than accepting it. Every obligation here is discharged, if at all, by an"
    echo "executed test — which is what Provisionally-Proven means."
  else
    jq -r '.proof_artifacts[] | "* \(.)"' <<<"$LEDGER_JSON"
  fi
  echo ""
  echo "== Harness"
  echo ""
  echo "These are the gate's own machinery, not tests, and are excluded from the"
  echo "inventory for that reason."
  echo ""
  jq -r '.harness[] | "* `\(. | tostring)`"' <<<"$LEDGER_JSON"
  echo ""
} > "${TMPDIR:-/tmp}/pons-dashboard.adoc.$$"

if [ -n "$OUT" ]; then
  mv "${TMPDIR:-/tmp}/pons-dashboard.adoc.$$" "$OUT"
else
  cat "${TMPDIR:-/tmp}/pons-dashboard.adoc.$$"
  rm -f "${TMPDIR:-/tmp}/pons-dashboard.adoc.$$"
fi
