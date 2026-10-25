#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# sonar_fixture_gate_test.sh — the fixture-aware SonarCloud quality gate.
#
# docs/PR57-RECONCILIATION.adoc records the position this implements:
#
#   "Expected fixture diagnostics need exact rule/path, reason, owner and
#    expiry; they are not false-positive waivers. Incidental style noise may
#    be corrected only when the tested shape is preserved. No broad fixture
#    exclusion, deleted failing case or coverage suppression is introduced
#    here. Configuring that external fixture-aware quality gate remains
#    outstanding."
#
# This is that configuration, and this is the gate that keeps it honest:
#
#   1. Every diagnostic SonarCloud is told to ignore is DECLARED, with an
#      exact rule key, an exact file path, a reason, an owner and an expiry.
#   2. Every declaration has a matching Sonar criterion, and every Sonar
#      criterion has a matching declaration. Neither side may grow alone.
#   3. Every criterion names a WHOLE FILE and a WHOLE RULE. A pattern -
#      `fixtures/**` or any other glob, `php:*` or any other wildcard - is
#      refused: a broad waiver is the thing this gate exists to prevent,
#      and the fixture corpus must still be analysed.
#   4. Every declaration names a file that exists, and every expiry is a
#      real calendar date strictly in the future. An expired waiver must be
#      re-approved, not inherited.
#   5. `sonar.issue.ignore.multicriteria` is assigned exactly once, with a
#      comma-separated list. A properties file keeps only the LAST
#      assignment of a key, so a file that assigns it once per criterion
#      silently ignores every criterion but the last.
#
# A NEW diagnostic in a fixture is therefore not silently waived: it fails
# the SonarCloud check, and the fix is to declare it (with a reason and an
# expiry) or to correct the fixture. Neither is automatic.
#
# EXIT CODES
#   0  the declaration and the Sonar configuration agree, and every waiver is
#      justified, exact and unexpired
#   1  a violation; every violation is named
#   2  NO CHECK WAS PERFORMED — a tool is missing or a file is unreadable
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

DECLARATION="ci/sonar-fixture-diagnostics.kyaml"
PROPERTIES="sonar-project.properties"

fail=0

die() {
  echo "sonar-fixture-gate: $*" >&2
  echo "sonar-fixture-gate: NO CHECK WAS PERFORMED - a skip is not a pass." >&2
  exit 2
}

command -v yq >/dev/null 2>&1 || die "yq is not on PATH"
command -v jq >/dev/null 2>&1 || die "jq is not on PATH"
[ -f "$DECLARATION" ] || die "$DECLARATION is missing"
[ -f "$PROPERTIES" ] || die "$PROPERTIES is missing"

TODAY="$(date -u +%Y-%m-%d)"

# The Sonar criteria, read from the properties file. The criterion set is
# declared by `sonar.issue.ignore.multicriteria=`; each member then has a
# `.ruleKey` and a `.resourceKey`.
#
# THE PROPERTY IS ASSIGNED EXACTLY ONCE. A properties file gives the LAST
# assignment of a key precedence, so declaring the property once per
# criterion would leave only the final criterion in force and drop the rest
# without a word. The list is therefore one comma-separated value, and this
# gate refuses a file that assigns the property more than once.
mapfile -t MULTICRITERIA_LINES < <(sed -n 's/^sonar\.issue\.ignore\.multicriteria=//p' "$PROPERTIES")
if [ "${#MULTICRITERIA_LINES[@]}" -ne 1 ]; then
  echo "::error::$PROPERTIES::sonar-fixture-gate: the file assigns 'sonar.issue.ignore.multicriteria' ${#MULTICRITERIA_LINES[@]} times. Only the LAST assignment of a property survives, so the criteria must be one comma-separated value on a single line." >&2
  fail=1
fi
CRITERION_IDS=()
if [ "${#MULTICRITERIA_LINES[@]}" -gt 0 ]; then
  IFS=',' read -r -a CRITERION_IDS <<<"${MULTICRITERIA_LINES[0]}"
fi

declare -A SONAR_RULE=()
declare -A SONAR_PATH=()
for id in "${CRITERION_IDS[@]}"; do
  [ -n "$id" ] || continue
  rule="$(sed -n "s/^sonar\.issue\.ignore\.multicriteria\.$id\.ruleKey=//p" "$PROPERTIES")"
  path="$(sed -n "s/^sonar\.issue\.ignore\.multicriteria\.$id\.resourceKey=//p" "$PROPERTIES")"
  if [ -z "$rule" ] || [ -z "$path" ]; then
    echo "::error::$PROPERTIES::sonar-fixture-gate: criterion '$id' has no ruleKey or no resourceKey." >&2
    fail=1
    continue
  fi
  # A wildcard rule key waives a whole rule wherever it fires. This gate
  # exists to refuse broad waivers, and a wildcard is the same move as a
  # glob, so it is refused here rather than in the declaration only.
  if [[ "$rule" == *'*'* || "$rule" == *'?'* ]]; then
    echo "::error::$PROPERTIES::sonar-fixture-gate: criterion '$id' names the rule key '$rule', which is a WILDCARD. Name one whole rule; a wildcard waives that rule everywhere." >&2
    fail=1
    continue
  fi
  SONAR_RULE["$id"]="$rule"
  SONAR_PATH["$id"]="$path"
done

# The declaration.
# Converted ONCE with yq, then queried with jq: `yq -o=json '.diagnostics[]'`
# emits one JSON document per diagnostic, and whether yq separates those
# documents is not a property this gate should depend on.
DECLARATION_JSON="$(yq -p yaml -o=json '.' "$DECLARATION")"
mapfile -t DECLARED < <(jq -c '.diagnostics[]?' <<<"$DECLARATION_JSON")

declare -A DECLARED_RULE=()
declare -A DECLARED_PATH=()
declare -A DECLARED_REASON=()
declare -A DECLARED_OWNER=()
declare -A DECLARED_EXPIRES=()

for entry in "${DECLARED[@]}"; do
  [ -n "$entry" ] || continue
  rule="$(jq -r '.rule' <<<"$entry")"
  path="$(jq -r '.path' <<<"$entry")"
  reason="$(jq -r '.reason // ""' <<<"$entry")"
  owner="$(jq -r '.owner // ""' <<<"$entry")"
  expires="$(jq -r '.expires // ""' <<<"$entry")"

  if [ -z "$reason" ]; then
    echo "::error file=$path::sonar-fixture-gate: the waiver for $rule has no reason. A waiver without a reason is a suppression." >&2
    fail=1
  fi
  if [ -z "$owner" ]; then
    echo "::error file=$path::sonar-fixture-gate: the waiver for $rule has no owner. A waiver without an owner is a suppression." >&2
    fail=1
  fi
  if [ -z "$expires" ]; then
    echo "::error file=$path::sonar-fixture-gate: the waiver for $rule has no expiry." >&2
    fail=1
  elif ! [[ "$expires" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
    echo "::error file=$path::sonar-fixture-gate: the waiver for $rule expires on '$expires', which is not a date in YYYY-MM-DD form. An expiry that cannot be read is not an expiry." >&2
    fail=1
  elif ! date -u -d "$expires" +%Y-%m-%d >/dev/null 2>&1; then
    echo "::error file=$path::sonar-fixture-gate: the waiver for $rule expires on '$expires', which is not a real calendar date." >&2
    fail=1
  elif [[ "$expires" < "$TODAY" || "$expires" == "$TODAY" ]]; then
    echo "::error file=$path::sonar-fixture-gate: the waiver for $rule expired on $expires (today is $TODAY). Re-approve it with a fresh reason, or correct the fixture." >&2
    fail=1
  fi
  if [ ! -f "$path" ]; then
    echo "::error file=$path::sonar-fixture-gate: the waiver names $path, which does not exist." >&2
    fail=1
  fi
  if [[ "$path" == *'*'* || "$path" == *'?'* ]]; then
    echo "::error file=$path::sonar-fixture-gate: the waiver path is a PATTERN. This gate exists to refuse broad waivers: name the whole file." >&2
    fail=1
  fi
  if [[ "$rule" == *'*'* || "$rule" == *'?'* ]]; then
    echo "::error file=$path::sonar-fixture-gate: the waiver names the rule key '$rule', which is a WILDCARD. Name one whole rule; a wildcard waives that rule everywhere." >&2
    fail=1
  fi
  case "$path" in
    fixtures/*) ;;
    *)
      echo "::error file=$path::sonar-fixture-gate: the waiver names a file outside the fixture corpus. Product code is analysed, not waived." >&2
      fail=1
      ;;
  esac
  # Keyed by rule AND path: one file may carry more than one waived
  # diagnostic, and a second waiver on the same file must not overwrite the
  # first.
  key="$rule|$path"
  DECLARED_RULE["$key"]="$rule"
  DECLARED_PATH["$key"]="$path"
  DECLARED_REASON["$key"]="$reason"
  DECLARED_OWNER["$key"]="$owner"
  DECLARED_EXPIRES["$key"]="$expires"
done

# Every Sonar criterion must be declared, and every declaration must have a
# criterion. A criterion with no declaration is a silent waiver; a
# declaration with no criterion is a promise the configuration does not keep.
declared_pairs="$(for p in "${!DECLARED_RULE[@]}"; do printf '%s\n' "$p"; done | sort -u)"
sonar_pairs="$(for id in "${!SONAR_RULE[@]}"; do printf '%s|%s\n' "${SONAR_RULE[$id]}" "${SONAR_PATH[$id]}"; done | sort -u)"

missing_in_sonar="$(comm -23 <(printf '%s\n' "$declared_pairs") <(printf '%s\n' "$sonar_pairs"))"
missing_in_decl="$(comm -13 <(printf '%s\n' "$declared_pairs") <(printf '%s\n' "$sonar_pairs"))"

if [ -n "$missing_in_sonar" ]; then
  while IFS= read -r pair; do
    [ -n "$pair" ] || continue
    echo "::error::$PROPERTIES::sonar-fixture-gate: the waiver '$pair' is declared but SonarCloud is not told to ignore it, so it would still fail the check." >&2
    fail=1
  done <<<"$missing_in_sonar"
fi

if [ -n "$missing_in_decl" ]; then
  while IFS= read -r pair; do
    [ -n "$pair" ] || continue
    echo "::error::$PROPERTIES::sonar-fixture-gate: SonarCloud is told to ignore '$pair', but no waiver is declared for it. That is a silent suppression." >&2
    fail=1
  done <<<"$missing_in_decl"
fi

echo "  declared waivers: ${#DECLARED_RULE[@]}; Sonar criteria: ${#SONAR_RULE[@]}"

if [ "$fail" -ne 0 ]; then
  echo "sonar-fixture-gate: FAILED" >&2
  exit 1
fi
echo "sonar-fixture-gate: OK - every waiver is exact, justified and unexpired"
exit 0
