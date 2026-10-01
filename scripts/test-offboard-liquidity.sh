#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
export PAPERCLIP_TEST_SOURCE="$root/tests/offboard-liquidity.rs"
export PAPERCLIP_TEST_FILTER=xbt_offboard_payout_reserve
exec bash "$root/scripts/test-default-policy.sh"
