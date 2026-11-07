#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Check that the shipped C parser is derived from the reviewed grammar.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
vendor="$root/vendor/tree-sitter-javascript"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# A regenerated vendored parser is irrelevant if Cargo stopped using it.
cd "$root"
cargo metadata --locked --format-version 1 > "$work/metadata.json"
jq -e --arg manifest "$vendor/Cargo.toml" '
  [.packages[] | select(.name == "tree-sitter-javascript")] |
  length == 1 and .[0].version == "0.25.0" and .[0].source == null and .[0].manifest_path == $manifest
' "$work/metadata.json" > /dev/null
# An independent workspace must also receive the modified parser, without
# repeating a [patch] section. Exercise the public library through that edge.
mkdir -p "$work/consumer/src"
cat > "$work/consumer/Cargo.toml" <<EOF
[package]
name = "pons-downstream-control"
version = "0.0.0"
edition = "2021"
[workspace]
[dependencies]
pons-core = { path = "$root/crates/pons-core" }
EOF
cat > "$work/consumer/src/main.rs" <<'EOF'
use pons_core::{lang::Lang, parse, source::SourceFile};
fn main() {
    let source = SourceFile {
        path: "downstream.jsx".into(), lang: Lang::JavaScript,
        text: "const v = <Icons.default in='SourceGraphic' />;".into(),
        transcoded: false,
    };
    assert!(parse::parse(&source).is_ok());
}
EOF
cargo run --offline --manifest-path "$work/consumer/Cargo.toml" --target-dir "$root/target"
curl --proto '=https' --proto-redir '=https' --fail --silent --show-error --location --retry 3 \
  https://github.com/tree-sitter/tree-sitter/releases/download/v0.27.0/tree-sitter-linux-x64.gz \
  -o "$work/generator.gz"
printf '%s  %s\n' '20a1f39ec1c45f2211492dcb8881c802b643b554bb196869a29ac3778277fa77' "$work/generator.gz" | sha256sum --check --strict
gzip -dc "$work/generator.gz" > "$work/tree-sitter"
chmod +x "$work/tree-sitter"
mkdir "$work/grammar"
cp -R "$vendor/." "$work/grammar/"
(cd "$work/grammar" && "$work/tree-sitter" generate --js-runtime native --abi 15)
# The pinned generator omits a final LF from these two JSON documents.
# The repository's JSON policy requires it; this is part of regeneration.
for file in src/grammar.json src/node-types.json; do
  printf '\n' >> "$work/grammar/$file"
done
for file in src/parser.c src/grammar.json src/node-types.json src/tree_sitter/parser.h src/tree_sitter/alloc.h src/tree_sitter/array.h; do
  cmp "$vendor/$file" "$work/grammar/$file" || {
    echo "grammar regeneration mismatch: $file" >&2
    exit 1
  }
done
echo 'Pinned grammar regeneration matches every shipped generated file.'
