# ASP platform work

Status: source/container preparation, not an installable Umbrel or StartOS ASP.
Do not run the ASP on a live node until the complete protection/recovery suite,
container builds, and platform acceptance tests pass.

The wallet is a separate app. An ASP operator needs PostgreSQL, the ASP daemon,
the mandatory watchman, a synchronized indexed XBT Knots endpoint, private keys,
and confirmed liquidity for both server and watchman fee wallets.

`Dockerfile` builds the ASP and watchman from source for each target architecture.
`compose.yaml` defines the private three-service topology. It publishes no ports
and does not initialize a new mnemonic automatically. Configuration and key
directories must be owned by uid/gid 1000. Use image references with real tested
digests, including PostgreSQL 17. Keep the PostgreSQL secret outside the repository.

Generate both configs with `prepare-config.py --container`, the same
`--data-dir /var/lib/paperclip-asp`, `--postgres-host postgres`,
`--postgres-password-file /absolute/private/secret`, and explicitly selected
`--rpc-url`, `--rpc-user`, and `--rpc-password-file`. Add `--watchman` for its
config. Cookie authentication remains available with `--rpc-cookie`; when used
in containers, mount the same cookie read-only into both services. The default
compose expects user/password RPC authentication and includes no host mounts.
Config output contains credentials: retain mode 0600 and do not commit it.
The admin interface stays on loopback even in container mode.

Initialize the ASP exactly once with its `create` command after PostgreSQL is
ready. Never initialize a different watchman seed: it uses the ASP mnemonic and
database. A restart uses `start` only. Back up the complete key directory and a
coherent PostgreSQL snapshot while both daemons are stopped. Restore both together.
Keep RPC on a private network or encrypted tunnel and expose only a TLS gRPC
proxy after validation. Container DNS endpoints must be reachable from this
compose network; platform adapters will supply their network integration.

Remaining platform work:

- Umbrel: authenticated operator setup/status UI, platform proxy, dependency
  discovery, and coordinated PostgreSQL/key backup and restore hooks.
- StartOS 0.3.5: native service supervision for PostgreSQL + ASP + watchman in
  its package model, operator configuration/actions, health checks, and backup.
- StartOS 0.4: separate SDK wrapper; 0.3.5 packages are not assumed compatible.
- Both: native amd64/arm64 build, install/upgrade/restart/restore tests and
  watchman failure handling before advertising public service readiness.

Backend compatibility is based on XBT behavior and required RPC capabilities,
not the package provider. Retropex's and Paul Lamb's indexed Knots variants and
privkeyio's CLN are targets. Pruned/no-txindex backends remain unsupported.
Existing CLN admission checks require current XBT identity and signature bits;
they do not assert the remaining HTLC flow is ready. Paul Lamb's Lightning Fork
is LND-based and needs a distinct adapter. Lightning remains disabled for now.

Sources: [Retropex Umbrel](https://github.com/Retropex/umbrel-apps/tree/rc4),
[privkeyio CLN](https://github.com/privkeyio/lightning/releases/tag/v26.06.8-blake2b.5),
[Paul Lamb Lightning Fork](https://github.com/paulscode/lightning-fork/tree/blake2b),
[Paul Lamb Knots wrapper](https://github.com/paulscode/knots-blake2b-startos).
