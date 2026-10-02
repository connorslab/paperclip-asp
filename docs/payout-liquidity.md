# On-chain payout liquidity

Offboards need spendable funds in the ASP rounds wallet. A successful fee estimate
does not reserve server liquidity or guarantee that a withdrawal can broadcast.
Fees earned in Ark are not necessarily immediately spendable on chain.

Set `vtxopool.onchain_reserve_sat` to keep a minimum balance out of automatic pool
issuance. The check includes the issuance transaction's miner fee and excludes
funds already locked by another operation. Offboards and rounds can spend this
reserve: it is a pool-replenishment limit, not a withdrawal limit or guarantee.
The default is zero for compatibility with existing configurations.

Monitor the rounds wallet's trusted balance and replenish before it runs low.
Do not use the watchman's recovery reserve as routine payout funding. If pool
issuance is deferred, Lightning receives that need pool VTXOs may lack capacity.
Size both pools for expected demand.

Insufficient server funds return gRPC `Unavailable` with a retryable liquidity
message. Clients should retain and retry the existing withdrawal rather than
create a replacement. No payout transaction is broadcast when preparation fails
for insufficient funds.

Run `scripts/test-offboard-liquidity.sh` in the documented isolated XBT test
environment to verify that pool issuance preserves the reserve and user
offboards can still spend it.

## Receive-pool age and replenishment

An unspent pool VTXO is not necessarily eligible for a new Lightning receive.
The receive path needs enough time before both its HTLC expiry and its VTXO
expiry to complete recovery. The server excludes inputs that fail this check.
See [the recovery-time rule](../LIGHTNING.md#receive-recovery-time).

Pool replenishment uses a recovery-aware expiry cutoff. Configure a pool
lifetime longer than this cutoff. Ensure that the target outputs, their
recovery reserves, and the funding transaction fee fit above the on-chain
payout reserve. Otherwise replenishment is deferred even when the wallet
shows a positive balance. Aging outputs remain available for their normal
recovery process; they do not count as immediately usable receive capacity.
