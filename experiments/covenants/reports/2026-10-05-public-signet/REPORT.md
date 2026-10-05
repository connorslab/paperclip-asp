# Paperclip Ark covenant experiment: public signet offline refresh

**Result: PASS within the laboratory scope below.** Two preauthorized refreshes
completed with no wallet command during the offline interval. The final user
claim recovered **97,000 test sats**, with no ASP command during recovery.

This is a single-balance, VTXO-like covenant protocol test. It is not a test of
the normal Ark VTXO database, autonomous refresh scheduler, wallet UI or public
ASP API. It does not establish production readiness or indefinite offline safety.

## Run identity

- Date: October 5, 2026.
- Started: 2026-10-05T22:56:54.021429+00:00.
- Completed: 2026-10-05T23:17:37.852534+00:00.
- Public Bitcoin signet: heights **253–274**.
- Node version: `/Satoshi:29.4.2/Knots:20260508.paperclip-signet1-g2f142fdd75719d23046be5b1577fc9ef280f3dc3/`.
- Challenge: `2102396d38e3ff703be31a2d97317835f4e9645b5ae5ea2d2d1ba406afa7ab185b5fac`.
- Profile: `paperclip-signet-offline-refresh-v1`.
- Wallet source: [`2409e284457f68ae3b80871dd0acb5f080568ab7`](https://github.com/connorslab/paperclip-wallet-app/commit/2409e284457f68ae3b80871dd0acb5f080568ab7).
- ASP executable code source: [`1c26035a4e8e62d79bca7b575b5657c45d23a3a2`](https://github.com/connorslab/paperclip-asp/commit/1c26035a4e8e62d79bca7b575b5657c45d23a3a2). Later ASP branch commits changed documentation only.
- Wallet SHA256: `163aac0e308c9ea0f4e2c645426a89d29edb82beeee35609d8f19213f553da56`.
- ASP SHA256: `c1686ad4bb138820b5103904c6e5438ec9d884abde6f07607cdbaa8306ab7626`.

## What ran

1. Fund a dedicated test wallet with 0.01 test BTC. Fund an initial covenant
   tree representing a 100,000-sat user allocation.
2. Generate and persist two owner authorizations before their replacement
   round funding outpoints exist. Each authorization commits the next owner,
   amount, expiry, server and earliest refund height.
3. Disable wallet command invocation in the harness. Using persisted permits,
   fund two replacement trees and exercise their on-chain refund paths. Each
   refund consumes both the old and replacement exit outputs, preserves the
   new user's authorized allocation, and returns duplicate backing to the server.
4. Reload saved recovery data, disable ASP command invocation, and restore
   wallet command access. Check premature recovery rejection; wait for real
   signet height 273; broadcast and confirm the unified-signed user claim.

The wallet-offline interval was **2026-10-05T22:57:37.119779+00:00 to 2026-10-05T23:06:44.573757+00:00** (0:09:07.453978). Command logs
contain no wallet invocation in that interval. The ASP-offline phase began at
2026-10-05T23:06:44.588940+00:00; it contains no subsequent ASP invocation. These are
subprocess invocation guards in a trusted test harness, not physically isolated
machines or a proof that a malicious server cannot access a user's keys.

The shared signet produced blocks normally. No generation, mock-time,
invalidation, reorganization, policy change or restart RPC was used. Pool and
production Ark services were not modified. Recovery keys remain in private
test storage; public artifacts include only public states and observations.

## Confirmed transactions

All nine transactions were cross-checked against the public explorer after
the final claim. The explorer is a separate index of the same node, not an
independent consensus-validation node. Full transaction IDs and block hashes are in
[results.json](results.json), with a second observation in
[explorer-observations.json](explorer-observations.json).

| Step | Block | vbytes | Fee (sats) | Transaction |
|---|---:|---:|---:|---|
| initial round funding | 254 | 156 | 156 | [d6a23a82e7b2…](https://node2.paperclippool.xyz/signet/#tx/d6a23a82e7b28fa4389929d2146549a907423405dc986d7d7050ce1103b9eda4) |
| refresh 1 round funding | 255 | 234 | 234 | [31a7d8243c31…](https://node2.paperclippool.xyz/signet/#tx/31a7d8243c31b3082d7c98b070a893631f416e18483a18ba53701f47dfc62098) |
| old balance unroll | 256 | 121 | 1000 | [5176a7d2dce9…](https://node2.paperclippool.xyz/signet/#tx/5176a7d2dce9223f7e2a7a1f051f4cb9f84a39db9bb0ff68f16da6fde568930d) |
| refresh 1 new balance unroll | 257 | 121 | 1000 | [826833eebb86…](https://node2.paperclippool.xyz/signet/#tx/826833eebb86225a9a3ccbda3a70380713dc5e80532ed413f2c292dc57788f7c) |
| offline refresh 1 refund | 258 | 313 | 1000 | [7b9c723662cb…](https://node2.paperclippool.xyz/signet/#tx/7b9c723662cbd93a7ed5b13a8a4c79a382610e1b15b0a0926303eb7aee65c2db) |
| refresh 2 round funding | 261 | 234 | 234 | [db4f5720c23b…](https://node2.paperclippool.xyz/signet/#tx/db4f5720c23b93b567dfa96e55eccda2caa16366a822d81b6ba980f8f735668c) |
| refresh 2 new balance unroll | 262 | 121 | 1000 | [d957e9940e48…](https://node2.paperclippool.xyz/signet/#tx/d957e9940e486a65ac17dfb50161927d273062f352ba82219e3e8ed354b4e160) |
| offline refresh 2 refund | 263 | 313 | 1000 | [57f12098d924…](https://node2.paperclippool.xyz/signet/#tx/57f12098d924b5ce0fa25cec5c1a863814e27d2ffc73da07e640f24ec985361f) |
| unilateral user recovery with ASP offline | 274 | 126 | 1000 | [bfaffcb05716…](https://node2.paperclippool.xyz/signet/#tx/bfaffcb05716390c4719baa68623e3e78b23b122ba89e5e943db9503e7c9730e) |

## Amounts and fees

| Stage | User allocation |
|---|---:|
| Initial tree | 100,000 sats |
| First authorized replacement | 99,000 sats |
| Second authorized replacement | 98,000 sats |
| Final recovered output | 97,000 sats |

Each refresh explicitly authorizes a 1,000-sat reduction; the final claim pays
1,000 sats. Three tree unrolls and two refunds also pay 1,000 sats each. The six
covenant spends therefore pay **6,000 sats** in total. The three round-funding
transactions pay **624 sats** in total at the requested 1 sat/vbyte;
total fees for the nine listed transactions are **6624 sats**.
The separate 0.01-test-BTC wallet funding transfer is not part of that fee total.
Its transaction ID is [0c10b12bac023912ffa746d79a13031f87ed897e220fef66876dc8725b4394af](https://node2.paperclippool.xyz/signet/#tx/0c10b12bac023912ffa746d79a13031f87ed897e220fef66876dc8725b4394af).
The two refunds returned 99,000 and 98,000 sats of duplicate backing to the
server's test key, in addition to their respective user allocations.
These fixed covenant fee reserves are test constants, not optimized production
fees. This run did not sweep the server reimbursement outputs.

## Checks

- Pinned public signet, synchronized and funded with test coins only.
- Altered permit metadata rejected before funding.
- refresh 1 rejects changed amount. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- refresh 1 rejects changed destination. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- refresh 1 rejects changed authorization. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- refresh 1 rejects changed input-order. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- refresh 1 rejects changed sighash. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- Refresh 1 consumed both inputs and preserved authorized allocation.
- Harness resumed from confirmed first refund without replaying any send.
- refresh 2 rejects changed amount. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- refresh 2 rejects changed destination. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- refresh 2 rejects changed authorization. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- refresh 2 rejects changed input-order. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- refresh 2 rejects changed sighash. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- Refresh 2 consumed both inputs and preserved authorized allocation.
- Two refreshes completed with no wallet process invocation during the offline interval.
- User recovery rejected before reaction window expires. Rejection: `non-final`.
- Mature old balance cannot be claimed again after refresh. Rejection: `missing-inputs`.
- Unified recovery signature cannot be relabeled as legacy. Rejection: `mempool-script-verify-flag-failed (Invalid Schnorr signature)`.
- Valid separately generated legacy signature remains consensus-accepted; signer enforcement is not global.
- User recovered 97000 sats from persisted data with no ASP invocation.

Output/input mutations change signed transaction data, so their rejection alone
does not isolate TEMPLATEHASH from ordinary signature checks. The missing-owner
authorization check directly exercises the CSFS authorization requirement.
These tests complement, but do not replace, the earlier unit/vector tests.

## Harness interruption and reconciliation

After the first refund confirmed in block 258, the test adapter treated the
CLI's empty response for a spent output as an empty string instead of null.
The assertion stopped the harness. This was not a rejected refund or an
identified wallet/ASP protocol failure. The adapter was corrected, both spent
inputs and the 99,000-sat output were checked on-chain, and the run resumed at
that checkpoint. No funding or refund transaction was resent. The interrupted
record is retained separately; the final evidence records the interruption.

## Limits and next tests

- The test deliberately unrolls trees and exercises refunds to test stale-exit
  safety. It does not demonstrate that ordinary refreshes need these on-chain
  exit transactions or measure the normal off-chain refresh cost.
- Both preauthorized refreshes executed after their earliest allowed heights.
  Premature-refund rejection was not reached in this public run; the earlier
  private-regtest report covers that check. Premature final user recovery was
  rejected on this signet.
- The signer emits unified transaction signatures, and relabeling them as
  legacy fails. A separately generated valid legacy signature remains accepted
  by this node. No global unified-signature consensus rule is claimed. The valid
  legacy variant was tested with testmempoolaccept only, never broadcast.
- No public-chain reorg, node restart, or missed-expiry race was induced. Earlier
  regtest evidence addresses some of these; they are not results of this run.
- There is no autonomous scheduler, durable service issuance/refund accounting,
  independent recovery-data service, persistent exit monitor, multi-user tree,
  wallet UI integration or mainnet deployment demonstrated here.
- Future work should test persistent scheduler crash recovery, duplicate permits,
  independent recovery delivery, reorgs around refund/expiry boundaries, fee
  pressure and concurrent users on isolated test infrastructure.

## Reproduction references

- [Wallet experimental branch](https://github.com/connorslab/paperclip-wallet-app/tree/experiment/covenant-offline-refresh).
- [ASP experimental branch](https://github.com/connorslab/paperclip-asp/tree/experiment/covenant-offline-refresh).
- See `experiments/covenants/README.md` for build instructions and the existing
  disposable-regtest driver. That driver must not be pointed at a shared signet:
  it uses generation, restart and reorg controls. This public run used a separate
  restricted harness, recorded locally with SHA256
  `d223fe00b4bb82049fd9fbf498c983d5958f9fbd5157c22fae2bfb693035f6e6`.
