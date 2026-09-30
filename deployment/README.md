# Deploying the XBT ASP

For container and home-server work, see [platform preparation](PLATFORMS.md).
It supports explicit private RPC endpoints without a vendor whitelist; the
native Umbrel/StartOS operator packages are still in progress.

These files prepare a Linux deployment. They do not install or activate anything
automatically. Use a separate ASP host to isolate its keys, database, and failure
domain from Paperclip's pool and Lightning services. A synchronized XBT Knots
backend may run there or be reached through a private loopback tunnel.

## Prerequisites

- A current XBT Knots backend with activated Blake2b headers, synchronized
  `txindex=1`, and normal relay rules. Do not enable policy exemptions.
- Linux with system-wide Nix, PostgreSQL, Caddy, and a dedicated `paperclip-asp`
  service account. Only SSH and HTTPS need public ingress.
- This source at `/opt/paperclip-asp`, built with `nix develop --command bash
  scripts/build.sh`. Run the paired strict-policy tests before deploying a revision.
- A PostgreSQL role `paperclip-asp` owning database `paperclip_asp`, authenticated
  over the local Unix socket with peer authentication. Do not expose PostgreSQL.
- `/var/lib/paperclip-asp` owned by the service account, mode 0700, and
  `/etc/paperclip-asp` mode 0750. The account must be able to read the RPC cookie.

Generate `/etc/paperclip-asp/config.json` using `prepare-config.py`, choosing the
network explicitly. Create `/etc/paperclip-asp/environment` (0600); for the
experimental mainnet deployment it contains `PAPERCLIP_XBT_MAINNET=1`. For regtest,
leave it empty. Set `HOME=/var/lib/paperclip-asp` there for the Nix cache.
Run Nix once as this account before enabling the hardened service.

Initialize exactly once as the service account:

```sh
nix develop /opt/paperclip-asp --command /opt/paperclip-asp/target/debug/paperclip-asp \
  --config /etc/paperclip-asp/config.json create
```

Install `paperclip-asp.service`, mark `run-asp.sh` executable, and substitute your
hostname in the Caddyfile. Start the ASP privately first. Query its local admin
API with `paperclip-asp rpc --addr 127.0.0.1:3536 wallet` to obtain the server's
actual funding address. Never reuse a test or another node's funding address.
Back up the newly generated key material before any funding. Enable the HTTPS
proxy only after the private funded lifecycle and recovery checks pass.

The **watchman is required** for double-spend protection and reclaiming funds.
Generate `/etc/paperclip-asp/watchman.json` with the same configuration generator
and `--watchman`. It shares the ASP's existing mnemonic directory and database;
do not initialize it with a different seed. Install `paperclip-watchman.service`
and make its wrapper executable. Fund the watchman fee wallet as well as the
rounds wallet, using addresses reported by the local admin API. Verify both
services remain healthy and the watchman follows the chain before opening access.

## Operations and recovery

Back up both the entire private ASP data directory and PostgreSQL using encrypted
off-host storage. A seed alone does not preserve signed round and recovery state.
Quiesce the service for a coherent snapshot and test restoring to an isolated host.
Retain the old binary, config, and backups before upgrades. Never edit stored
signed transactions to upgrade their format. Old positions must refresh cooperatively.

Monitor process restarts, Knots synchronization and index progress, server wallet
balances, round failures, pending transactions, database disk space, and exit
deadlines. Keep the admin port loopback-only; it is not a public health endpoint.
The server subsidizes round recovery reserves and needs its own confirmed liquidity.

Profile 2 supports ordinary Ark balances, boarding, transfers, refreshes,
cooperative withdrawals, and unilateral exits. Lightning contracts and the VTXO
pool remain disabled. Fixed reserves are not a guarantee at arbitrarily high fees.
Mainnet opt-in is experimental; a regtest pass does not establish production safety.
