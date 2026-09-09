# SPDX-License-Identifier: MPL-2.0
set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

build:
    cargo build --workspace --locked

test:
    cargo test --workspace --locked

falsify:
    cargo test -p pons-rules --test falsifier --locked

lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings

catalogue:
    cargo run --quiet --locked -p pons-cli -- catalogue > docs/catalogue.adoc

man:
    mkdir -p docs/man
    cargo run --quiet --locked -p pons-cli -- man > docs/man/pons.1

docs:
    mkdir -p target/docs
    asciidoctor --safe-mode safe -a reproducible -D target/docs README.adoc docs/USAGE.adoc docs/SCOPE.adoc docs/catalogue.adoc

e2e:
    bash tests/e2e.sh

aspect:
    bash tests/aspect_tests.sh

bench:
    bash benches/pons_bench.sh

check: lint test aspect

container-build:
    podman build -f build/container/Containerfile -t localhost/pons:dev .

container-smoke:
    podman run --rm --network=none --read-only --cap-drop=all --security-opt=no-new-privileges localhost/pons:dev --version

rsr-profile standards:
    bash {{quote(standards)}}/scripts/check-rsr-profile.sh .
