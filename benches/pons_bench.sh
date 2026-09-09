#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --locked -p pons-cli
# This reports observed wall/user/system time. It asserts no speedup claim.
time target/release/pons scan fixtures --format json > /dev/null
