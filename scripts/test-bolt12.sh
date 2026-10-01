#!/usr/bin/env bash
set -euo pipefail
: "${PAPERCLIP_CLN_EXEC:?Set the isolated XBT CLN binary}"
: "${PAPERCLIP_CLN_PLUGIN_DIR:?Set the XBT hold plugin directory}"
: "${LEGACY_BARK_EXEC:?Set the verified released wallet binary for compatibility testing}"
export LIGHTNINGD_EXEC="$PAPERCLIP_CLN_EXEC" LIGHTNINGD_PLUGIN_DIR="$PAPERCLIP_CLN_PLUGIN_DIR"
unset LIGHTNINGD_DOCKER_IMAGE
root=$(cd "$(dirname "$0")/.." && pwd)
export PAPERCLIP_TEST_SOURCE="$root/tests/bolt12-receive.rs"
export PAPERCLIP_TEST_FILTER=xbt_reusable_bolt12_receive
exec bash "$root/scripts/test-default-policy.sh"
