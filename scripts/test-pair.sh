#!/usr/bin/env bash
set -euo pipefail
umask 077
: "${IN_NIX_SHELL:?Use nix develop --command bash scripts/test-pair.sh}"
: "${PAPERCLIP_WALLET_BIN:?Set an absolute path to paperclip-wallet}"
: "${PAPERCLIP_ASP_BIN:?Set an absolute path to paperclip-asp}"
: "${XBT_BITCOIND:?Set the verified Knots test binary}"
expected=d04cd8211e711af989a7a62d0b8b55a8cfe496694392518da0ccb8488469b3799
actual=$(sha256sum "$XBT_BITCOIND" | cut -d' ' -f1)
[[ "$actual" == "$expected" ]] || { echo 'Unexpected Knots binary hash' >&2; exit 1; }
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$root/.state"
run=$(mktemp -d "$root/.state/e2e.XXXXXX")
git clone --quiet https://github.com/connorslab/bark-xbt "$run/harness"
cd "$run/harness"
git checkout --quiet e107d09e7fd173fc5895a2b9eec500082f48d4cd
# The upstream integration fixture listens on all interfaces. Isolate this lab.
python3 - <<'PY'
from pathlib import Path
import shutil
p=Path('testing/src/daemon/captaind/mod.rs')
t=p.read_text(); old='format!("0.0.0.0:{}", public_port)'
assert old in t
p.write_text(t.replace(old, 'format!("127.0.0.1:{}", public_port)'))
for name in ['justfile','testing/xbt-bitcoind']:
 p=Path(name); p.write_text(p.read_text().replace('#!/usr/bin/env bash', '#!'+shutil.which('bash')))
PY
export CAPTAIND_EXEC="$PAPERCLIP_ASP_BIN" BARK_EXEC="$PAPERCLIP_WALLET_BIN"
export BITCOIND_EXEC="$PWD/testing/xbt-bitcoind" ASSUME_BUILT=1 KEEP_ALL_TEST_DATA=1
export TEST_DIRECTORY="$run/data" CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
set -o pipefail
just int '--test bark xbt_lifecycle --test-threads 1' 2>&1 | tee "$run/lifecycle.log"
echo "Evidence: $run"
