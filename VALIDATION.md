# Default-policy exit integration — 2026-09-29

Private regtest profile 1 for ordinary public-key balances. No production service
or real funds were used. The old interactive lab remains separate.

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
issuance, mainnet, and comprehensive adversarial/reorg testing are outside this
profile. The earlier interactive lab's API and UI tests are historical checks,
not a new browser test of this revision. See FUNDED-EXITS.md for accounting.
