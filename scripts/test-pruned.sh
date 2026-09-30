#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
: "${PAPERCLIP_CLN_EXEC:?Set the isolated XBT CLN binary or wrapper}"
: "${PAPERCLIP_CLN_PLUGIN_DIR:?Set the XBT hold plugin directory}"
export LIGHTNINGD_EXEC="$PAPERCLIP_CLN_EXEC" LIGHTNINGD_PLUGIN_DIR="$PAPERCLIP_CLN_PLUGIN_DIR"
unset LIGHTNINGD_DOCKER_IMAGE
export PAPERCLIP_PRUNED_RPC=1
export PAPERCLIP_PRUNED_ADAPTER="$root/deployment/pruned_rpc.py"
export PAPERCLIP_TEST_PRUNED=1
export PAPERCLIP_TEST_SOURCE="$root/tests/pruned-lifecycle.rs"
export PAPERCLIP_TEST_FILTER=xbt_pruned_lifecycle
exec bash "$root/scripts/test-default-policy.sh"
