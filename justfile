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
