#!/usr/bin/env bash
set -euo pipefail
umask 077
: "${IN_NIX_SHELL:?Use nix develop --command bash scripts/test-pair.sh}"
: "${PAPERCLIP_WALLET_BIN:?Set an absolute path to paperclip-wallet}"
: "${PAPERCLIP_ASP_BIN:?Set an absolute path to paperclip-asp}"
: "${PAPERCLIP_WATCHMAN_BIN:?Set an absolute path to paperclip-watchman}"
: "${XBT_BITCOIND:?Set the verified Knots test binary}"
expected=d04cd8211e711af989a7a62d0b8b55a8cfe496694392518da0ccb848469b3799
actual=$(sha256sum "$XBT_BITCOIND" | cut -d' ' -f1)
[[ "$actual" == "$expected" ]] || { echo 'Unexpected Knots binary hash' >&2; exit 1; }
root=$(cd "$(dirname "$0")/.." && pwd)
export PAPERCLIP_SHARED_SOURCE="$root"
export PAPERCLIP_WALLET_SOURCE="${PAPERCLIP_WALLET_SOURCE:-$root/../paperclip-wallet}"
test -f "$PAPERCLIP_WALLET_SOURCE/bark/src/lib.rs"
cmp "$root/SHARED-SOURCE.sha256" "$PAPERCLIP_WALLET_SOURCE/SHARED-SOURCE.sha256"
(cd "$root" && sha256sum --quiet -c SHARED-SOURCE.sha256)
(cd "$PAPERCLIP_WALLET_SOURCE" && sha256sum --quiet -c SHARED-SOURCE.sha256)
export PAPERCLIP_TEST_SOURCE="${PAPERCLIP_TEST_SOURCE:-$root/tests/default-policy-lifecycle.rs}"
mkdir -p "$root/.state"
run=$(mktemp -d "$root/.state/default-policy.XXXXXX")
git clone --quiet https://github.com/connorslab/bark-xbt "$run/harness"
cd "$run/harness"
git checkout --quiet e107d09e7fd173fc5895a2b9eec500082f48d4cd
# The upstream integration fixture listens on all interfaces. Isolate this lab.
python3 - <<'PY'
from pathlib import Path
import shutil, os
# The harness must decode the same versioned protocol as the two binaries.
for folder in ['lib', 'bitcoin-ext', 'server-rpc', 'cln-rpc']:
 shutil.copytree(Path(os.environ['PAPERCLIP_SHARED_SOURCE']) / folder, folder, dirs_exist_ok=True)
shutil.copytree(Path(os.environ['PAPERCLIP_SHARED_SOURCE']) / 'server/src', 'server/src', dirs_exist_ok=True)
for folder in ['bark/src', 'bark-json/src']:
 shutil.copytree(Path(os.environ['PAPERCLIP_WALLET_SOURCE']) / folder, folder, dirs_exist_ok=True)
# Keep the wallet's direct dependencies aligned with its copied source.
shutil.copyfile(Path(os.environ['PAPERCLIP_WALLET_SOURCE']) / 'bark/Cargo.toml', 'bark/Cargo.toml')
p=Path('testing/Cargo.toml')
p.write_text(p.read_text().replace('[dependencies]', '[dependencies]\nasync-stream.workspace = true', 1))
p=Path('testing/src/daemon/barkd.rs')
p.write_text(p.read_text().replace('SendRequest {', 'SendRequest { max_total_sat: None,'))
p=Path('testing/src/context/mod.rs')
t=p.read_text(); t=t.replace('\n\t\t\tcln_array,', '\n\t\t\texperimental_funded_lightning: false,\n\t\t\texperimental_bolt12_receive: false,\n\t\t\tcln_array,', 1)
p.write_text(t)
p=Path('testing/src/daemon/captaind/proxy.rs')
t=p.read_text()
marker='impl<T: ArkRpcProxy> rpc::server::ArkService for ArkRpcProxyWrapper<T> {'
methods='\n\tasync fn get_lightning_offer_info(&self, req: tonic::Request<protos::Empty>) -> Result<tonic::Response<protos::LightningOfferInfo>, tonic::Status> {\n\t\tself.upstream_client().get_lightning_offer_info(req).await\n\t}\n\tasync fn register_bolt12_receive(&self, req: tonic::Request<protos::RegisterBolt12ReceiveRequest>) -> Result<tonic::Response<protos::Empty>, tonic::Status> {\n\t\tself.upstream_client().register_bolt12_receive(req).await\n\t}\n\ttype ServeLightningOffersStream = tonic::Streaming<protos::LightningOfferServer>;\n\tasync fn serve_lightning_offers(&self, req: tonic::Request<tonic::Streaming<protos::LightningOfferClient>>) -> Result<tonic::Response<Self::ServeLightningOffersStream>, tonic::Status> {\n\t\tlet mut input = req.into_inner();\n\t\tlet stream = async_stream::stream! { while let Ok(Some(message)) = input.message().await { yield message; } };\n\t\tself.upstream_client().serve_lightning_offers(stream).await\n\t}\n'
assert marker in t
p.write_text(t.replace(marker, marker+methods))
p=Path('testing/src/daemon/lightningd.rs')
p.write_text(p.read_text().replace('addr=0.0.0.0:', 'addr=127.0.0.1:'))
p=Path('testing/src/daemon/captaind/mod.rs')
t=p.read_text(); old='format!("0.0.0.0:{}", public_port)'
assert old in t
p.write_text(t.replace(old, 'format!("127.0.0.1:{}", public_port)'))
p=Path('testing/src/bark.rs')
t=p.read_text(); assert '.strip_prefix("bark ")' in t
p.write_text(t.replace('.strip_prefix("bark ")', '.strip_prefix("paperclip-wallet ")').replace('match cp {', 'match cp { Cp::LightningOffer(_) => "ln_offer.saved".to_string(),'))
p=Path('testing/xbt-bitcoind')
t=p.read_text(); assert '-mempooltruc=enforce -subdustfeepenalty=0' in t
p.write_text(t.replace('-mempooltruc=enforce -subdustfeepenalty=0', '-mempooltruc=reject'))
if os.environ.get('PAPERCLIP_TEST_PRUNED') == '1':
 t=p.read_text(); assert '-connect=0' in t
 p.write_text(t.replace('-connect=0', '-connect=0 -txindex=0 -prune=1 -fastprune=1'))
shutil.copyfile(os.environ['PAPERCLIP_TEST_SOURCE'], 'testing/tests/bark/xbt.rs')
for name in ['justfile','testing/xbt-bitcoind']:
 p=Path(name); p.write_text(p.read_text().replace('#!/usr/bin/env bash', '#!'+shutil.which('bash')))
PY
export CAPTAIND_EXEC="$PAPERCLIP_ASP_BIN" BARK_EXEC="$PAPERCLIP_WALLET_BIN"
export WATCHMAND_EXEC="$PAPERCLIP_WATCHMAN_BIN"
export BITCOIND_EXEC="$PWD/testing/xbt-bitcoind" ASSUME_BUILT=1 KEEP_ALL_TEST_DATA=1
export TEST_DIRECTORY="$run/data" CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
set -o pipefail
just int "--test bark ${PAPERCLIP_TEST_FILTER:-xbt_lifecycle} --test-threads 1" 2>&1 | tee "$run/lifecycle.log"
echo "Evidence: $run"
