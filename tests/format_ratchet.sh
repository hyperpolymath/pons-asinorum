#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# format_ratchet.sh — the closed-world format gate for this repository.
#
# It enforces two estate policies, and it enforces them as INVENTORIES
# rather than as spot checks, because a spot check cannot notice a file that
# was added without a decision:
#
#   1. YAML (standards 3-practice/YAML-POLICY.adoc, D280 + D307).
#      Every `.yml`, `.yaml` and `.kyaml` file in this repository must be
#      accounted for in `ci/yaml-ratchet.kyaml`: either it is in
#      `converted` — and then its bytes must be exactly what yq's KYAML
#      encoder produces — or it is in `excluded` with a stated reason.
#      A YAML file that appears in neither is a FAILURE, not a silent pass.
#
#   2. JSON (standards 3-practice/JSON-POLICY.adoc, owner ruling D306).
#      Every `.json` and `.jsonl` file must be VALID I-JSON (RFC 7493) —
#      no duplicate keys, no NaN or Infinity, UTF-8 throughout, integers
#      inside the IEEE-754 double range. Validity is enforced for ALL of
#      them. Canonicality (RFC 8785) is enforced for the files listed in
#      `canonical`; the rest are declared non-canonical with a reason and
#      are reported, not tolerated silently.
#
# WHY AN INVENTORY AND NOT A SCAN
# The estate's own gate (standards: scripts/check-ijson-jcs.sh) reports
# every non-canonical file and exits 0. That is the right default for an
# estate where most repositories have not converted: a report is
# information, not assurance. But a repository that has made a decision
# about each of its files should hold that decision, and a file added
# without a decision is precisely how a policy decays. So this gate is
# CLOSED: the inventory is the list, and the tree must equal it.
#
# EXIT CODES (the estate's convention)
#   0  every file is accounted for, every JSON file is valid I-JSON, and
#      every canonical file is canonical
#   1  a policy violation; every violation is named
#   2  NO CHECK WAS PERFORMED — a required tool is missing, or a ratchet
#      file is missing or unreadable. A skip is not a pass.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

YAML_RATCHET="ci/yaml-ratchet.kyaml"
JSON_RATCHET="ci/json-ratchet.kyaml"

fail=0

die() {
  echo "format-ratchet: $*" >&2
  echo "format-ratchet: NO CHECK WAS PERFORMED - a skip is not a pass." >&2
  exit 2
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "$1 is not on PATH"
}

need yq
need git

IJSON_JCS="${IJSON_JCS:-ijson-jcs}"
command -v "$IJSON_JCS" >/dev/null 2>&1 || die "$IJSON_JCS is not on PATH"

[ -f "$YAML_RATCHET" ] || die "$YAML_RATCHET is missing"
[ -f "$JSON_RATCHET" ] || die "$JSON_RATCHET is missing"

# The verdict and the tool's own report for the file last classified. They
# are globals rather than return values so that the classifier can report
# WHY a file was rejected.
IJSON_JCS_VERDICT=""
IJSON_JCS_REPORT=""

# classify_json FILE — set IJSON_JCS_VERDICT to OK, NOT CANONICAL or
# INVALID.
#
# `ijson-jcs check` prints `OK <path>`, `NOT CANONICAL <path>` or
# `INVALID <path>: reason`, and exits 0, 1 or 2. The printed verdict and
# the exit status must AGREE, and only those three verdicts are accepted:
# a usage error, a tool that died, or a verdict this gate does not know is
# INVALID. The distinction that matters is INVALID (not I-JSON) versus NOT
# CANONICAL (valid, but not RFC 8785 bytes) — and a file nobody could
# classify belongs to neither side of that distinction.
#
# `--` ends the tool's option parsing, so a path that begins with a hyphen
# is still read as a path.
classify_json() {
  local file="$1" report status
  report="$("$IJSON_JCS" check -- "$file" 2>&1)"
  status=$?
  IJSON_JCS_REPORT="$report"
  case "$report" in
    "OK "*)
      IJSON_JCS_VERDICT=OK
      [ "$status" -eq 0 ] || IJSON_JCS_VERDICT=INVALID
      ;;
    "NOT CANONICAL "*)
      IJSON_JCS_VERDICT=NOT_CANONICAL
      [ "$status" -eq 1 ] || IJSON_JCS_VERDICT=INVALID
      ;;
    "INVALID "*)
      IJSON_JCS_VERDICT=INVALID
      ;;
    *)
      IJSON_JCS_VERDICT=INVALID
      ;;
  esac
}

# classify_jsonl FILE — a JSON Lines file is a SEQUENCE OF RECORDS, not one
# document, and `ijson-jcs` reads whole documents only. Each line is
# therefore canonicalised on its own with `ijson-jcs canon`, exactly as the
# estate's own scripts/check-ijson-jcs.sh does. A blank or unparseable
# line is INVALID; a line whose canonical form differs, or a missing final
# newline, is NOT CANONICAL.
classify_jsonl() {
  local file="$1" line canon verdict=OK
  if [ -s "$file" ] && [ "$(tail -c 1 "$file" | od -An -c | tr -d ' \n')" != '\n' ]; then
    verdict=NOT_CANONICAL
  fi
  while IFS= read -r line || [ -n "$line" ]; do
    if [ -z "$line" ]; then
      IJSON_JCS_VERDICT=INVALID
      IJSON_JCS_REPORT="INVALID $file: a JSON Lines file may not contain a blank line"
      return
    fi
    if ! canon="$(printf '%s' "$line" | "$IJSON_JCS" canon 2>&1)"; then
      IJSON_JCS_VERDICT=INVALID
      IJSON_JCS_REPORT="INVALID $file: a record is not valid I-JSON: $canon"
      return
    fi
    if [ "$canon" != "$line" ]; then
      verdict=NOT_CANONICAL
    fi
  done <"$file"
  IJSON_JCS_VERDICT="$verdict"
  IJSON_JCS_REPORT="$verdict $file"
}

# yq must be able to WRITE kyaml, or the "is it KYAML?" question has no
# answer and the check would pass by saying nothing. The estate's
# kyaml-format.sh refuses (exit 2) in the same situation.
if ! yq -p yaml -o kyaml '.' <<< '{}' >/dev/null 2>&1; then
  die "this yq cannot write kyaml (-o kyaml); the KYAML check cannot be performed"
fi

echo "== YAML inventory =="

mapfile -t YAML_CONVERTED < <(yq -p yaml -r '.converted[]' "$YAML_RATCHET")
mapfile -t YAML_EXCLUDED < <(yq -p yaml -r '.excluded[].paths[]' "$YAML_RATCHET")
mapfile -t YAML_TREE < <(git ls-files -- '*.yml' '*.yaml' '*.kyaml')

# Accounted set, as an associative array for O(1) membership.
declare -A YAML_ACCOUNTED=()
for p in "${YAML_CONVERTED[@]}" "${YAML_EXCLUDED[@]}"; do
  [ -n "$p" ] || continue
  YAML_ACCOUNTED["$p"]=1
done

for f in "${YAML_TREE[@]}"; do
  if [ -z "${YAML_ACCOUNTED["$f"]+x}" ]; then
    echo "::error file=$f::format-ratchet: $f is a YAML file that no ratchet accounts for. Add it to $YAML_RATCHET as converted (if it is KYAML) or as excluded (with a reason)." >&2
    fail=1
  fi
done

for p in "${YAML_CONVERTED[@]}" "${YAML_EXCLUDED[@]}"; do
  [ -n "$p" ] || continue
  if [ ! -f "$p" ]; then
    echo "::error file=$p::format-ratchet: $p is listed in $YAML_RATCHET but does not exist. A stale entry is how an inventory stops describing the tree." >&2
    fail=1
  fi
done

# Every CONVERTED file must be exactly yq's KYAML rendering. This is the
# same comparison the estate's scripts/kyaml-format.sh --check makes; it is
# repeated here so the gate is runnable without fetching that script.
for p in "${YAML_CONVERTED[@]}"; do
  [ -n "$p" ] || continue
  [ -f "$p" ] || continue
  if ! yq -p yaml -o kyaml '.' "$p" | cmp -s - "$p"; then
    echo "::error file=$p::format-ratchet: $p is not KYAML. Rewrite it with yq -p yaml -o kyaml '.' (standards: scripts/kyaml-format.sh)." >&2
    fail=1
  fi
done

echo "  yaml files in tree: ${#YAML_TREE[@]}; converted: ${#YAML_CONVERTED[@]}; excluded: ${#YAML_EXCLUDED[@]}"

echo "== JSON inventory =="

mapfile -t JSON_CANONICAL < <(yq -p yaml -r '.canonical[]' "$JSON_RATCHET")
mapfile -t JSON_NONCANONICAL_PATTERNS < <(yq -p yaml -r '.declared_non_canonical[].paths[]' "$JSON_RATCHET")
mapfile -t JSON_TREE < <(git ls-files -- '*.json' '*.jsonl')

declare -A JSON_IS_CANONICAL=()
for p in "${JSON_CANONICAL[@]}"; do
  [ -n "$p" ] || continue
  JSON_IS_CANONICAL["$p"]=1
done

json_matches_pattern() {
  local file="$1" pattern="$2"
  # Bash pattern matching treats `*` as matching `/` too, so one `*` covers a
  # whole subtree. That is what the estate's own ignore lists assume.
  # `[[ == ]]` is used rather than `case`, because an unquoted expansion in
  # a `case` pattern is a glob that can also match a literal: the pattern
  # here is meant to be a glob.
  [[ "$file" == $pattern ]]
}

json_is_declared_non_canonical() {
  local file="$1" pattern
  for pattern in "${JSON_NONCANONICAL_PATTERNS[@]}"; do
    [ -n "$pattern" ] || continue
    if json_matches_pattern "$file" "$pattern"; then
      return 0
    fi
  done
  return 1
}

invalid=0
canonical_ok=0
canonical_bad=0
promotable=0
declared_non_canonical=0
accounted_json=0

for f in "${JSON_TREE[@]}"; do
  if [ -n "${JSON_IS_CANONICAL["$f"]+x}" ]; then
    accounted_json=$((accounted_json + 1))
  elif json_is_declared_non_canonical "$f"; then
    accounted_json=$((accounted_json + 1))
  else
    echo "::error file=$f::format-ratchet: $f is a JSON file that no ratchet accounts for. Add it to $JSON_RATCHET as canonical (if it is RFC 8785 canonical) or as declared non-canonical (with a reason)." >&2
    fail=1
    continue
  fi

  # The estate file profile requires exactly one final LF (standards
  # JSON-POLICY §3). `ijson-jcs check` accepts canonical bytes with no
  # trailing newline, so the framing is checked here rather than inferred.
  last="$(tail -c 1 "$f" | od -An -c | tr -d ' \n')"
  if [ "$last" != "\\n" ]; then
    echo "::error file=$f::format-ratchet: $f does not end with a final LF." >&2
    fail=1
    continue
  fi
  if [ "$(tail -c 2 "$f" | od -An -c | tr -d ' \n')" = "\\n\\n" ]; then
    echo "::error file=$f::format-ratchet: $f ends with more than one LF. The profile is exactly one." >&2
    fail=1
    continue
  fi

  # The verdict comes from the classifier, which accepts only the three
  # documented verdicts and requires the tool's exit status to agree with
  # the verdict it printed.
  case "$f" in
    *.jsonl) classify_jsonl "$f" ;;
    *) classify_json "$f" ;;
  esac
  case "$IJSON_JCS_VERDICT" in
    INVALID)
      echo "::error file=$f::format-ratchet: $f is not valid I-JSON (RFC 7493): ${IJSON_JCS_REPORT#INVALID $f: }" >&2
      invalid=$((invalid + 1))
      fail=1
      continue
      ;;
    NOT_CANONICAL)
      # Valid, but the bytes are not RFC 8785 canonical.
      if [ -n "${JSON_IS_CANONICAL["$f"]+x}" ]; then
        echo "::error file=$f::format-ratchet: $f is listed as canonical but its bytes are not RFC 8785 canonical. Run: ijson-jcs fix $f" >&2
        canonical_bad=$((canonical_bad + 1))
        fail=1
      else
        declared_non_canonical=$((declared_non_canonical + 1))
      fi
      ;;
    OK)
      canonical_ok=$((canonical_ok + 1))
      if [ -z "${JSON_IS_CANONICAL["$f"]+x}" ]; then
        echo "::notice file=$f::format-ratchet: $f is already RFC 8785 canonical but is declared non-canonical. Move it to 'canonical' in $JSON_RATCHET." >&2
        promotable=$((promotable + 1))
      fi
      ;;
    *)
      # Unreachable: the classifier only ever sets one of the three.
      echo "::error file=$f::format-ratchet: $f was not classified (verdict '$IJSON_JCS_VERDICT')." >&2
      fail=1
      ;;
  esac
done

for p in "${JSON_CANONICAL[@]}"; do
  [ -n "$p" ] || continue
  if [ ! -f "$p" ]; then
    echo "::error file=$p::format-ratchet: $p is listed as canonical in $JSON_RATCHET but does not exist." >&2
    fail=1
  fi
done

echo "  json files in tree: ${#JSON_TREE[@]}; accounted: ${accounted_json}; already canonical: ${canonical_ok}; invalid: ${invalid}; declared-canonical but not canonical: ${canonical_bad}; declared non-canonical: ${declared_non_canonical}; promotable (canonical but declared otherwise): ${promotable}"

if [ "$fail" -ne 0 ]; then
  echo "format-ratchet: FAILED" >&2
  exit 1
fi
echo "format-ratchet: OK"
exit 0
