# Native development and tests

Use Linux (Ubuntu 24.04 CI or WSL2) with Rust 1.90.0, as pinned in
`rust-toolchain.toml`. Nix is optional. Cargo dependencies remain locked.

Install native packages:

```sh
sudo apt-get update
sudo apt-get install -y build-essential git curl python3 clang libclang-dev cmake pkg-config protobuf-compiler libssl-dev libsqlite3-dev libpq-dev
rustup toolchain install 1.90.0 --profile minimal
cargo install just --version 1.40.0 --locked
bash scripts/build.sh
just checks
just unit
just unit-server
python3 deployment/test_config.py
python3 deployment/test_operator.py
python3 deployment/test_provision.py
```

Install rustup through its official installer if it is not present. GitHub CI
already provides rustup. The workflow `build.yml` runs these native build and
unit checks without deployment credentials. Dispatch it on the experimental
branch. The existing Debian Dockerfile also builds without Nix.

These are repeatable source checks, not a claim of byte-identical binaries:
the Linux package repositories and runner image are not snapshot-pinned.

For end-to-end tests, use the `just int` and `just int-lightning` wrappers and
supply the matching wallet binaries and verified Knots/CLN binaries documented
in README.md and the scripts. Preserve hash checks, loopback bindings and
isolated regtest state. Native unit CI does not establish Lightning settlement
or gossip v2 interoperability. Never use production data or customer invoices.
