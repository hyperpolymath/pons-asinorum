#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Exercise the production build script in a minimal isolated Cargo repository.
set -euo pipefail
project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
probe_dir="$(mktemp -d)"
trap 'rm -rf "$probe_dir"' EXIT
mkdir -p "$probe_dir/crates/pons-core/src" "$probe_dir/schemas"
cp "$project_dir/crates/pons-core/build.rs" "$probe_dir/crates/pons-core/build.rs"
cat > "$probe_dir/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/pons-core"]
resolver = "2"
EOF
cat > "$probe_dir/crates/pons-core/Cargo.toml" <<'EOF'
[package]
name = "provenance-probe"
version = "0.1.0"
edition = "2021"
EOF
cat > "$probe_dir/crates/pons-core/src/main.rs" <<'EOF'
const SCHEMA: &str = include_str!("../../../schemas/probe.json");
fn main() { println!("{} {}", env!("PONS_SOURCE_DIRTY"), SCHEMA.len()); }
EOF
printf '{}\n' > "$probe_dir/schemas/probe.json"
printf 'target/\n' > "$probe_dir/.gitignore"
cd "$probe_dir"
export CARGO_TARGET_DIR="$probe_dir/target"
cargo generate-lockfile --offline --quiet
git init -q
git add .
git -c user.name=Fixture -c user.email=fixture@example.invalid -c commit.gpgsign=false commit -qm fixture
[[ "$(cargo run --offline --quiet)" == 'false 3' ]]
printf '{ }\n' > schemas/probe.json
[[ "$(cargo run --offline --quiet)" == 'true 4' ]]
echo "build provenance: embedded schema edit invalidates clean producer status"
