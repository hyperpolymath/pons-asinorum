#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Run only in a disposable checkout. The caller provides evidence/output paths.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
if git symbolic-ref -q HEAD >/dev/null; then
    echo "Refusing mutation tests on an attached branch; use a detached disposable worktree" >&2
    exit 1
fi
: "${PONS_MUTATION_DISPOSABLE:?Set this only in a disposable checkout}"
: "${PONS_MUTATION_LOG_DIR:?Provide a log directory outside the checkout}"
mkdir -p "$PONS_MUTATION_LOG_DIR"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
mutant() {
    local name="$1" file="$2" expression="$3" package="$4" test="$5"
    git restore --worktree -- "$file"
    sed -i "$expression" "$file"
    if git diff --quiet -- "$file"; then echo "mutation did not apply: $name" >&2; exit 1; fi
    local rc=0
    timeout 90 cargo test --locked -p "$package" "$test" > "$PONS_MUTATION_LOG_DIR/$name.log" 2>&1 || rc=$?
    git restore --worktree -- "$file"
    if [[ "$rc" != 101 ]] || ! grep -q 'test result: FAILED' "$PONS_MUTATION_LOG_DIR/$name.log"; then
        echo "mutant $name was not killed by a test assertion (exit=$rc)" >&2; exit 1
    fi
    echo "$name: killed (test assertion, exit 101)"
}
mutant empty-success crates/pons-cli/src/main.rs 's/report.files_scanned == 0/false/' pons-cli no_input_is_not_success_even_without_strict_flag
mutant comment-is-statement crates/pons-rules/src/t0/unreachable_after_jump.rs 's/while next.is_some_and(|n| n.kind().contains("comment"))/while false/' pons-rules every_rule_is_proven_by_its_own_fixture_corpus
mutant html-clears-all-contexts crates/pons-core/src/php.rs 's/^                    HTML,$/                    ALL,/' pons-rules every_rule_is_proven_by_its_own_fixture_corpus
mutant lost-branch-facts crates/pons-core/src/php.rs 's/|= bits/\&= bits/' pons-core branch_join_early_exit_and_loop_fixed_point
mutant non-durable-commit crates/pons-protocols/src/wharf.rs 's/|| !durable/|| false/' pons-protocols denied_transition_matrix
