# Current profile 2 status

The following startup notes are historical. Profile 2 now constructs ordinary
version-2 recovery transactions. Index synchronization and explicit mainnet opt-in
remain required. No public deployment has been made by this update. A separate
raw-transaction mainnet pilot is not evidence of a completed ASP/wallet lifecycle.
See `VALIDATION.md` for the current test evidence.

# Experimental XBT mainnet opt-in

The wallet and ASP accept Bitcoin Blake2b mainnet only when their process has
`PAPERCLIP_XBT_MAINNET=1`. Regtest remains enabled without this setting. Testnet,
signet and other networks remain rejected. This is experimental support, not a
successful mainnet validation or permission to operate a public service.

Use `--mainnet` when creating the wallet, and `network = "bitcoin"` in the ASP
configuration. Keep mainnet data directories, PostgreSQL databases, keys and
loopback RPC ports separate from the existing regtest lab. Never reuse its seed.
The environment opt-in is also needed when reopening a mainnet wallet; retain
it in the private service configuration for recovery.

The existing backend network checks, activated XBT header check, transaction
index requirement, unified signatures, funded recovery checks, and explicit
Lightning capability checks remain in place. Esplora remains unsupported. Mainnet
requires a synchronized Knots XBT backend and synchronized `txindex=1`; verify
`getindexinfo` before funding. Never bypass the index check to get past startup.

For the authorized Flynn experiment, the aggregate allocation limit is 100,000
sats, including ASP reserves and transaction fees. No recurring funding is
authorized. Existing CLN channels are unrelated and must remain untouched.
Before any funding, verify both applications start, both wallet backups exist,
the complete recovery data is retained, and the proposed allocations fit the cap.

On September 29, Flynn's existing CLN backend was synchronized on XBT mainnet
and reported default relay policy, but `getindexinfo` returned `{}`. It cannot
currently support the required confirmed-transaction lookups. Indexing the
existing unpruned backend requires enabling `txindex=1`, restarting that node,
and waiting for the index to synchronize; it does not require a fresh IBD when
the existing block files are intact. This change was not made to the Umbrel node.
No claim of a completed mainnet Ark transfer should be made without a funded
transaction record and confirmed recovery evidence.

## Mainnet startup attempt

On Flynn, the wallet, wallet daemon and ASP builds passed, along with the new
explicit opt-in unit test. Without the opt-in the real
wallet binary refused mainnet. With the opt-in it reached the existing backend
and refused startup with `txindex is not enabled`. The portable Nix environment
could not resolve `.local`, so the test used the address resolved by Flynn's host
resolver for the same Umbrel backend; CLN configuration was not changed.

No funding or transaction broadcast was performed. Spend: **0 sats** of the
authorized 100,000-sat aggregate cap. No mainnet transaction ID exists. Wallet
and ASP mainnet paths additionally require synchronized indexing and a backend
out of IBD. Existing unified-signature and funded-exit rules remain unchanged.

The backend currently reports an empty index list. Enable and synchronize its
transaction index before retrying. No Umbrel restart, new IBD, production service
change, or CLN channel operation was performed by this attempt.

The separate Umbrel test authorized on 2026-09-30 allocates 900,000 sats from
FLYNN plus network fees: 500,000 CLN, 300,000 ASP, and 100,000 user wallet.
This is separate from the older 100,000-sat recovery experiment above.
