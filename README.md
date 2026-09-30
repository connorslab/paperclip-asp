# Paperclip ASP

Private experimental XBT Ark Service Provider based on Bark by Second and the Bark
contributors. This repository contains the server and its shared dependencies.
The wallet and web interface are in `connorslab/paperclip-wallet`.

Funded Lightning is available with explicit private-test opt-in; see [current implementation and activation requirements](LIGHTNING.md).

**Experimental. Regtest by default; mainnet requires explicit opt-in.** See
`MAINNET.md`. Mainnet transaction validation is not yet complete.

## Build

```sh
nix develop --command bash scripts/build.sh
./target/debug/paperclip-asp --help
```

Use the private test setup with the matching wallet revision. The application
uses funded recovery profile 2 with default Knots relay policy and
`acceptnonstdtxn=0`. Unified sighash protection and the explicit network opt-in guard
remain enabled. Ordinary public-key boards, rounds, transfers and exits carry
explicit recovery reserves. See `FUNDED-EXITS.md` for accounting and limits.

## Test the pair

```sh
export PAPERCLIP_WALLET_BIN=/absolute/path/to/paperclip-wallet
export PAPERCLIP_ASP_BIN=/absolute/path/to/paperclip-asp
export PAPERCLIP_WATCHMAN_BIN=/absolute/path/to/paperclip-watchman
export XBT_BITCOIND=/absolute/path/to/verified/knots/bin/bitcoind
nix develop --command bash scripts/test-pair.sh
```

This runs the actual deposit, refresh, transfer, cooperative withdrawal and
server-offline unilateral-exit test. It uses private regtest data, loopback-only
ASP endpoints, the pinned source harness, and no production RPC or wallets.
The fixture creates and cleans up its own daemons. Do not use existing wallet or
chain directories for the fixture. Keep its output private: it contains test data.

`scripts/lab.py` creates a separate profile-2 interactive lab and leaves the old
lab state intact. It is not the complete validation runner; use
`scripts/test-pair.sh` for this profile. Existing lab wallets need a planned
cooperative refresh, not an assumption that old signatures gained reserves.

## Operations and limits

Use a dedicated PostgreSQL database and separate Knots regtest process. Bind
public/admin/integration test RPC endpoints to loopback. Keep any persistent test
state outside Git, with owner-only access. Do not connect a production Lightning
node. Existing Paperclip pool and Lightning services must remain separate.

Comprehensive adversarial testing, production monitoring and independent review
remain prerequisites for production deployment. Lightning and liquidity-pool
allocation require explicit `experimental_funded_lightning` opt-in. See `VALIDATION.md` for verified coverage.
See `PROTOCOL.json`, `UPSTREAM.md` and the original MIT `LICENSE`.

## Deployment and current validation

[Deployment instructions](deployment/README.md) include private configuration, systemd, TLS, and backup requirements.

Profile 2 removes the TRUC/version-3 dependency. See `PROTOCOL.json` and `VALIDATION.md` for the tested scope; mainnet opt-in remains experimental.
