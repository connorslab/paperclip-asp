#!/usr/bin/env bash
set -euo pipefail
command -v cargo >/dev/null
command -v python3 >/dev/null
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
cargo build --locked -p bark-server --bin paperclip-asp --bin paperclip-watchman
