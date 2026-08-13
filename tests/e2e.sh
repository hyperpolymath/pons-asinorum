#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Copyright (c) 2026 Jonathan D.A. Jewell (hyperpolymath) <j.d.a.jewell@open.ac.uk>
#
# pons-asinorum — End-to-End Tests
#
# Since pons is currently in planning phase (implementation not started),
# these tests validate the documentation and design artefacts.
#
# Usage:
#   bash tests/e2e.sh
#   just e2e
#
# Merge requirements (STANDING): All 6 test categories must pass before merge:
#   P2P, E2E (this file), aspect, execution, lifecycle, benchmarks

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

# check <label> <expected-substring> <actual>
check() {
    local name="$1" expected="$2" actual="$3"
    if echo "$actual" | grep -q "$expected"; then
        green "  PASS: $name"
        PASS=$((PASS + 1))
    else
        red "  FAIL: $name (expected '$expected', got '${actual:0:120}')"
        FAIL=$((FAIL + 1))
    fi
}

# skip <label> <reason>
skip_test() {
    yellow "  SKIP: $1 ($2)"
    SKIP=$((SKIP + 1))
}

echo "═══════════════════════════════════════════════════════════════"
echo "  pons-asinorum — End-to-End Tests"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# ─── Preflight ───────────────────────────────────────────────────────
bold "Preflight checks"

# Check README exists
if [ -f "$PROJECT_DIR/README.adoc" ]; then
    green "  README.adoc found"
    PASS=$((PASS + 1))
else
    red "  README.adoc not found"
    FAIL=$((FAIL + 1))
fi

# Check LICENSE files
if [ -f "$PROJECT_DIR/LICENSE" ] && [ -f "$PROJECT_DIR/LICENSE.adoc" ]; then
    green "  LICENSE files found"
    PASS=$((PASS + 1))
else
    red "  LICENSE files missing"
    FAIL=$((FAIL + 1))
fi

# Check FUNDING files
if [ -f "$PROJECT_DIR/FUNDING.adoc" ] && [ -f "$PROJECT_DIR/.github/FUNDING.yml" ]; then
    green "  FUNDING files found"
    PASS=$((PASS + 1))
else
    red "  FUNDING files missing"
    FAIL=$((FAIL + 1))
fi

# Check GOVERNANCE
if [ -f "$PROJECT_DIR/GOVERNANCE.md" ]; then
    green "  GOVERNANCE.md found"
    PASS=$((PASS + 1))
else
    red "  GOVERNANCE.md missing"
    FAIL=$((FAIL + 1))
fi

# Check CODE_OF_CONDUCT
if [ -f "$PROJECT_DIR/CODE_OF_CONDUCT.md" ]; then
    green "  CODE_OF_CONDUCT.md found"
    PASS=$((PASS + 1))
else
    red "  CODE_OF_CONDUCT.md missing"
    FAIL=$((FAIL + 1))
fi

echo ""

# ─── Documentation Structure ─────────────────────────────────────────
bold "Documentation structure checks"

# Check ADRs exist
ADR_COUNT=$(find "$PROJECT_DIR/docs/adr" -name "*.adoc" 2>/dev/null | wc -l)
if [ "$ADR_COUNT" -ge 4 ]; then
    green "  ADR directory has $ADR_COUNT ADRs"
    PASS=$((PASS + 1))
else
    red "  Expected at least 4 ADRs, found $ADR_COUNT"
    FAIL=$((FAIL + 1))
fi

# Check PLAN.adoc
if [ -f "$PROJECT_DIR/docs/PLAN.adoc" ]; then
    green "  PLAN.adoc found"
    PASS=$((PASS + 1))
else
    red "  PLAN.adoc missing"
    FAIL=$((FAIL + 1))
fi

# Check pons-kickoff.adoc
if [ -f "$PROJECT_DIR/docs/pons-kickoff.adoc" ]; then
    green "  pons-kickoff.adoc found"
    PASS=$((PASS + 1))
else
    red "  pons-kickoff.adoc missing"
    FAIL=$((FAIL + 1))
fi

echo ""

# ─── Machine Readable Structure ──────────────────────────────────────
bold "Machine readable structure checks"

# Check .machine_readable exists
if [ -d "$PROJECT_DIR/.machine_readable" ]; then
    green "  .machine_readable/ directory exists"
    PASS=$((PASS + 1))
else
    red "  .machine_readable/ directory missing"
    FAIL=$((FAIL + 1))
fi

# Check contractiles
if [ -d "$PROJECT_DIR/.machine_readable/contractiles" ]; then
    CONTRACTILE_COUNT=$(find "$PROJECT_DIR/.machine_readable/contractiles" -type f | wc -l)
    green "  contractiles/ has $CONTRACTILE_COUNT files"
    PASS=$((PASS + 1))
else
    red "  contractiles/ directory missing"
    FAIL=$((FAIL + 1))
fi

# Check descriptiles
if [ -d "$PROJECT_DIR/.machine_readable/descriptiles" ]; then
    DESCRIPTILE_COUNT=$(find "$PROJECT_DIR/.machine_readable/descriptiles" -type f | wc -l)
    green "  descriptiles/ has $DESCRIPTILE_COUNT files"
    PASS=$((PASS + 1))
else
    red "  descriptiles/ directory missing"
    FAIL=$((FAIL + 1))
fi

# Check scripts
if [ -d "$PROJECT_DIR/.machine_readable/scripts" ]; then
    SCRIPT_COUNT=$(find "$PROJECT_DIR/.machine_readable/scripts" -type f | wc -l)
    green "  scripts/ has $SCRIPT_COUNT files"
    PASS=$((PASS + 1))
else
    red "  scripts/ directory missing"
    FAIL=$((FAIL + 1))
fi

# Check 0-AI-MANIFEST.a2ml
if [ -f "$PROJECT_DIR/.machine_readable/0-AI-MANIFEST.a2ml" ]; then
    MANIFEST_CONTENT=$(cat "$PROJECT_DIR/.machine_readable/0-AI-MANIFEST.a2ml")
    check "0-AI-MANIFEST has pons-asinorum" "pons-asinorum" "$MANIFEST_CONTENT"
else
    red "  0-AI-MANIFEST.a2ml missing"
    FAIL=$((FAIL + 1))
fi

echo ""

# ─── RSR Compliance ─────────────────────────────────────────────────
bold "RSR compliance checks"

# Check for SPDX headers in key files
for file in README.adoc LICENSE.adoc FUNDING.adoc EXPLAINME.adoc; do
    if [ -f "$PROJECT_DIR/$file" ]; then
        if grep -q "SPDX-License-Identifier" "$PROJECT_DIR/$file"; then
            green "  $file has SPDX header"
            PASS=$((PASS + 1))
        else
            red "  $file missing SPDX header"
            FAIL=$((FAIL + 1))
        fi
    fi
done

# Check CODEOWNERS policy (Rule 1: no owner lines for solo-maintained)
if [ -f "$PROJECT_DIR/.github/CODEOWNERS" ]; then
    CODEOWNERS_CONTENT=$(cat "$PROJECT_DIR/.github/CODEOWNERS")
    if echo "$CODEOWNERS_CONTENT" | grep -q "Solo-maintained.*no owner lines"; then
        green "  CODEOWNERS follows Rule 1 (no owner lines)"
        PASS=$((PASS + 1))
    else
        red "  CODEOWNERS does not follow Rule 1"
        FAIL=$((FAIL + 1))
    fi
else
    red "  CODEOWNERS missing"
    FAIL=$((FAIL + 1))
fi

echo ""

# ─── Template instantiation ────────────────────────────────────────────
INSTANTIATION_TEST="$(dirname "$0")/e2e/template_instantiation_test.sh"
if [ -f "$INSTANTIATION_TEST" ]; then
    echo ""
    echo "── Template instantiation ─────────────────────────────────────"
    if bash "$INSTANTIATION_TEST" "$PROJECT_DIR"; then
        green "PASS: template instantiation"
        PASS=$((PASS + 1))
    else
        red "FAIL: template instantiation"
        FAIL=$((FAIL + 1))
    fi
else
    yellow "SKIP: template instantiation (test not found)"
    SKIP=$((SKIP + 1))
fi

# ═══════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Results: "
printf "PASS=%d  " "$PASS"
if [ "$FAIL" -gt 0 ]; then printf "FAIL=%d  " "$FAIL"; else printf "FAIL=0  "; fi
if [ "$SKIP" -gt 0 ]; then printf "SKIP=%d" "$SKIP"; else printf "SKIP=0"; fi
echo "═══════════════════════════════════════════════════════════════"

exit "$FAIL"
