#!/usr/bin/env bash
set -euo pipefail
umask 077
# Install Nix system-wide and realize the flake as the service user beforehand.
export PATH="/nix/var/nix/profiles/default/bin:$PATH"
exec nix develop /opt/paperclip-asp --command /opt/paperclip-asp/target/debug/paperclip-asp \
  --config /etc/paperclip-asp/config.json start
