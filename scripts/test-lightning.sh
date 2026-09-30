#!/usr/bin/env bash
set -euo pipefail
: "${PAPERCLIP_CLN_EXEC:?Set the isolated verified XBT CLN binary or wrapper}"
: "${PAPERCLIP_CLN_PLUGIN_DIR:?Set the directory containing the XBT hold plugin}"
export LIGHTNINGD_EXEC="$PAPERCLIP_CLN_EXEC" LIGHTNINGD_PLUGIN_DIR="$PAPERCLIP_CLN_PLUGIN_DIR"
unset LIGHTNINGD_DOCKER_IMAGE
root=$(cd "$(dirname "$0")/.." && pwd)
export PAPERCLIP_TEST_SOURCE="$root/tests/funded-lightning.rs"
export PAPERCLIP_TEST_FILTER=xbt_funded_lightning
exec bash "$root/scripts/test-default-policy.sh"
