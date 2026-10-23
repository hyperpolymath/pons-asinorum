#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Documentation/CLI drift gate. A documented command or flag that the binary
# does not accept is a correctness defect for a tool whose contract is "never
# dress a heuristic up as a proof". A fake flag appended to USAGE must make
# this script fail; tests/e2e.sh exercises that mutant.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

PONS_BIN="${PONS_BIN:-}"
if [[ -z "$PONS_BIN" ]]; then
    if [[ -x "$PROJECT_DIR/target/release/pons" ]]; then
        PONS_BIN="$PROJECT_DIR/target/release/pons"
    elif [[ -x "$PROJECT_DIR/target/debug/pons" ]]; then
        PONS_BIN="$PROJECT_DIR/target/debug/pons"
    else
        echo "doc-drift: pons binary not found; build it or set PONS_BIN" >&2
        exit 1
    fi
fi

USAGE="${PONS_USAGE:-$PROJECT_DIR/docs/USAGE.adoc}"
MAN="${PONS_MAN:-$PROJECT_DIR/docs/man/pons.1}"
CATALOGUE="${PONS_CATALOGUE:-$PROJECT_DIR/docs/catalogue.adoc}"

fail() {
    echo "doc-drift: $1" >&2
    exit 1
}

HELP="$("$PONS_BIN" --help)"
SCAN_HELP="$("$PONS_BIN" scan --help)"
TRACE_HELP="$("$PONS_BIN" trace --help)"
GENERATED_CATALOGUE="$("$PONS_BIN" catalogue)"

for cmd in scan catalogue man doctor trace; do
    grep -qE "(^|[[:space:]])${cmd}([[:space:]]|$)" <<<"$HELP" \
        || fail "pons --help does not document '$cmd'"
done

# Every backticked `pons <command>` in USAGE is either present in --help or on
# a line explicitly marked planned.
mapfile -t documented_commands < <(grep -oE '`pons [a-z-]+' "$USAGE" | sed 's/`pons //' | sort -u)
[[ "${#documented_commands[@]}" -gt 0 ]] || fail "no commands found in USAGE; the check would be vacuous"
for cmd in "${documented_commands[@]}"; do
    if grep -F "\`pons $cmd" "$USAGE" | grep -qv 'planned'; then
        grep -qE "(^|[[:space:]])${cmd}([[:space:]]|$)" <<<"$HELP" \
            || fail "USAGE documents 'pons $cmd' as available, but --help does not"
    fi
done

# Every long flag in USAGE must be accepted by scan or trace unless its only
# occurrences are on planned lines. This is the mutant that catches a fake flag.
mapfile -t documented_flags < <(grep -oE -- '--[a-z][a-z-]+' "$USAGE" | sort -u)
[[ "${#documented_flags[@]}" -gt 0 ]] || fail "no flags found in USAGE; the check would be vacuous"
for flag in "${documented_flags[@]}"; do
    if ! grep -F -- "$flag" "$USAGE" | grep -qv 'planned'; then
        continue
    fi
    grep -Fq -- "$flag" <<<"$SCAN_HELP" \
        || grep -Fq -- "$flag" <<<"$TRACE_HELP" \
        || grep -Fq -- "$flag" <<<"$HELP" \
        || fail "USAGE documents '$flag', but neither scan --help nor trace --help accepts it"
done

# Planned commands must not accidentally be advertised as present.
for cmd in challenge extract spell search; do
    if grep -qE "(^|[[:space:]])${cmd}([[:space:]]|$)" <<<"$HELP"; then
        fail "planned command '$cmd' is present in --help; update the documentation status"
    fi
    grep -qi 'planned' "$USAGE" || fail "USAGE does not mark any planned work"
done

# The committed catalogue must list exactly the live registry's rule IDs.
generated_ids="$(sed -n 's/^== [0-9][0-9]*\. `\(.*\)`$/\1/p' <<<"$GENERATED_CATALOGUE" | sort)"
documented_ids="$(sed -n 's/^== [0-9][0-9]*\. `\(.*\)`$/\1/p' "$CATALOGUE" | sort)"
[[ -n "$generated_ids" ]] || fail "pons catalogue produced no rule IDs; check would be vacuous"
[[ -n "$documented_ids" ]] || fail "docs/catalogue.adoc contains no rule IDs; check would be vacuous"
diff -u <(printf '%s\n' "$generated_ids") <(printf '%s\n' "$documented_ids") \
    || fail "docs/catalogue.adoc rule IDs differ from the live registry"

grep -q 'planned' "$MAN" || fail "man page does not mark deferred commands as planned"

echo "doc-drift: help, USAGE, man and catalogue agree with the binary"
