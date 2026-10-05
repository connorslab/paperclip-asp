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

Use the laboratory commands below, not the normal wallet/ASP startup commands.
There is no public covenant ASP service or automatic signet wallet UI yet.
The reproducible lifecycle driver uses a fresh private regtest node; it does
not connect to the mainnet ASP or broadcast onto the public signet.

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

The test starts a fresh private regtest node with the signet's experimental
covenant rules and active Bitcoin/RDTS rules. It does not sync mainnet or touch
the public seed. It tests two offline refreshes, output/recipient mutation,
missing authorization, input reordering, timelocks, old-balance double claims,
node restart, one-block reorg, server-absent recovery and no-refresh fallback.

## Important limits

- One balance per tree and refund; no aggregation, multi-input consolidation,
  Arkoor transfers or Lightning in this protocol yet.
- No autonomous production scheduler, refresh mailbox, persistent watchtower
  or wallet UI integration. The test driver supplies those actions in a controlled
  order. The new CLI commands are protocol tools, not a deployed Ark service.
- The server must remain online to fund rounds and react to stale exits. An
  offline user needs monitoring and recovery data available independently of
  the server. Offline refresh is not indefinite offline safety.
- Missing a refresh does not remove the original exit path. However, the user
  must unroll the tree before its server-reclaim expiry. A stopped ASP and an
  offline user are not automatically protected by this prototype.
- Before accepting a permit in a future live service, verify the old funding
  transaction, exact script and value, confirmations, unspent status, expiry,
  and duplicate enrollment atomically. Before spending ASP funds, durably save
  the signed funding transaction and recovery data; reorgs and retries must not
  cause duplicate issuance. These are integration requirements, not features
  supplied by a signature-verification call alone.
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
