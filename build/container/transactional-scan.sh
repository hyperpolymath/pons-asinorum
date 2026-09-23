#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
if [ "$#" -ne 2 ]; then
  echo "Usage: $0 INPUT_DIRECTORY REPORT.json" >&2
  exit 2
fi
for tool in podman firewall-cmd getenforce jq realpath mktemp; do
  command -v "$tool" >/dev/null || { echo "Required deployment tool missing: $tool" >&2; exit 2; }
done
firewall-cmd --state >/dev/null
test "$(getenforce)" = Enforcing || { echo 'Deployment requires enforcing SELinux; no host setting was changed.' >&2; exit 2; }
input=$(realpath -- "$1")
test -d "$input" || { echo 'Input must be a directory' >&2; exit 2; }
report=$(realpath -m -- "$2")
test -d "$(dirname "$report")" || { echo 'Report parent must already exist' >&2; exit 2; }
temporary=$(mktemp "${report}.XXXXXX")
trap 'rm -f -- "$temporary"' EXIT
podman run --rm --network=none --read-only --cap-drop=all \
  --security-opt=no-new-privileges --pids-limit=64 --memory=512m --cpus=2 \
  --volume "$input:/input:ro" "${PONS_IMAGE:-localhost/pons:dev}" \
  scan /input --format json > "$temporary"
jq -e '.tool.name == "pons" and .scanned.complete == true and (.findings | type == "array")' "$temporary" >/dev/null
mv -f -- "$temporary" "$report"
trap - EXIT
