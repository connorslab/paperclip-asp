//! Isolated XBT regtest settlement and recovery; never production endpoints.
use std::time::Duration;
use ark_testing::{btc, sat, TestContext};
use ark_testing::util::FutureExt;
use bitcoincore_rpc::RpcApi;
use server::vtxopool::VtxoTarget;

#[tokio::test]
async fn xbt_funded_lightning_settlement() {
	let ctx = TestContext::new("xbt/funded-lightning").await;
	let info: serde_json::Value = ctx.bitcoind().sync_client().call("getmempoolinfo", &[]).unwrap();
	assert_eq!(info["truc_policy"], "reject");
	let ln = ctx.new_lightning_setup("xbt-ln").await;
	let srv = ctx.captaind("asp").lightningd(&ln.internal).funded(btc(2))
		.cfg(|c| {
			c.experimental_funded_lightning = true;
			c.vtxopool.vtxo_targets = vec![VtxoTarget { amount: sat(200_000), count: 4 }];
			c.cln_xpay_timeout = Duration::from_secs(5);
		}).create().await;
	srv.wait_for_vtxopool(&ctx).await;
	let wallet = ctx.bark("wallet", &srv).funded(sat(1_000_000)).create().await;
	wallet.board_and_confirm_and_register(&ctx, sat(500_000)).await;
	assert_eq!(wallet.spendable_balance().await, sat(498_000));
	ln.sync().await;
	let invoice = ln.external.invoice(Some(sat(20_000)), "send", "funded XBT send").await;
	wallet.pay_lightning_wait(&invoice, None).await;
	assert_eq!(wallet.spendable_balance().await, sat(472_000));
	assert!(wallet.try_pay_lightning(&invoice, None, true).await.is_err());
	assert_eq!(wallet.spendable_balance().await, sat(472_000));
	let invoice = ln.external.invoice(None, "amountless", "amountless XBT send").await;
	wallet.pay_lightning_wait(invoice, Some(sat(20_000))).await;
	assert_eq!(wallet.spendable_balance().await, sat(446_000));
	let offer = ln.external.offer(Some(sat(20_000)), Some("XBT BOLT12")).await;
	wallet.pay_lightning_wait(offer, None).await;
	assert_eq!(wallet.spendable_balance().await, sat(420_000));
	let offer = ln.external.offer(None, Some("amountless XBT BOLT12")).await;
	wallet.pay_lightning_wait(offer, Some(sat(20_000))).await;
	assert_eq!(wallet.spendable_balance().await, sat(394_000));
	let invoice = wallet.bolt11_invoice(sat(30_000)).await;
	// The invoice and pool survive an ASP process restart before settlement.
	srv.stop().await.unwrap();
	srv.start().await.unwrap();
	// The test harness assigns a fresh port on restart.
	wallet.set_ark_url(&srv).await;
	tokio::join!(
		ln.external.pay_bolt11(&invoice.invoice),
		wallet.lightning_receive(&invoice.invoice).wait_millis(60_000),
	);
	assert_eq!(wallet.spendable_balance().await, sat(420_000));
	// A disconnected recipient must refund the HTLC, retaining only actual reserves.
	let refund_wallet = ctx.bark("refund-wallet", &srv).funded(sat(200_000)).create().await;
	refund_wallet.board_and_confirm_and_register(&ctx, sat(100_000)).await;
	let unreachable = ctx.lightningd("unreachable").create().await;
	let invoice = unreachable.invoice(Some(sat(20_000)), "refund", "refund test").await;
	refund_wallet.try_pay_lightning(&invoice, None, true).await.unwrap();
	refund_wallet.sync().await;
	assert_eq!(refund_wallet.spendable_balance().await, sat(88_000));
	assert_eq!(refund_wallet.offchain_balance().await.pending_lightning_send, sat(0));
}

#[tokio::test]
async fn xbt_funded_lightning_outgoing_unilateral_refund() {
	let ctx = TestContext::new("xbt/ln-refund-exit").await;
	let relay = ctx.new_bitcoind("independent-relay").await;
	let ln = ctx.new_lightning_setup("ln").await;
	let srv = ctx.captaind("asp").lightningd(&ln.internal).no_vtxo_pool()
		.cfg(|c| { c.experimental_funded_lightning = true; }).create().await;
	let wallet = ctx.bark("wallet", &srv).funded(sat(200_000)).create().await;
	wallet.board_and_confirm_and_register(&ctx, sat(100_000)).await;
	let preimage = ark::lightning::Preimage::random();
	let hash = preimage.compute_payment_hash();
	let invoice = ln.external.hold_client().await.invoice(cln_rpc::plugins::hold::InvoiceRequest {
		payment_hash: hash.to_vec(), amount_msat: 20_000_000,
		description: None, expiry: Some(3600), min_final_cltv_expiry: Some(80), routing_hints: vec![],
	}).await.unwrap().into_inner().bolt11;
	wallet.pay_lightning(&invoice, None).await;
	ln.external.wait_for_hold_invoice_accepted(hash).await;
	let pending = wallet.client().await.pending_lightning_send_vtxos().await.unwrap();
	assert_eq!(pending.len(), 1);
	let id = pending[0].id();
	srv.stop().await.unwrap();
	wallet.start_exit_vtxos([id]).await;
	wallet.progress_exit().await;
	tokio::time::timeout(Duration::from_secs(15), async {
		loop {
			if !relay.sync_client().get_raw_mempool().unwrap().is_empty() { break; }
			tokio::time::sleep(Duration::from_millis(100)).await;
		}
	}).await.expect("independent relay did not receive funded exit");
	ark_testing::exit::complete_exit(&ctx, &wallet).await;
	let before = wallet.onchain_balance().await;
	wallet.claim_all_exits(wallet.get_onchain_address().await).await;
	ctx.generate_blocks(1).await;
	assert!(wallet.onchain_balance().await > before + sat(18_000));
}

#[derive(Clone)]
struct RefuseIncomingClaim;
#[async_trait::async_trait]
impl ark_testing::daemon::captaind::proxy::ArkRpcProxy for RefuseIncomingClaim {
	async fn claim_lightning_receive(
		&self, _upstream: &mut ark_testing::daemon::captaind::ArkClient,
		_req: server_rpc::protos::ClaimLightningReceiveRequest,
	) -> Result<server_rpc::protos::ArkoorPackageCosignResponse, tonic::Status> {
		Err(tonic::Status::unavailable("isolated recovery test: no cooperative claim"))
	}
}

#[tokio::test]
async fn xbt_funded_lightning_incoming_unilateral_claim() {
	let ctx = TestContext::new("xbt/ln-receive-exit").await;
	let ln = ctx.new_lightning_setup("ln").await;
	let srv = ctx.captaind("asp").lightningd(&ln.internal).funded(btc(2))
		.cfg(|c| {
			c.experimental_funded_lightning = true;
			c.vtxopool.vtxo_targets = vec![VtxoTarget { amount: sat(200_000), count: 4 }];
		}).create().await;
	srv.wait_for_vtxopool(&ctx).await;
	let proxy = srv.start_proxy_no_mailbox(RefuseIncomingClaim).await;
	let wallet = ctx.bark("wallet", &proxy.address)
		.cfg(|c| { c.lightning_receive_claim_retries = 0; }).create().await;
	let invoice = wallet.bolt11_invoice(sat(30_000)).await;
	let inv = invoice.invoice.clone();
	let external = ln.external;
	let payment = tokio::spawn(async move { external.try_pay_bolt11(inv).await });
	assert!(wallet.try_lightning_receive(&invoice.invoice).wait_millis(60_000).await.is_err());
	let parsed: ark::lightning::Invoice = invoice.invoice.parse().unwrap();
	let state = wallet.client().await.lightning_receive_state(parsed.payment_hash()).await.unwrap();
	let bark::actions::lightning::receive::LightningReceiveState::InProgress(receive) = state else {
		panic!("expected a pending recovery contract");
	};
	let bark::actions::lightning::receive::Progress::PreimageRevealed(htlcs) = receive.progress else {
		panic!("expected persisted claim attempt");
	};
	assert_eq!(wallet.onchain_balance().await, sat(0));
	srv.stop().await.unwrap();
	wallet.start_exit_vtxos(htlcs.vtxo_ids).await;
	ark_testing::exit::complete_exit(&ctx, &wallet).await;
	wallet.claim_all_exits(wallet.get_onchain_address().await).await;
	ctx.generate_blocks(1).await;
	assert!(wallet.onchain_balance().await > sat(28_000));
	payment.abort();
}

#[tokio::test]
async fn xbt_funded_lightning_empty_pool_keeps_preimage_private() {
	let ctx = TestContext::new("xbt/ln-empty-pool").await;
	let ln = ctx.new_lightning_setup("ln").await;
	let srv = ctx.captaind("asp").lightningd(&ln.internal).no_vtxo_pool()
		.cfg(|c| { c.experimental_funded_lightning = true; }).create().await;
	let wallet = ctx.bark("wallet", &srv).create().await;
	let invoice = wallet.bolt11_invoice(sat(30_000)).await;
	let parsed: ark::lightning::Invoice = invoice.invoice.parse().unwrap();
	let hash = parsed.payment_hash();
	let inv = invoice.invoice.clone();
	let external = ln.external;
	let payment = tokio::spawn(async move { external.try_pay_bolt11(inv).await });
	ln.internal.wait_for_hold_invoice_accepted(hash).await;
	let attempt = tokio::time::timeout(Duration::from_secs(3), wallet.try_lightning_receive(&invoice.invoice)).await;
	assert!(!matches!(attempt, Ok(Ok(_))));
	let state = wallet.client().await.lightning_receive_state(hash).await.unwrap();
	let bark::actions::lightning::receive::LightningReceiveState::InProgress(receive) = state else {
		panic!("expected an unclaimed payment");
	};
	assert!(matches!(receive.progress, bark::actions::lightning::receive::Progress::AwaitingPayment));
	assert_eq!(wallet.spendable_balance().await, sat(0));
	assert!(!payment.is_finished(), "empty pool must not settle the hold invoice");
	ln.internal.hold_client().await.cancel(cln_rpc::plugins::hold::CancelRequest {
		payment_hash: hash.to_vec(),
	}).await.unwrap();
	assert!(tokio::time::timeout(Duration::from_secs(15), payment).await.unwrap().unwrap().is_err());
}

#[tokio::test]
async fn xbt_funded_lightning_receive_after_pool_ages() {
	let ctx = TestContext::new("xbt/ln-aged-pool").await;
	let ln = ctx.new_lightning_setup("ln").await;
	let srv = ctx.captaind("asp").lightningd(&ln.internal).funded(btc(2))
		.cfg(|c| {
			c.experimental_funded_lightning = true;
			c.vtxo_exit_delta = bitcoin_ext::BlockDelta::new(144);
			c.htlc_expiry_delta = bitcoin_ext::BlockDelta::new(40);
			c.max_user_invoice_cltv_delta = bitcoin_ext::BlockDelta::new(250);
			c.htlc_send_expiry_delta = bitcoin_ext::BlockDelta::new(258);
			c.vtxopool.vtxo_lifetime = bitcoin_ext::BlockDelta::new(432);
			c.vtxopool.vtxo_pre_expiry = bitcoin_ext::BlockDelta::new(144);
			c.vtxopool.vtxo_targets = vec![VtxoTarget { amount: sat(300_000), count: 2 }];
		}).create().await;
	srv.wait_for_vtxopool(&ctx).await;
	// Old outputs now pass the old 144-block cutoff but fail recovery headroom.
	srv.stop().await.unwrap();
	ctx.generate_blocks(240).await;
	srv.start().await.unwrap();
	srv.wait_for_vtxopool(&ctx).await;
	let wallet = ctx.bark("receiver", &srv).funded(sat(100_000)).create().await;
	wallet.board_and_confirm_and_register(&ctx, sat(50_000)).await;
	ln.sync().await;
	let before = wallet.spendable_balance().await;
	let invoice = wallet.bolt11_invoice(sat(10_000)).await;
	tokio::join!(
		ln.external.pay_bolt11(&invoice.invoice),
		wallet.lightning_receive(&invoice.invoice).wait_millis(60_000),
	);
	assert_eq!(wallet.spendable_balance().await, before + sat(6_000));
}
