set shell := ["bash", "-euo", "pipefail", "-c"]

checks:
	cargo check --locked --workspace --tests --examples

unit filter="":
	cargo test --locked -p ark-lib -p bark-bitcoin-ext --lib {{filter}}

unit-server filter="":
	cargo test --locked -p bark-server --lib {{filter}}

int:
	bash scripts/test-pair.sh

int-lightning:
	bash scripts/test-lightning.sh

int-pruned:
	python3 deployment/test_pruned_rpc.py
	python3 deployment/test_pruned_knots.py

int-pruned-lifecycle:
	bash scripts/test-pruned.sh

int-bolt12:
	bash scripts/test-bolt12.sh

covenant-unit:
	cargo test --locked -p bark-bitcoin-ext --no-default-features --features experimental-covenants --lib covenant

covenant-int:
	cargo build --locked -p bark-bitcoin-ext --no-default-features --features experimental-covenants --examples
	bash experiments/covenants/test.sh

covenant-services:
	cargo build --locked -p bark-server --features experimental-covenants --bin paperclip-asp --bin paperclip-watchman
	PYTHONPATH="$COVENANT_NODE_SOURCE/test/functional" python3 experiments/covenants/test_services.py --descriptors --configfile="$COVENANT_NODE_CONFIG"

covenant-data-policy:
	PYTHONPATH="$COVENANT_NODE_SOURCE/test/functional" python3 experiments/covenants/test_data_policy.py --configfile="$COVENANT_NODE_CONFIG"
