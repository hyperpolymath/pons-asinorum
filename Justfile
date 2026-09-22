# SPDX-License-Identifier: MPL-2.0

build:
    cargo build --workspace

test:
    cargo test --workspace

fmt:
    cargo fmt --check

lint:
    RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings

falsify:
    cargo test -p pons-rules --test falsifier

# End-to-end: builds the binary first, because tests/e2e.sh executes it and
# deliberately FAILS rather than skips when it is absent.
e2e:
    cargo build --release
    bash tests/e2e.sh

# Proves the e2e suite is not vacuous: against a stub binary it must go red.
e2e-mutant:
    #!/usr/bin/env bash
    if PONS_BIN=/bin/true bash tests/e2e.sh >/dev/null 2>&1; then
        echo "e2e suite passed against a STUB binary — it is vacuous"; exit 1
    else
        echo "e2e suite correctly rejects a stub binary"
    fi

check: fmt lint test e2e
