# Private Umbrel installation — 2026-09-30

Installed on umbrel-knots.local as `blake2b-paperclip-asp` (port 38181),
alongside `blake2b-paperclip-wallet` (port 38180). Images are built locally and
served by a registry bound only to 127.0.0.1:5010. This is a private installation,
not a published app-store release.

The selected backend is the existing Retropex Knots app over its private Docker
RPC endpoint, port 9332. Mainnet and synchronized txindex were verified. No node
configuration changes or transfers were made. ASP and wallet keys were created
independently; wallet balances are zero.

## Installation details

Use package-umbrel.py with immutable ASP and PostgreSQL image references.
PostgreSQL PGDATA must be a subdirectory of its volume: Umbrel can place hidden
files in the volume root. Provision private configuration with provision-umbrel.py
before installing through Umbrel. ASP configuration must require board funding
transactions on mainnet.

After first ASP initialization, obtain its rounds-wallet address using
`paperclip-asp --config /config/asp.json rpc wallet`. Set watchman.json's
`sweep_address` to that operator-owned address, then restart the operator app.
For separately generated watchman configuration, use prepare-config.py's
`--watchman --sweep-address ADDRESS`. The watchman shares the ASP mnemonic and
database. Never generate a replacement mnemonic for it.

The private operator API reports both child-process states and admin-RPC
readiness. Verify all three after initialization and restart; a running operator
container alone does not prove the ASP or watchman is healthy.

## Remaining test gates

Lightning is deliberately disabled. Funded-profile HTLC builders, reserve
accounting, pool issuance, claim/refund handling, and watchman recovery must be
integrated and tested before enabling it. See ../LIGHTNING.md. Do not remove
the configuration gate merely to connect a CLN backend.

Funded Ark lifecycle, recovery, backup restore, and Lightning payment tests have
not been completed on this Umbrel installation. Do not fund these test apps yet.
StartOS installation remains unverified.
