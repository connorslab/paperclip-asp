#!/usr/bin/env bash
set -euo pipefail
umask 077
export PATH="/nix/var/nix/profiles/default/bin:$PATH"
exec nix develop /opt/paperclip-asp --command /opt/paperclip-asp/target/debug/paperclip-watchman \
  --config /etc/paperclip-asp/watchman.json start
