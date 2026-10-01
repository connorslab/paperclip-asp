//! Real onion-message and held-payment interoperability on isolated XBT regtest.
use std::time::Duration;
use ark_testing::{btc, sat, TestContext};
use server::vtxopool::VtxoTarget;

#[tokio::test]
async fn xbt_reusable_bolt12_receive() {
	let ctx = TestContext::new("xbt/reusable-bolt12").await;
	// Keep Core's random anti-fee-sniping backdating away from locktime 21,
	// which Knots' default CAT21 policy rejects. Do not relax relay policy.
	ctx.generate_blocks(200).await;
	let ln = ctx.new_lightning_setup("ln").await;
	let srv = ctx.captaind("asp").lightningd(&ln.internal).funded(btc(2))
		.cfg(|c| {
			c.experimental_funded_lightning = true;
			c.experimental_bolt12_receive = true;
			c.invoice_check_interval = Duration::from_millis(200);
			c.receive_htlc_forward_timeout = Duration::from_secs(8);
			c.vtxopool.vtxo_targets = vec![VtxoTarget { amount: sat(200_000), count: 4 }];
		}).create().await;
	srv.wait_for_vtxopool(&ctx).await;
	let wallet = ctx.bark("receiver", &srv).funded(sat(500_000)).create().await;
	wallet.board_and_confirm_and_register(&ctx, sat(200_000)).await;
	let client = wallet.client().await;
	client.start_daemon().unwrap();
	let offer = client.create_lightning_offer("Reusable regtest offer".into(), None).await.unwrap();
	tokio::time::sleep(Duration::from_secs(8)).await;
	// Knowing a public offer does not authorize replacing its wallet session.
	let (tx, rx) = tokio::sync::mpsc::channel(1);
	let mut impostor = srv.get_public_rpc().await.serve_lightning_offers(
		tokio_stream::wrappers::ReceiverStream::new(rx),
	).await.unwrap().into_inner();
	assert_eq!(impostor.message().await.unwrap().unwrap().challenge.len(), 32);
	tx.send(server_rpc::protos::LightningOfferClient {
		offer: offer.offer.clone(), challenge_signature: vec![0; 64], ..Default::default()
	}).await.unwrap();
	assert_eq!(impostor.message().await.unwrap_err().code(), tonic::Code::Unauthenticated);
	ln.sync().await;
	let before = wallet.spendable_balance().await;
	let mut invoices = Vec::new();
	for amount in [30_000, 40_000] {
		let invoice = ln.external.grpc_client().await.fetch_invoice(cln_rpc::FetchinvoiceRequest {
			offer: offer.offer.clone(), amount_msat: Some(cln_rpc::Amount { msat: amount * 1000 }),
			..Default::default()
		}).await.unwrap().into_inner().invoice;
		assert!(invoice.starts_with("lni1"));
		let pending = client.pending_lightning_receives().await.unwrap();
		assert!(pending.iter().any(|p| p.invoice.to_string() == invoice));
		ln.external.try_pay_bolt11(&invoice).await.unwrap();
		tokio::time::timeout(Duration::from_secs(60), async {
			loop {
				client.sync_pending_lightning_receives().await.unwrap();
				if client.pending_lightning_receives().await.unwrap().is_empty() { break; }
				tokio::time::sleep(Duration::from_millis(200)).await;
			}
		}).await.unwrap();
		invoices.push(invoice);
	}
	assert_ne!(invoices[0], invoices[1]);
	assert!(wallet.spendable_balance().await > before + sat(50_000));
	// Separate payers may request invoices concurrently from the same offer.
	let fetch = async || {
		ln.external.grpc_client().await.fetch_invoice(cln_rpc::FetchinvoiceRequest {
			offer: offer.offer.clone(), amount_msat: Some(cln_rpc::Amount { msat: 30_000_000 }),
			..Default::default()
		}).await.unwrap().into_inner().invoice
	};
	let (first, second) = tokio::join!(fetch(), fetch());
	let first_parsed = first.parse::<ark::lightning::Invoice>().unwrap();
	let second_parsed = second.parse::<ark::lightning::Invoice>().unwrap();
	assert_ne!(first_parsed.payment_hash(), second_parsed.payment_hash());
	let (first_paid, second_paid) = tokio::join!(
		ln.external.try_pay_bolt11(&first), ln.external.try_pay_bolt11(&second),
	);
	first_paid.unwrap();
	second_paid.unwrap();
	// An issued invoice and its wallet checkpoint must survive both processes restarting.
	let interrupted = fetch().await;
	client.stop_daemon_wait().await.unwrap();
	drop(client);
	srv.stop().await.unwrap();
	srv.start().await.unwrap();
	// The integration harness allocates new RPC ports on every start.
	wallet.set_ark_url(&srv).await;
	let reopened = wallet.client().await;
	assert_eq!(reopened.lightning_offer().await.unwrap().unwrap().offer, offer.offer);
	reopened.start_daemon().unwrap();
	tokio::time::sleep(Duration::from_secs(8)).await;
	ln.external.try_pay_bolt11(&interrupted).await.unwrap();
	// Run the immutable public 0.7.7 CLI against the new ASP, not a rebuilt old source tree.
	let legacy_exec = std::env::var("LEGACY_BARK_EXEC").expect("released wallet binary required");
	let legacy = ctx.bark("legacy", &srv).exec(legacy_exec).funded(sat(500_000)).create().await;
	legacy.board_and_confirm_and_register(&ctx, sat(200_000)).await;
	let legacy_before = legacy.spendable_balance().await;
	let legacy_invoice = legacy.bolt11_invoice(sat(30_000)).await;
	assert!(legacy_invoice.invoice.starts_with("lnbcrt"));
	let (paid, ()) = tokio::join!(
		ln.external.try_pay_bolt11(&legacy_invoice.invoice),
		legacy.lightning_receive(&legacy_invoice.invoice),
	);
	paid.unwrap();
	assert!(legacy.spendable_balance().await > legacy_before);
	legacy.pay_lightning_wait(&offer.offer, Some(sat(30_000))).await;
	// Canceling one invoice must neither revive that payment nor disable the offer.
	let canceled = fetch().await;
	let canceled_hash = canceled.parse::<ark::lightning::Invoice>().unwrap().payment_hash();
	// The legacy unauthenticated cancel RPC is deliberately disabled. A refusal
	// must preserve the wallet checkpoint; only the server may cancel the hold.
	assert!(reopened.cancel_lightning_receive(canceled_hash).await.is_err());
	assert!(reopened.pending_lightning_receives().await.unwrap().iter().any(|p| p.payment_hash == canceled_hash));
	ln.internal.hold_client().await.cancel(cln_rpc::plugins::hold::CancelRequest {
		payment_hash: canceled_hash.to_vec(),
	}).await.unwrap();
	tokio::time::sleep(Duration::from_secs(2)).await;
	reopened.sync_pending_lightning_receives().await.unwrap();
	assert!(ln.external.try_pay_bolt11(&canceled).await.is_err());
	assert!(!reopened.pending_lightning_receives().await.unwrap().iter().any(|p| p.payment_hash == canceled_hash));
	// If the wallet goes offline after issuing an invoice, the payer must get
	// its held payment back. No Ark credit or preimage is produced on timeout.
	let offline_invoice = fetch().await;
	let offline_hash = offline_invoice.parse::<ark::lightning::Invoice>().unwrap().payment_hash();
	reopened.stop_daemon_wait().await.unwrap();
	let before_offline = wallet.spendable_balance().await;
	assert!(tokio::time::timeout(Duration::from_secs(90), ln.external.try_pay_bolt11(&offline_invoice)).await.unwrap().is_err());
	reopened.sync_pending_lightning_receives().await.unwrap();
	assert!(!reopened.pending_lightning_receives().await.unwrap().iter().any(|p| p.payment_hash == offline_hash));
	assert_eq!(wallet.spendable_balance().await, before_offline);
	reopened.start_daemon().unwrap();
	tokio::time::sleep(Duration::from_secs(8)).await;
	let invoice = ln.external.grpc_client().await.fetch_invoice(cln_rpc::FetchinvoiceRequest {
		offer: offer.offer.clone(), amount_msat: Some(cln_rpc::Amount { msat: 30_000_000 }),
		..Default::default()
	}).await.unwrap().into_inner().invoice;
	reopened.disable_lightning_offer().await.unwrap();
	// Disabling the reusable offer must not discard an already-issued payment.
	ln.external.try_pay_bolt11(&invoice).await.unwrap();
	tokio::time::sleep(Duration::from_secs(6)).await;
	reopened.sync_pending_lightning_receives().await.unwrap();
	assert!(reopened.pending_lightning_receives().await.unwrap().is_empty());
	assert!(ln.external.grpc_client().await.fetch_invoice(cln_rpc::FetchinvoiceRequest {
		offer: offer.offer, amount_msat: Some(cln_rpc::Amount { msat: 30_000_000 }),
		..Default::default()
	}).await.is_err());
	reopened.stop_daemon();
}

#[tokio::test]
async fn xbt_reusable_bolt12_receive_no_pool() {
	let ctx = TestContext::new("xbt/bolt12-empty-pool").await;
	ctx.generate_blocks(200).await;
	let ln = ctx.new_lightning_setup("ln").await;
	let srv = ctx.captaind("asp").lightningd(&ln.internal).funded(btc(2))
		.cfg(|c| {
			c.experimental_funded_lightning = true;
			c.experimental_bolt12_receive = true;
			c.invoice_check_interval = Duration::from_millis(200);
			c.receive_htlc_forward_timeout = Duration::from_secs(8);
			c.vtxopool.vtxo_targets = vec![];
		}).create().await;
	let wallet = ctx.bark("receiver", &srv).funded(sat(500_000)).create().await;
	wallet.board_and_confirm_and_register(&ctx, sat(200_000)).await;
	let client = wallet.client().await;
	client.start_daemon().unwrap();
	let offer = client.create_lightning_offer("Empty pool test".into(), Some(30_000)).await.unwrap();
	tokio::time::sleep(Duration::from_secs(8)).await;
	ln.sync().await;
	let before = wallet.spendable_balance().await;
	let invoice = ln.external.grpc_client().await.fetch_invoice(cln_rpc::FetchinvoiceRequest {
		offer: offer.offer, ..Default::default()
	}).await.unwrap().into_inner().invoice;
	assert!(tokio::time::timeout(Duration::from_secs(90), ln.external.try_pay_bolt11(&invoice)).await.unwrap().is_err());
	client.sync_pending_lightning_receives().await.unwrap();
	assert!(client.pending_lightning_receives().await.unwrap().is_empty());
	assert_eq!(wallet.spendable_balance().await, before);
	client.stop_daemon_wait().await.unwrap();
}
