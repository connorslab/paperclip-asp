//! Pool replenishment must leave payout liquidity available to users.
use std::time::Duration;
use ark_testing::{sat, TestContext};
use server::vtxopool::VtxoTarget;

#[tokio::test]
async fn xbt_offboard_payout_reserve() {
	let ctx = TestContext::new("xbt/offboard-payout-reserve").await;
	ctx.generate_blocks(200).await;
	let srv = ctx.captaind("asp").funded(sat(500_000)).cfg(|c| {
		c.experimental_funded_lightning = true;
		c.vtxopool.onchain_reserve_sat = 450_000;
		// Single-leaf issuances are skipped; use two to exercise funding selection.
		c.vtxopool.vtxo_targets = vec![VtxoTarget { amount: sat(200_000), count: 2 }];
	}).create().await;
	let wallet = ctx.bark("withdrawer", &srv).funded(sat(150_000)).create().await;
	wallet.board_and_confirm_and_register(&ctx, sat(100_000)).await;
	tokio::time::sleep(Duration::from_secs(10)).await;
	assert_eq!(srv.wallet_status().await.rounds.trusted_balance, sat(500_000),
		"automatic pool issuance spent payout reserves");
	let before = wallet.onchain_balance().await;
	wallet.offboard_all(&wallet.get_onchain_address().await).await;
	ctx.generate_blocks(2).await;
	assert!(wallet.onchain_balance().await > before + sat(90_000));
	assert!(srv.wallet_status().await.rounds.trusted_balance < sat(450_000),
		"user offboards must be allowed to spend the reserve");
}
