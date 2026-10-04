#!/usr/bin/env bash
# Disposable, loopback-only regtest. Never load an operator configuration.
set -euo pipefail
umask 077
: "${IN_NIX_SHELL:?Use the pinned build shell}"
root=$(cd "$(dirname "$0")/.." && pwd)
deps="$root/.state/ln-failure-deps"
mkdir -p "$deps/plugins"
export CARGO_TARGET_DIR="$root/target"
export PAPERCLIP_TEST_NIX_LIBS="${LD_LIBRARY_PATH:-}"

curl -fL --retry 3 -o "$deps/knots.tar.gz" https://github.com/bitcoinknots/bitcoin/releases/download/v29.4.2.knots20260508/bitcoin-29.4.2.knots20260508-x86_64-linux-gnu.tar.gz
curl -fL --retry 3 -o "$deps/cln.tar.xz" https://github.com/privkeyio/lightning/releases/download/v26.06.8-blake2b.5/clightning-v26.06.8-blake2b.5-Ubuntu-24.04-amd64.tar.xz
echo "b59d0445a317e21a03dc29425db3aba79b27d5125230b1a2b1dce62e120827c5  $deps/knots.tar.gz" | sha256sum -c -
echo "1b4792375bf9a24a15d6878d831e9ab0fb568614ebdcf4eff1ee5e6d34b4a70a  $deps/cln.tar.xz" | sha256sum -c -
tar -xf "$deps/knots.tar.gz" -C "$deps"
mkdir -p "$deps/cln"
tar -xf "$deps/cln.tar.xz" -C "$deps/cln"
export XBT_BITCOIND="$deps/bitcoin-29.4.2.knots20260508/bin/bitcoind"
echo "d04cd8211e711af989a7a62d0b8b55a8cfe496694392518da0ccb848469b3799  $XBT_BITCOIND" | sha256sum -c -

git clone --quiet https://github.com/BoltzExchange/hold.git "$deps/hold"
git -C "$deps/hold" checkout --quiet 14c3568d2b9be7af23df69a4dc579dd198428f1d
git -C "$deps/hold" apply "$root/tests/fixtures/hold-xbt.patch"
printf '\n[workspace]\n' >> "$deps/hold/Cargo.toml"
mkdir -p "$deps/hold/vendor"
cp -r "$root/vendor/lightning-types" "$deps/hold/vendor/"
(cd "$deps/hold" && cargo build --locked --no-default-features)

# CLN's release uses Ubuntu libraries; the locally built hold plugin uses Nix.
cat > "$deps/lightningd" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
base=$(cd "$(dirname "$0")" && pwd)
export PATH="$base/bitcoin-29.4.2.knots20260508/bin:$PATH"
unset LD_LIBRARY_PATH
exec "$base/cln/usr/bin/lightningd" "$@"
SH
cat > "$deps/plugins/hold" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
export LD_LIBRARY_PATH="$PAPERCLIP_TEST_NIX_LIBS"
exec "$CARGO_TARGET_DIR/debug/hold" "$@"
SH
# The release auto-loads cln-grpc. Registering a symlink again is an error.
test -x "$deps/cln/usr/libexec/c-lightning/plugins/cln-grpc"
chmod 700 "$deps/lightningd" "$deps/plugins/hold"
export PAPERCLIP_CLN_EXEC="$deps/lightningd"
export PAPERCLIP_CLN_PLUGIN_DIR="$deps/plugins"

git clone --quiet https://github.com/connorslab/paperclip-wallet-app.git "$deps/wallet"
git -C "$deps/wallet" checkout --quiet d1e74d7fd1ff4b77fceefe9791ce86700af1b5e1
export PAPERCLIP_WALLET_SOURCE="$deps/wallet"
(cd "$deps/wallet" && bash scripts/build.sh)
(cd "$root" && bash scripts/build.sh)
export PAPERCLIP_WALLET_BIN="$CARGO_TARGET_DIR/debug/paperclip-wallet"
export PAPERCLIP_ASP_BIN="$CARGO_TARGET_DIR/debug/paperclip-asp"
export PAPERCLIP_WATCHMAN_BIN="$CARGO_TARGET_DIR/debug/paperclip-watchman"
export POSTGRES_BINS
POSTGRES_BINS=$(dirname "$(command -v postgres)")
bash "$root/scripts/test-lightning.sh"
