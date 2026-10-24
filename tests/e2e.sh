#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# pons-asinorum — End-to-End Tests
#
# Two halves, and the second is the one that matters:
#
#   1. Repository shape — the governance and documentation artefacts exist and
#      say what they are supposed to say.
#   2. Binary behaviour — the `pons` binary is actually executed and its
#      observable contract asserted: --version, the ADR-0005 exit-code matrix,
#      a known-answer positive corpus, and the falsification invariant.
#
# Half 1 alone is a vacuous gate: it can pass on a tree where the tool does not
# build. Half 2 is what stops that. If you add checks, add them to half 2.
#
# Usage:
#   bash tests/e2e.sh          # uses target/release/pons, else target/debug/pons
#   PONS_BIN=/path/to/pons bash tests/e2e.sh
#   just e2e                   # builds first, then runs this
#
# Non-vacuity self-check (run it after changing half 2 — it MUST go red):
#   PONS_BIN=/bin/true bash tests/e2e.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

PASS=0
FAIL=0
SKIP=0

# ─── Colour helpers ──────────────────────────────────────────────────
green() { printf '\033[32m%s\033[0m\n' "$*"; }
red()   { printf '\033[31m%s\033[0m\n' "$*"; }
yellow(){ printf '\033[33m%s\033[0m\n' "$*"; }
bold()  { printf '\033[1m%s\033[0m\n' "$*"; }

# ─── Assertion helpers ───────────────────────────────────────────────

ok()   { green "  PASS: $1"; PASS=$((PASS + 1)); }
bad()  { red   "  FAIL: $1"; FAIL=$((FAIL + 1)); }

# check <label> <expected-substring> <actual>
check() {
    local name="$1" expected="$2" actual="$3"
    if echo "$actual" | grep -q "$expected"; then
        ok "$name"
    else
        red "  FAIL: $name (expected '$expected', got '${actual:0:120}')"
        FAIL=$((FAIL + 1))
    fi
}

# check_absent <label> <forbidden-substring> <actual>
check_absent() {
    local name="$1" forbidden="$2" actual="$3"
    if echo "$actual" | grep -q "$forbidden"; then
        red "  FAIL: $name (found forbidden '$forbidden')"
        FAIL=$((FAIL + 1))
    else
        ok "$name"
    fi
}

# expect_rc <label> <want> <got> — numeric, NOT substring. `grep -q 1` would
# happily match an rc of 12; exit codes are compared with -eq or not at all.
expect_rc() {
    local name="$1" want="$2" got="$3"
    if [ "$got" -eq "$want" ]; then
        ok "$name (rc=$got)"
    else
        red "  FAIL: $name (expected rc=$want, got rc=$got)"
        FAIL=$((FAIL + 1))
    fi
}

# have <label> <path...> — every path must exist
have() {
    local name="$1"; shift
    local missing=""
    for p in "$@"; do [ -e "$PROJECT_DIR/$p" ] || missing="$missing $p"; done
    if [ -z "$missing" ]; then ok "$name"; else bad "$name (missing:$missing)"; fi
}

# run_pons <args...> — echoes output, records rc via pons_rc. Never trips
# `set -e`, because a non-zero exit is the thing under test, not a script
# failure.
#
# The rc travels through a file, not a shell variable: `VAR=$(run_pons ...)`
# runs run_pons in a subshell, so an RC variable set inside it is lost and the
# caller silently reads a stale rc. This bit the ADR-0005 exit-code matrix —
# both default scans exited 0 but were recorded as rc=1 left over from an
# earlier check, while the earlier checks passed vacuously on stale zeros.
PONS_RC_FILE="$(mktemp "${TMPDIR:-/tmp}/pons-e2e-rc.XXXXXX")"
trap 'rm -f "$PONS_RC_FILE"' EXIT

run_pons() {
    local out
    set +e
    out="$("$PONS_BIN" "$@" 2>&1)"
    printf '%s' "$?" > "$PONS_RC_FILE"
    set -e
    printf '%s' "$out"
}

# pons_rc — the exit status of the most recent run_pons call, direct or in a
# command substitution. Compare numerically only.
pons_rc() { cat "$PONS_RC_FILE"; }

echo "═══════════════════════════════════════════════════════════════"
echo "  pons-asinorum — End-to-End Tests"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# ─── Preflight ───────────────────────────────────────────────────────
bold "Preflight checks"

have "README.adoc present"          README.adoc
have "LICENSE files present"        LICENSE LICENSE.adoc
have "FUNDING files present"        FUNDING.adoc .github/FUNDING.yml
have "GOVERNANCE.adoc present"      GOVERNANCE.adoc
have "CODE_OF_CONDUCT.adoc present" CODE_OF_CONDUCT.adoc
have "MAINTAINERS present"          MAINTAINERS

echo ""

# ─── Documentation Structure ─────────────────────────────────────────
bold "Documentation structure checks"

ADR_COUNT=$(find "$PROJECT_DIR/docs/adr" -name "*.adoc" 2>/dev/null | wc -l)
if [ "$ADR_COUNT" -ge 5 ]; then
    ok "ADR directory has $ADR_COUNT ADRs (>= 5)"
else
    bad "Expected at least 5 ADRs, found $ADR_COUNT"
fi

have "PLAN.adoc present"          docs/PLAN.adoc
have "pons-kickoff.adoc present"  docs/pons-kickoff.adoc

# The docs must not re-assert the planning-phase claim that was true before M0-M2
# landed. A stale status line is the defect this check exists to catch.
README_TXT=$(cat "$PROJECT_DIR/README.adoc")
check_absent "README does not claim implementation not started" "implementation not started" "$README_TXT"

echo ""

# ─── Machine Readable Structure ──────────────────────────────────────
bold "Machine readable structure checks"

# NOTE: there is deliberately no assertion on `0-AI-MANIFEST.a2ml` here.
# A2ML was retired as a format; a test that asserts its presence would pin this
# repo to it. Whether the file is removed is a separate governance decision —
# this suite simply declines to require it.

have ".machine_readable/ present"  .machine_readable
have "contractiles/ present"       .machine_readable/contractiles
have "descriptiles/ present"       .machine_readable/descriptiles
have "scripts/ present"            .machine_readable/scripts

echo ""

# ─── RSR Compliance ─────────────────────────────────────────────────
bold "RSR compliance checks"

for file in README.adoc LICENSE.adoc FUNDING.adoc EXPLAINME.adoc; do
    if [ -f "$PROJECT_DIR/$file" ]; then
        if grep -q "SPDX-License-Identifier" "$PROJECT_DIR/$file"; then
            ok "$file has SPDX header"
        else
            bad "$file missing SPDX header"
        fi
    fi
done

# CODEOWNERS policy, Rule 1: a solo-maintained repo carries no owner lines.
if [ -f "$PROJECT_DIR/.github/CODEOWNERS" ]; then
    CODEOWNERS_CONTENT=$(cat "$PROJECT_DIR/.github/CODEOWNERS")
    check "CODEOWNERS follows Rule 1 (no owner lines)" "Solo-maintained.*no owner lines" "$CODEOWNERS_CONTENT"
else
    bad "CODEOWNERS missing"
fi

echo ""

# ═══════════════════════════════════════════════════════════════════════
#   Binary behaviour — the half that is not vacuous
# ═══════════════════════════════════════════════════════════════════════
bold "Binary behaviour checks"

if [ -n "${PONS_BIN:-}" ]; then
    :
elif [ -x "$PROJECT_DIR/target/release/pons" ]; then
    PONS_BIN="$PROJECT_DIR/target/release/pons"
elif [ -x "$PROJECT_DIR/target/debug/pons" ]; then
    PONS_BIN="$PROJECT_DIR/target/debug/pons"
else
    PONS_BIN=""
fi

if [ -z "$PONS_BIN" ]; then
    # Deliberately a FAILURE, not a skip. A skip is not a pass: a suite that
    # silently stops exercising the tool when the tool is absent is the exact
    # vacuous gate this half exists to prevent. Run `cargo build` or `just e2e`.
    bad "pons binary not found (build it, or set PONS_BIN)"
else
    echo "  using: $PONS_BIN"

    # --- 1. The tool identifies itself ---------------------------------
    VERSION_OUT=$(run_pons --version); VERSION_RC=$(pons_rc)
    expect_rc "--version exits 0" 0 "$VERSION_RC"
    check "--version prints name and semver" '^pons [0-9]\+\.[0-9]\+\.[0-9]\+' "$VERSION_OUT"

    HELP_OUT=$(run_pons --help); HELP_RC=$(pons_rc)
    expect_rc "--help exits 0" 0 "$HELP_RC"
    check "--help documents the scan subcommand" "scan" "$HELP_OUT"
    check "--help documents the trace subcommand" "trace" "$HELP_OUT"
    check "--help documents the catalogue subcommand" "catalogue" "$HELP_OUT"

    CATALOGUE_OUT=$(run_pons catalogue); CATALOGUE_RC=$(pons_rc)
    expect_rc "catalogue exits 0" 0 "$CATALOGUE_RC"
    check "catalogue lists a registered rule" "dead-store" "$CATALOGUE_OUT"

    MAN_OUT=$(run_pons man); MAN_RC=$(pons_rc)
    expect_rc "man exits 0" 0 "$MAN_RC"
    check "man documents scan" "scan" "$MAN_OUT"

    TRACE_OUT=$(run_pons trace "$PROJECT_DIR/fixtures/protocols/good_trace.json" --protocol "$PROJECT_DIR/protocols/linear-egg.toml"); TRACE_RC=$(pons_rc)
    expect_rc "a valid protocol trace exits 0" 0 "$TRACE_RC"
    check "trace names its protocol" "linear-egg" "$TRACE_OUT"

    run_pons trace "$PROJECT_DIR/fixtures/protocols/bad_trace.json" --protocol "$PROJECT_DIR/protocols/linear-egg.toml" >/dev/null; TRACE_BAD_RC=$(pons_rc)
    expect_rc "a violating protocol trace exits 1" 1 "$TRACE_BAD_RC"

    if PONS_BIN="$PONS_BIN" bash "$SCRIPT_DIR/doc_drift.sh" >/dev/null 2>&1; then
        ok "docs, help and catalogue agree"
    else
        bad "docs/help/catalogue drift gate failed"
    fi

    DRIFT_TMP=$(mktemp -d)
    cp "$PROJECT_DIR/docs/USAGE.adoc" "$DRIFT_TMP/USAGE.adoc"
    printf '\n--definitely-not-a-real-flag\n' >> "$DRIFT_TMP/USAGE.adoc"
    if PONS_BIN="$PONS_BIN" PONS_USAGE="$DRIFT_TMP/USAGE.adoc" bash "$SCRIPT_DIR/doc_drift.sh" >/dev/null 2>&1; then
        bad "doc drift gate accepted a fake flag (vacuous)"
    else
        ok "doc drift gate rejects a fake flag"
    fi
    rm -rf "$DRIFT_TMP"

    # --- 2. Known-answer positive corpus -------------------------------
    # fixtures/<rule>/positive is the committed known-answer control: the rule
    # MUST fire there. A tmpdir would prove less and rot faster.
    POS="fixtures/self-assignment/positive"
    POS_OUT=$(run_pons scan "$PROJECT_DIR/$POS"); POS_RC=$(pons_rc)
    expect_rc "scan of a dirty tree still exits 0 (ADR-0005)" 0 "$POS_RC"
    check "positive corpus fires its rule" "self-assignment" "$POS_OUT"
    check "finding carries a location"     ":[0-9]\+:[0-9]\+" "$POS_OUT"
    check "finding carries an evidence note" "evidence:" "$POS_OUT"
    # The message column must carry prose, not a second copy of the rule id.
    check_absent "message is not a repeat of the rule id" "self-assignment: self-assignment" "$POS_OUT"

    # --- 3. Falsification invariant, at the binary level ----------------
    # The falsifier test drives rules directly and bypasses Engine::scan
    # entirely. This asserts the same invariant through the real binary: a rule
    # that fires anywhere in its own negative corpus is falsified.
    # Other rules MAY fire there (while-true-no-break/negative legitimately
    # trips constant-condition), so the predicate is "its own id", not "silent".
    NEG_VIOLATIONS=""
    NEG_CHECKED=0
    for d in "$PROJECT_DIR"/fixtures/*/negative; do
        [ -d "$d" ] || continue
        rule=$(basename "$(dirname "$d")")
        neg_out=$(run_pons scan "$d")
        NEG_CHECKED=$((NEG_CHECKED + 1))
        if echo "$neg_out" | grep -q "] $rule:"; then
            NEG_VIOLATIONS="$NEG_VIOLATIONS $rule"
        fi
    done
    if [ "$NEG_CHECKED" -eq 0 ]; then
        bad "falsification invariant checked 0 corpora (fixtures missing?)"
    elif [ -z "$NEG_VIOLATIONS" ]; then
        ok "falsification invariant holds across $NEG_CHECKED negative corpora"
    else
        bad "rules fired in their own negative corpus:$NEG_VIOLATIONS"
    fi

    # --- 4. Clean tree ---------------------------------------------------
    CLEAN_OUT=$(run_pons scan "$PROJECT_DIR/crates/pons-cli/src"); CLEAN_RC=$(pons_rc)
    expect_rc "scan of a clean tree exits 0" 0 "$CLEAN_RC"
    check_absent "clean tree reports no findings" "\[WARN\]" "$CLEAN_OUT"

    # --- 5. The ADR-0005 exit-code matrix --------------------------------
    run_pons scan "$PROJECT_DIR/$POS" --fail-on warn  >/dev/null; W_RC=$(pons_rc)
    expect_rc "--fail-on warn with warnings exits 1" 1 "$W_RC"

    run_pons scan "$PROJECT_DIR/$POS" --fail-on error >/dev/null; E_RC=$(pons_rc)
    expect_rc "--fail-on error with only warnings exits 0" 0 "$E_RC"

    run_pons scan "$PROJECT_DIR/crates/pons-cli/src" --fail-on warn >/dev/null; CW_RC=$(pons_rc)
    expect_rc "--fail-on warn on a clean tree exits 0" 0 "$CW_RC"

    run_pons scan "$PROJECT_DIR/does-not-exist-zzz" >/dev/null; MISS_RC=$(pons_rc)
    expect_rc "scan of a nonexistent path exits 2 (operational error)" 2 "$MISS_RC"
fi

# ═══════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "═══════════════════════════════════════════════════════════════"
printf "  Results: PASS=%d  FAIL=%d  SKIP=%d\n" "$PASS" "$FAIL" "$SKIP"
echo "═══════════════════════════════════════════════════════════════"

exit "$FAIL"
