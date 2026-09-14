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

check: fmt lint test
