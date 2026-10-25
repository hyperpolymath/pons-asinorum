#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT
validator="$SCRIPT_DIR/validate_workflows_test.sh"
for source in 'name: Example' '---
{
  name: "Example",
  jobs: {},
}'; do
    printf '%s\n' "$source" > "$fixture_dir/probe.yml"
    bash "$validator" "$fixture_dir" >/dev/null 2>&1
done
for source in 'jobs: {}' 'name: []' 'name: ""' '{name: "unterminated'; do
    printf '%s\n' "$source" > "$fixture_dir/probe.yml"
    if bash "$validator" "$fixture_dir" >/dev/null 2>&1; then
        echo "workflow field check accepted malformed/missing name" >&2
        exit 1
    fi
done
set +e
YQ_BIN="$fixture_dir/missing-yq" bash "$validator" "$fixture_dir" >/dev/null 2>&1
status=$?
set -e
[[ "$status" == 2 ]] || { echo "missing yq did not fail closed" >&2; exit 1; }
echo "workflow regression: block YAML, flow KYAML, invalid fields and missing tool passed"
