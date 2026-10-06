# Paperclip offline refresh laboratory

Experimental Bitcoin covenant protocol. Test coins only. This is a separate,
opt-in protocol path, not a migration of existing Paperclip balances. Based on
Bark by Second and contributors, with Paperclip's unified-sighash implementation.

## Implemented

The wallet signs a bounded sequence of single-balance refresh authorizations.
Each authorization specifies the next owner key, amount, expiry, server key and
earliest refund height. Future round funding outpoints need not exist yet.
The server can then fund a new covenant tree while the wallet is offline.

Each tree has a TEMPLATEHASH-enforced unroll branch and a timed server reclaim
branch. Its exit output has two script paths:

- A CSFS authorization by the owner plus a concrete unified signature by the
  server permits a refund transaction.
- After expiry plus the reaction delay, the owner alone can claim the output
  with a unified signature.

The refund consumes both old and new exit outputs. It recreates the new owner's
full authorized allocation and returns the duplicate backing to the server.
This is what prevents a refreshed user claiming both allocations. Later refunds
can bind to this replacement output because TEMPLATEHASH omits input outpoints.
NUMS internal keys prevent bypassing these policies via a known Taproot key.
An additional, domain-separated owner signature authenticates all permit metadata.

This follows the single-input Erk construction described by Steven Roose:
https://delvingbitcoin.org/t/evolving-the-ark-protocol-using-ctv-and-csfs/1602
It substitutes the signet's BIP446 TEMPLATEHASH for CTV. This is an experiment,
not a claim of equivalence or a completed security review of either protocol.

## Public experimental branches

Both repositories use `experiment/covenant-offline-refresh`:

- ASP: https://github.com/connorslab/paperclip-asp/tree/experiment/covenant-offline-refresh
- Wallet: https://github.com/connorslab/paperclip-wallet-app/tree/experiment/covenant-offline-refresh

```sh
git clone --branch experiment/covenant-offline-refresh https://github.com/connorslab/paperclip-asp.git
git clone --branch experiment/covenant-offline-refresh https://github.com/connorslab/paperclip-wallet-app.git
```

Use the laboratory or opt-in service commands below, not normal startup commands.
There is no public covenant enrollment API or automatic signet wallet UI yet.
The reproducible private lifecycle drivers use fresh regtest nodes. Public
signet tests use a separate, dedicated wallet and normal block production.

## Build and run

Run `nix develop` in each repository before building. The feature is off by default.

```sh
# In the ASP repository
cargo build --locked -p bark-server --features experimental-covenants --bin paperclip-asp
./target/debug/paperclip-asp covenant-lab --experimental-signet < test-request.json

# In the wallet repository
cargo build --locked -p bark-cli --features experimental-covenants --bin paperclip-wallet
./target/debug/paperclip-wallet covenant-lab --experimental-signet < test-request.json
```

These commands run before opening normal configuration, wallet databases or
service connections. They process one JSON request from stdin and return JSON.
They never broadcast a transaction, run the normal ASP, or access its funds.
The isolated regtest driver handles funding, mining and broadcast explicitly.

Lightweight builds exercise the identical shared implementation:

```sh
cargo build --locked -p bark-bitcoin-ext --no-default-features \
  --features experimental-covenants --examples
target/debug/examples/covenant-wallet --experimental-signet < test-request.json
target/debug/examples/covenant-asp --experimental-signet < test-request.json
```

Every request includes `profile: "paperclip-signet-offline-refresh-v1"`,
`challenge: "2102396d38e3ff703be31a2d97317835f4e9645b5ae5ea2d2d1ba406afa7ab185b5fac"`
and a `command`. See `test_offline_refresh.py` for the full request flow.
Wallet commands: `pubkey`, `authorize`, `claim`. ASP commands: `verify`, `round`,
`refund`. Keys enter through stdin only; never put them into a command line,
log, public proof bundle or an ASP permit. The test driver creates disposable
keys in an owner-only file. Use an owner-only parent directory as well.

## Tests

The [native ASP/watchman service report](reports/2026-10-05-services/REPORT.md)
records passing private crash/reorg tests and two autonomous public-signet
refreshes, followed by a confirmed wallet exit with both services stopped.
It also includes the data-policy probes and the remaining integration limits.

The [October 5 public signet report](reports/2026-10-05-public-signet/REPORT.md)
records two preauthorized refreshes, the offline command interval, invalid-spend
checks and confirmed user recovery. It includes transaction links and raw
evidence. The result applies to this laboratory, not the normal Ark service.

Build the pinned covenant node from `connorslab/paperclip-xbt-signet`, commit
`2f142fdd75719d23046be5b1577fc9ef280f3dc3`. Do not change an existing node/datadir.
Set `COVENANT_NODE_SOURCE` to its checkout, `COVENANT_NODE_CONFIG` to its generated
`build/test/config.ini`, and `COVENANT_RESULTS` to an output file outside Git.

```sh
just covenant-unit
just covenant-int
# After building the full applications, use their paths instead of examples:
COVENANT_WALLET=/path/to/paperclip-wallet \
COVENANT_ASP=/path/to/paperclip-asp just covenant-int
```

## Experimental scheduler and watchman

The native ASP and watchman binaries also offer a separate, long-running
`covenant-service` mode. They share an owner-only directory, an OS file lock,
and an atomically written, fsynced signed-transaction journal. The ASP funds
preauthorized replacement trees when their scheduled height arrives. Watchman
observes stale unrolls and broadcasts the authorized refunds. Both roles reuse
the same signed transactions after a restart or reorg.

This mode does not start the normal Ark API or connect to its PostgreSQL database.
Enrollment is a local administrative CLI operation. It validates the permit
chain, service identity, confirmed unspent backing, exact script/value and
reaction window. Re-enrolling the same request is idempotent; conflicting
requests for the same backing are rejected.

```sh
cargo build --locked -p bark-server --features experimental-covenants \
  --bin paperclip-asp --bin paperclip-watchman
paperclip-asp covenant-enroll --service-config service.json --request enrollment.json
paperclip-asp covenant-service --service-config service.json
# Separate terminal/process, same configuration and state directory:
paperclip-watchman covenant-service --service-config service.json
```

Example configuration (paths are placeholders):

```json
{
  "experimental_test_only": true,
  "network": "signet",
  "state_dir": "/private/covenant-test/state",
  "rpc_url": "http://127.0.0.1:48332/",
  "cookie_file": "/private/covenant-test/rpc.cookie",
  "funding_wallet": "covenant-test-example",
  "server_key_file": "/private/covenant-test/server.key",
  "max_funding_sat": 400000,
  "poll_ms": 2000
}
```

Use a dedicated descriptor wallet and disposable server key, stored with mode
0600 under an owner-only directory. Use `regtest` for private tests. Signet mode
checks the exact experimental challenge. RPC must be loopback; use a private
SSH tunnel when the node is remote. Do not expose its wallet RPC publicly.
Enrollment JSON contains `funding` (`txid:vout`) and the ordered `permits` array
generated by the existing wallet `authorize` command.

The observer reads blocks directly and checks active-chain hashes; txindex is
unnecessary. Its in-memory index is rebuilt after restart. This bounded test
implementation stops progressing at height 10,000 and requires unpruned blocks.
The journal and `recovery.json` preserve public state and signed transactions.
Keep the owner's keys and recovery data independently: a server-local export
alone does not protect against server loss or a missed expiry.

To run the native process tests, set the three binary paths plus the node/evidence
variables described above, then run `just covenant-services`. This creates a
new private chain, enables descriptor wallets, leaves txindex disabled, and tests
crashes before broadcasts, concurrent schedulers, autonomous stale-exit response,
reorg reconciliation, and final recovery with both services stopped. Test-only
failpoints `after-funding-journal` and `after-refund-journal` fire once per journal.

The test starts a fresh private regtest node with the signet's experimental
covenant rules and active Bitcoin/RDTS rules. It does not sync mainnet or touch
the public seed. It tests two offline refreshes, output/recipient mutation,
missing authorization, input reordering, timelocks, old-balance double claims,
node restart, one-block reorg, server-absent recovery and no-refresh fallback.

## Important limits

The [data-policy characterization](DATA-POLICY.md) confirms that general CSFS
scripts can carry arbitrary signed messages within existing limits. Equivalent
SHA256-only samples were smaller. The implementation is not claimed to prevent
arbitrary data storage or to have completed an exhaustive spam/DoS review.

- One balance per tree and refund; no aggregation, multi-input consolidation,
  Arkoor transfers or Lightning in this protocol yet.
- No production scheduler, refresh mailbox or wallet UI integration. The separate
  service mode implements test scheduling and watching, but does not integrate
  with normal Ark APIs, database migrations, or user account balances.
- The server must remain online to fund rounds and react to stale exits. An
  offline user needs monitoring and recovery data available independently of
  the server. Offline refresh is not indefinite offline safety.
- Missing a refresh does not remove the original exit path. However, the user
  must unroll the tree before its server-reclaim expiry. A stopped ASP and an
  offline user are not automatically protected by this prototype.
- Test service enrollment validates backing and duplicates under a shared lock;
  signed transactions are saved before broadcast. Production deployment still
  needs independent recovery delivery, stronger operational monitoring, database
  integration and wider adversarial testing. Fixed test-chain bounds and a single
  shared directory are not a distributed deployment design.
- Fixed test budgets: 1,000 sats each for tree unroll, refund and final claim;
  at most 1,000 sats reduction per refresh. These are funded test fee reserves,
  not fee estimates or production pricing. Fee spikes/fee bumping are not solved.
- The node does not globally require unified signatures. These tools always emit
  `0x21` unified transaction signatures. CSFS signatures are 64-byte BIP340
  signatures over TEMPLATEHASH, not unified transaction signatures. The test
  explicitly distinguishes these and verifies that relabeling fails.
- Only the experimental network activates these opcodes. On other chains,
  reserved tapscript opcodes can mean OP_SUCCESS. Never fund these scripts on
  mainnet or use a node without the pinned experimental rules.

Normal Bark/Paperclip RPC messages, existing database formats, boarding,
invoices and normal transaction paths are unchanged. This feature is not a fix
for unrelated upstream bugs.
