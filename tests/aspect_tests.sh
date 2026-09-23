#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
cd "$(dirname "$0")/.."
for file in Cargo.toml Cargo.lock Justfile .machine_readable/rsr-profile.a2ml .github/SECURITY.md .github/CONTRIBUTING.md .github/CODE_OF_CONDUCT.md; do
  test -s "$file" || { echo "Required file missing or empty: $file" >&2; exit 1; }
done
count=0
while IFS= read -r -d '' file; do
  count=$((count + 1))
  if ! head -n 5 "$file" | grep -Fq 'SPDX-License-Identifier: MPL-2.0'; then
    echo "Missing MPL-2.0 source header: $file" >&2
    exit 1
  fi
done < <(find src crates -type f -name '*.rs' -print0)
test "$count" -gt 0 || { echo 'No Rust source inspected' >&2; exit 1; }
if grep -n '6a2/' README.adoc docs/PLAN.adoc .machine_readable/descriptiles/*.a2ml; then
  echo 'Retired descriptive-anchor path is still live' >&2
  exit 1
fi
printf 'Repository aspects passed; %s Rust source files inspected.\n' "$count"
