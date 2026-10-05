#!/usr/bin/env bash
set -euo pipefail
: "${COVENANT_NODE_SOURCE:?Set the pinned experimental node source checkout}"
: "${COVENANT_NODE_CONFIG:?Set the experimental node build/test/config.ini}"
: "${COVENANT_RESULTS:?Set an evidence path outside the source checkout}"
export PYTHONPATH="$COVENANT_NODE_SOURCE/test/functional${PYTHONPATH:+:$PYTHONPATH}"
export COVENANT_BIN="${COVENANT_BIN:-${CARGO_TARGET_DIR:-$PWD/target}/debug/examples}"
exec python3 experiments/covenants/test_offline_refresh.py --configfile="$COVENANT_NODE_CONFIG" "$@"
