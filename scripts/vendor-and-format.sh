#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Bootstrap helper used only on GitHub Actions runners. It vendors the locked
# dependencies once, records the source replacement, then delegates to the real
# rustfmt so `cargo fmt` behaves normally.
set -euo pipefail

if [[ "${CI:-}" == "true" && ! -d vendor ]]; then
    cargo vendor --versioned-dirs vendor > /tmp/pons-cargo-vendor.log 2>&1
    python3 - <<'PY'
from pathlib import Path
config = Path(".cargo/config.toml")
text = config.read_text()
if "replace-with" not in text:
    text += """
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
"""
    config.write_text(text)
print("vendored locked dependencies for offline verification")
PY
    git add -f vendor .cargo/config.toml
fi

exec rustfmt "$@"
