#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# verify-upstream.sh — prove the vendored benchmark harness still matches
# upstream.
#
# `scripts/proven-bench/` holds three files copied byte-for-byte from
# hyperpolymath/proven-tests-and-benches. A vendored copy is a promise that
# the copy says what upstream says; this script checks it, by fetching the
# same revision from upstream and comparing digests.
#
# It is a network check, so it runs in CI (where the network is available)
# rather than in the offline test suite. It is deliberately NOT part of the
# provenance gate: a vendored harness that cannot reach the network is still
# a usable harness, and a gate that cannot look must not report a pass — so
# this script exits 2 (no check performed) when it cannot fetch, and the
# caller decides whether that is acceptable.
#
# EXIT CODES
#   0  every vendored file matches upstream at the pinned revision
#   1  a vendored file differs from upstream
#   2  the check could not be performed (no network, upstream missing the
#      file, upstream unreachable)
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/proven-bench/PIN
. "$SCRIPT_DIR/PIN"

FILES=(
  "scripts/check-bench-regression.sh"
  "scripts/compute-bench-baseline.sh"
  "scripts/bench-stats.jq"
)

command -v curl >/dev/null 2>&1 || {
  echo "verify-upstream: curl is not on PATH" >&2
  exit 2
}
command -v sha256sum >/dev/null 2>&1 || {
  echo "verify-upstream: sha256sum is not on PATH" >&2
  exit 2
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fail=0
for path in "${FILES[@]}"; do
  local_name="$(basename "$path")"
  url="https://raw.githubusercontent.com/${UPSTREAM_REPO}/${UPSTREAM_REV}/${path}"
  if ! curl --proto '=https' --proto-redir '=https' --fail --silent --show-error --location \
        "$url" --output "$tmp/$local_name"; then
    echo "verify-upstream: could not fetch $url" >&2
    echo "verify-upstream: NO CHECK WAS PERFORMED - a skip is not a pass." >&2
    exit 2
  fi
  upstream_digest="$(sha256sum "$tmp/$local_name" | cut -d' ' -f1)"
  vendored_digest="$(sha256sum "$SCRIPT_DIR/$local_name" | cut -d' ' -f1)"
  if [ "$upstream_digest" != "$vendored_digest" ]; then
    echo "::error::verify-upstream: $local_name differs from $UPSTREAM_REPO@$UPSTREAM_REV" >&2
    echo "::error::verify-upstream:   upstream $upstream_digest" >&2
    echo "::error::verify-upstream:   vendored $vendored_digest" >&2
    fail=1
  else
    echo "  $local_name: matches upstream ($upstream_digest)"
  fi
done

if [ "$fail" -ne 0 ]; then
  echo "verify-upstream: FAILED" >&2
  exit 1
fi
echo "verify-upstream: OK - the vendored harness matches $UPSTREAM_REPO@$UPSTREAM_REV"
exit 0
