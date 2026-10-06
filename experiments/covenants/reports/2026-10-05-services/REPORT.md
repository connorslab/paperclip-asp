# Native covenant ASP/watchman: private and public signet test report

**PASS within the experimental scope below.** Two long-running native service
processes scheduled and monitored two offline refreshes. Watchman handled a
stale exit autonomously. The wallet ultimately recovered **97,000 test sats**
with both services stopped.

This is the separate `covenant-service` mode inside the actual Paperclip ASP and
watchman binaries. It is **not** the normal Ark API/database path, wallet UI, or
a production deployment. One trusted operator drove a single test balance.

## Versions and network

- ASP/watchman source: [`bd67f1d`](https://github.com/connorslab/paperclip-asp/commit/bd67f1d).
- Wallet source: [`2409e284457f68ae3b80871dd0acb5f080568ab7`](https://github.com/connorslab/paperclip-wallet-app/commit/2409e284457f68ae3b80871dd0acb5f080568ab7).
- Experimental node source: `2f142fdd75719d23046be5b1577fc9ef280f3dc3`.
- Profile: `paperclip-signet-offline-refresh-v1`.
- Public Bitcoin signet, heights **304–333**.
- Started: 2026-10-05T23:48:24.028599+00:00; completed: 2026-10-06T00:16:36.255163+00:00.
- ASP executable SHA256: `5b86d732bda6ddaf04a01952672e6de96afe8d58584009018d2530e16e22ef31`.
- Watchman executable SHA256: `99e399e0a19f72e4f0934fd011739f4f6c433059986b43d854ed450f38abb0fd`.
- Wallet executable SHA256: `163aac0e308c9ea0f4e2c645426a89d29edb82beeee35609d8f19213f553da56`.
- Exact challenge is validated by the service; no txindex was enabled.

## Private tests completed first

`just checks`, `just covenant-unit` (three tests), and `just covenant-services`
passed. The service test creates a fresh private regtest chain, uses descriptor
wallets and active RDTS/covenant rules, and leaves txindex disabled. It verifies:

- Mainnet configuration refusal, malformed permit rejection and duplicate
  enrollment without issuing additional funding.
- Crash immediately after funding-journal fsync, before broadcast; restart
  broadcasts the exact signed transaction. Two concurrent schedulers do not
  create duplicate replacement rounds.
- Two autonomous refresh rounds with the wallet process unavailable; watchman
  waits until it observes the stale exit.
- Crash after refund-journal fsync, before refund broadcast; watchman restart
  reuses the same signatures and completes both refunds with the ASP stopped.
- A one-block private reorg is observed as zero confirmations, then reconfirmed
  without issuing different funding or refund transactions.
- A premature claim is rejected; after its timelock the wallet exits with both
  services stopped.

See [regtest.json](regtest.json), [regtest.log](regtest.log),
[checks.log](checks.log) and [wallet-checks.log](wallet-checks.log). Wallet
workspace checks also passed. Harness setup initially needed descriptor-wallet
options and the correct test-framework datadir property. Its reorg generator
also needed a different timestamp to avoid reproducing an invalidated block.
Those setup attempts are not counted as passing tests.

## Public service run

1. Fund a separate test wallet with 0.01 test BTC from the existing signet miner
   wallet. The public web-faucet wallet was not used.
2. Fund a 100,000-sat covenant allocation and enroll two owner-signed permits.
3. Run `paperclip-asp covenant-service` and `paperclip-watchman covenant-service`
   as separate processes, using a shared private journal and an SSH-only RPC
   connection to the existing public signet node.
4. Invoke no wallet command from **2026-10-05T23:50:33.555061+00:00** through
   **2026-10-05T23:56:57.368645+00:00** (0:06:23.813584). ASP funds both authorized rounds
   at their scheduled heights. The controller then broadcasts the original
   stale unroll; watchman constructs and broadcasts both refunds without the
   controller invoking the ASP `refund` tool.
5. Restart both processes for a 20-second smoke check; their signed transactions
   remain unchanged. This is not a second exhaustive public crash/reorg test;
   those recovery cases were tested privately.
6. Stop both services at **2026-10-05T23:56:57.368645+00:00**. Reconstruct the final
   claim from saved state, check premature rejection, wait for natural height
   **332**, then broadcast and confirm the unified-signed wallet claim.

The first recovery-signing request was rejected because the node's default
address type was legacy. The controller resumed from the already-confirmed
refund, explicitly requested a SegWit address and verified the 98,000-sat
backing before signing. No funding/refund was repeated and no invalid claim was
broadcast. [interrupted-results.json](interrupted-results.json) preserves this
interruption; the final results include the reconciliation.

The wallet-offline condition concerns command invocation by the trusted harness,
not physical isolation against an administrator. Owner keys were backed up in
operator-only local storage; the services use their separate server key.
No mainnet funds, production Ark keys, public payload probes, node restarts,
forced public blocks, mock time or public-chain reorgs were used.

## Confirmed transactions

| Step | Height | vbytes | Fee (sats) | Transaction |
| --- | ---: | ---: | ---: | --- |
| dedicated test reserve | 306 | 225 | 225 | [7bca7de2c006…](https://node2.paperclippool.xyz/signet/#tx/7bca7de2c00631a9e4a6f8204ad096b3a5adc7f2404a27c38c5280ecaca34048) |
| original backing | 307 | 234 | 234 | [92d02319ac79…](https://node2.paperclippool.xyz/signet/#tx/92d02319ac798c7aeecbb5505501cd172121c83cac5529e0fc402491663a7110) |
| stale user unroll | 313 | 121 | 1000 | [efe2ab703dea…](https://node2.paperclippool.xyz/signet/#tx/efe2ab703dea2daf655480f0a23d7aabce085b96bebc52a74f62d403795e9af4) |
| round 1 funding_hex | 311 | 234 | 234 | [2b748a6ed701…](https://node2.paperclippool.xyz/signet/#tx/2b748a6ed701a40fed7449cfafbd27f063e3402704706b4bf216b2ad74cc4686) |
| round 1 unroll_hex | 313 | 121 | 1000 | [9ebc0250bcd8…](https://node2.paperclippool.xyz/signet/#tx/9ebc0250bcd8feee887da459f29c4aff1e0cd40b13fc1a3572d8b21c073c4470) |
| round 1 refund_hex | 313 | 313 | 1000 | [c5c3a8d74511…](https://node2.paperclippool.xyz/signet/#tx/c5c3a8d74511f8e6d85ced5290986f1790b526f8d037172118fafe6f9ed71a3a) |
| round 2 funding_hex | 312 | 234 | 234 | [e6e6a7e3067f…](https://node2.paperclippool.xyz/signet/#tx/e6e6a7e3067fbc78c1da05583d5409789b0ba2d1afa0136dbd1d17dc6e37efdc) |
| round 2 unroll_hex | 313 | 121 | 1000 | [1b14c94b9fe7…](https://node2.paperclippool.xyz/signet/#tx/1b14c94b9fe7029adfe039abbb191569bf94683b171f7d625971ec158affb6af) |
| round 2 refund_hex | 313 | 313 | 1000 | [0e64edc73cc1…](https://node2.paperclippool.xyz/signet/#tx/0e64edc73cc1cfe2bbcf05be5fe9ee68df66757a5292c6ed67a0e012f3659992) |
| independent user recovery | 333 | 126 | 1000 | [8539d3d685ae…](https://node2.paperclippool.xyz/signet/#tx/8539d3d685aebf8b0a9e657171d9c99bed52a559a573a039f1fc9e944b321c3a) |

Total recorded fees including the dedicated reserve funding: **6927 test sats**.
User allocation: **100,000 → 99,000 → 98,000 → 97,000**. Both authorized refreshes
reduce the allocation by 1,000 sats; final recovery pays 1,000 sats. Other tree,
refund and wallet-funding costs are additional server-side test costs, not
production fee estimates. The server's reimbursement outputs still require
separate recovery/accounting; the experiment does not automate their recycling.

Full transaction IDs, block hashes and executable hashes are in
[results.json](results.json). [explorer-observations.json](explorer-observations.json)
is a second read through the public explorer, which indexes the same node and is
not an independent consensus-validation node. The existing signet, miner and
mainnet fallback service PIDs/start times were unchanged throughout.

The dedicated test wallet was backed up in private local and server storage,
checksums verified, and unloaded after recovery. The temporary RPC cookie and
tunnel were removed. Keys and wallet backups are excluded from public artifacts.
See [cleanup.json](cleanup.json) for the non-secret cleanup record.

## Data-storage and spam question

`just covenant-data-policy` passed twelve private probes. General CSFS accepted
arbitrary signed messages within existing limits: 80-byte witness items and
256-byte executed script pushes. The respective 81/257-byte probes were rejected.
Equivalent SHA256-only constructions accepted the same tested sizes and were
16 vbytes smaller. These are per-item bounds, not per-transaction payload limits.

Thus the opcodes retain a data-storage surface; these simple probes do not show
cheaper storage from CSFS. They do not prove spam resistance or cover complex
combinations, sustained CPU/memory pressure, policy classification or all abuse
patterns. See [DATA-POLICY.md](../../DATA-POLICY.md) and
[data-policy.json](data-policy.json). No policy or consensus rules were changed
in response to these observations.

## Remaining limits

- One allocation per tree; no normal Ark boarding, Arkoor/Lightning operations,
  wallet UI, PostgreSQL migration or public enrollment API in this mode.
- Recovery data must reach the owner independently. A server-local export alone
  is insufficient. An offline owner can still miss a tree's reclaim deadline;
  there is no promise of indefinite offline safety.
- Fixed test fees, cumulative funding budget and a single shared directory.
  No production fee bumping, distributed service coordination or automatic
  expiry sweep. Operational alarms and broader adversarial tests remain needed.
- The observer scans an unpruned test chain from genesis after each process
  restart and is capped below height 10,000. It is deliberately not a scalable
  production index.
- Unified signatures are used by the protocol signers. CSFS signatures are
  BIP340 message signatures. The node is not globally configured to forbid
  every otherwise-valid legacy transaction signature.

## Public run checks

- two native service processes: ASP independently funds both preauthorized refreshes; watchman remains idle before stale exit.
- watchman autonomously observes and settles stale exit through both permits while wallet remains offline.
- both services restart from their journal without issuing duplicate transactions.
- reconciled confirmed refund; wallet signs a unified recovery to an explicitly supported destination.
- timelocked wallet exit confirms with ASP and watchman stopped, using natural public signet blocks.
- existing signet, miner, and mainnet fallback services were never restarted.
