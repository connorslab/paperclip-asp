# Default-policy exit integration — 2026-09-29

Private regtest profile 1 for ordinary public-key balances. No production service
or real funds were used. The old interactive lab remains separate.

## Mainnet startup attempt

On Flynn, the new explicit opt-in unit test passed. Without the opt-in the real
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

## Verified checks

- Wallet CLI, wallet daemon and ASP compile with the locked dependencies.
- Initial complete default-policy lifecycle: passed in 73.472 seconds.
- Late mailbox receipt and complete-wallet backup restore: passed. A late valid
  payment is retained for recovery rather than discarded or marked spendable.
- Shared protocol source bytes match between the two private repositories.
- Unified signature checks: 3 passed, including Knots reference vectors and
  atomic rejection of a legacy signing request.
- ASP configuration checks: 10 passed. Unsupported Lightning and liquidity-pool
  configurations are refused; the default liquidity pool is empty.
- Library suite: 109 passed, 36 failed, 1 ignored. Unmodified e107d09e baseline:
  107 passed, the exact same 36 failed, 1 ignored. Both new payment reserve tests
  pass. The inherited suite uses a Bitcoin kernel verifier without XBT unified
  sighash support. This is **not an all-green upstream unit suite**; do not hide
  or disable these failures. Actual XBT consensus acceptance is checked by Knots
  in the integration tests.

## Final end-to-end result

Nextest run `b97b5168-f172-4693-8180-f4466394013c`: **2 passed**, 193 unrelated
tests skipped; total 89.412 seconds. Main lifecycle: 77.616 seconds. Late receipt
and full-wallet backup restore: 11.796 seconds.

The main lifecycle covers boarding, exact transfer/reserve balances, rejection
of dust, shared refresh, cooperative withdrawal, rejection of a legacy sighash
byte, ASP-offline exit with zero on-chain fee balance, relay to an independent
default-policy node, backend restart/mempool loss, and a reorganization removing
an already-claimable exit. Both affected wallets subsequently recover and claim.
The reorganization test found and drove fixes for stale claimable state and
stale-block transactions being mistaken for mempool transactions.

Tested debug binary SHA256:

```text
bcc8298d1e2b3fe4c3c2c36d8dd295c0690df3c8d8f32725e53c0ca899d8ce6f  paperclip-wallet
a791d49f1a09323c3a22ad5683da1830b0bd6b34ce6b2bb949e84c5e8e0c97f2  paperclip-walletd
698e4f192d2a6d03fb662ee4f422d8f7dfdb6334f0759746336fe4902d966151  paperclip-asp
```

Flynn evidence is retained at
`/home/connor/paperclip-ark-local-test/paperclip-system/paperclip-asp/.state/default-policy.lCiMhv/stale-lifecycle.log`
and `paperclip-system/funded-build/`. These logs and generated test wallet data
stay private. Production lightningd and RTL remained active.

## Reproduction

Build both repositories in their Nix shells, then in the ASP repository:

```sh
export PAPERCLIP_WALLET_BIN=/absolute/path/to/paperclip-wallet
export PAPERCLIP_ASP_BIN=/absolute/path/to/paperclip-asp
export XBT_BITCOIND=/absolute/path/to/knots/bin/bitcoind
nix develop --command bash scripts/test-pair.sh
```

Knots binary: v29.4.2.knots20260508,
SHA256 `d04cd8211e711af989a7a62d0b8b55a8cfe496694392518da0ccb848469b3799`.
Harness source: e107d09e7fd173fc5895a2b9eec500082f48d4cd.
Harness adapters use loopback, recognize the Paperclip binary name, choose the
Nix Bash path, and remove relay exemptions. The test uses standardness and
asserts default TRUC acceptance, 1 sat/vbyte relay floor, and 3 sat/vbyte dust rate.
An old RPC script-type enum is avoided with a raw JSON block lookup; no consensus
or transaction assertion is bypassed.

## Boundaries

This is a tested regtest implementation, not production approval. VTXOs still
expire; a seed alone is not a complete recovery backup. Finite fees cannot
guarantee confirmation during arbitrary congestion or censorship. New admission
fails closed outside the supported policy envelope. Legacy signed VTXOs need
cooperative refresh to obtain funded recovery paths. HTLCs, server liquidity-pool
issuance, completed mainnet transaction validation, and comprehensive adversarial/reorg testing are outside this
profile. The earlier interactive lab's API and UI tests are historical checks,
not a new browser test of this revision. See FUNDED-EXITS.md for accounting.

## Lightning preparation — 2026-09-30

The isolated FLYNN source checkout passed `just unit-server xbt_backend`
(one CLN admission test) and `just checks` (workspace, tests, examples).
The CLN test covers the pinned prerelease feature declarations and rejects old,
missing, and optional-only identity declarations. No real CLN connection or
Lightning payment was made. See `LIGHTNING.md` for the remaining protocol work.

## Platform preparation — 2026-09-30

Two CLN capability tests now pass, including extra features and padded vectors.
The container configuration generator passed local tests and both generated ASP
and watchman configurations passed the native binary's `check-config` command
in the isolated FLYNN lab. RPC endpoint selection is independent of provider;
admin access stays loopback-only and Lightning remains disabled.

Container images and platform installs have not been tested. The ASP operator
UI, platform dependency discovery, StartOS supervision, and coordinated backup
hooks are still required; see `deployment/PLATFORMS.md`. No VPS changes or real
payments were performed for this increment.

## Funded Lightning integration — 2026-09-30

Supersedes the earlier disabled-Lightning notes. Both workspace checks and eight
funded builder/policy unit tests passed. Four isolated integration tests passed
(run 8444ccef-5dc7-4259-9727-6d1de87bbc22): BOLT11/BOLT12 including amountless
requests, duplicates, payment refunds and reserve accounting, ASP restart,
empty-pool preimage protection, incoming unilateral claim with no on-chain wallet
funds, and outgoing unilateral timeout refund propagated to an independent node.
Knots rejects TRUC and enforces standardness. CLN is v26.06.8-blake2b.5 with XBT hold.

These tests exposed and fixed two wallet recovery defects: explicit exit selection
excluded locked HTLCs, and refund readiness omitted absolute CLTV. CLI and REST
selection now include unspent locked contracts; readiness accounts for CSV and CLTV.
UI contract tests and the hold plugin's BTC-invoice rejection unit test passed.

Umbrel wallet, ASP/watchman and hold-enabled CLN are deployed with digest-pinned
images. Authenticated invoice creation verified the complete wallet–ASP–CLN–hold
path and required XBT invoice bit 512; that unpaid smoke invoice was canceled.
APIs reject unauthenticated access (401), and gRPC ports remain private.
Mainnet settlement validation is separate and awaits the funded channel.

### Authorized Umbrel mainnet funding

900,000 sats sent from FLYNN (plus 230-sat fee) in
`ffa9ef9e7c8ccfe038ebad56369efb352db4d01441d0ad7ad31a7e9280cf4483`.
CLN received 500,000, ASP 300,000, and the user wallet 100,000 sats.
Funding has one confirmation at this checkpoint.

A private 400,000-sat channel to FLYNN was broadcast in
`db697ab0e68a9f2640f50233eaf9923bf151809fc95aedebd6e160225c08b6cd`
(328-sat fee, paid from CLN's allocation), with 200,000 sats on each side.
Channel ID: `cdb6085c2260e1d6ebed5ac99f8051f13b92f9ea3302f540269f8ae6b07a69db`.
It is awaiting lock-in. ASP pool targets are two 100,000-sat VTXOs; issuance
waits for its two-confirmation funding threshold. No mainnet Lightning payment
has settled yet. The user wallet's 100,000 sats remain on-chain.

Next: verify channel normal and usable pool inventory, receive a small Lightning
payment from FLYNN into Ark, then pay small BOLT11/BOLT12 requests back to FLYNN.
Use the newly balanced private channel. No further on-chain withdrawal from
FLYNN is authorized beyond the already sent allocation without a new instruction.

### Mainnet Lightning settlement verified — 2026-09-30

Channel reached CHANNELD_NORMAL with 3 confirmations; pool issuance had 2.
FLYNN paid 50,000 sats into the Ark wallet, credited as 45,800 sats after a
200-sat service fee and 4,000-sat recovery reserve. The wallet then paid FLYNN
15,000 sats by BOLT11 (135 service + 6,000 reserve) and 10,000 sats by BOLT12
(115 service + 6,000 reserve). Both wallet statuses and FLYNN invoices report paid.
Final wallet balances: 8,550 spendable Ark sats and 100,000 confirmed on-chain
sats; no pending sends, claims, rounds, boards, offboards or exits.
The reusable BOLT12 test offer was disabled after settlement.

BOLT11 payment hash: ff0a52d8fd013480dd94af81a4b1f5e744c19a41aaf573561fa44faec3b0af56
BOLT12 payment hash: d190f1637997bda8201a646799bb4dc6e5987ec2f2eaef1bb9a0e0f689096035
This completes the private Umbrel mainnet Lightning smoke test. Offline recovery
remains verified on isolated regtest; no mainnet emergency exit was initiated.
