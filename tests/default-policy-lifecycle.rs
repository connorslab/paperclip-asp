//! Private XBT regtest only. No relay-policy exemptions or production endpoints.
use std::time::Duration;
use bitcoin::Amount;
use bitcoincore_rpc::RpcApi;
use ark_testing::{btc, sat, TestContext};
use ark_testing::constants::ROUND_CONFIRMATIONS;
use ark_testing::exit::complete_exit;

#[tokio::test]
async fn xbt_lifecycle() {
	let ctx = TestContext::new("xbt/funded_default_policy").await;
	let relay = ctx.new_bitcoind("independent-relay").await;
	for node in [ctx.bitcoind().sync_client(), relay.sync_client()] {
		let info: serde_json::Value = node.call("getmempoolinfo", &[]).unwrap();
		assert_eq!(info["truc_policy"], "accept");
		assert_eq!(info["minrelaytxfee"].as_f64(), Some(0.00001));
		assert_eq!(info["dustrelayfee"].as_f64(), Some(0.00003));
	}
	let srv = ctx.captaind("server").no_vtxo_pool().funded(btc(10)).create().await;
	let alice = ctx.bark("alice", &srv).funded(sat(1_000_000)).create().await;
	let bob = ctx.bark("bob", &srv).create().await;
	let carol = ctx.bark("carol", &srv).create().await;
	alice.board_and_confirm_and_register(&ctx, sat(500_000)).await;
	assert_eq!(alice.spendable_balance().await, sat(498_000));
	// The new recovery signature carries unified ALL, and changing its
	// sighash byte to legacy ALL is rejected while the funding input exists.
	let board_id = alice.vtxo_ids().await[0];
	let board = alice.raw_vtxo(board_id).await;
	let mut legacy = board.transactions().next().unwrap().tx;
	let mut witness = legacy.input[0].witness.to_vec();
	assert_eq!(witness[0].last(), Some(&0x21));
	*witness[0].last_mut().unwrap() = 0x01;
	legacy.input[0].witness = bitcoin::Witness::from_slice(&witness);
	let rejected = ctx.bitcoind().sync_client().test_mempool_accept(&[&legacy]).unwrap();
	assert!(!rejected[0].allowed);
	alice.send_oor(&bob.address().await, sat(100_000)).await;
	assert_eq!(alice.spendable_balance().await, sat(392_000));
	assert_eq!(bob.spendable_balance().await, sat(100_000));
	assert!(alice.try_send_oor(&carol.address().await, sat(329), true).await.is_err());
	assert_eq!(alice.spendable_balance().await, sat(392_000));
	// Two users now share ancestors in the same freshly funded round.
	ctx.refresh_all(&srv, &[&alice, &bob]).await;
	ctx.generate_blocks(ROUND_CONFIRMATIONS).await;
	bob.send_oor(&carol.address().await, sat(20_000)).await;
	assert_eq!(bob.spendable_balance().await, sat(74_000));
	assert_eq!(carol.spendable_balance().await, sat(20_000));
	// Cooperative withdrawal also remains available.
	bob.offboard_all(&bob.get_onchain_address().await).await;
	ctx.generate_blocks(1).await;
	assert_eq!(bob.spendable_balance().await, Amount::ZERO);
	assert!(bob.onchain_balance().await > sat(60_000));
	// Carol has never had an onchain UTXO to pay for a CPFP child.
	assert_eq!(carol.onchain_balance().await, Amount::ZERO);
	let carol_vtxo = carol.raw_vtxo(carol.vtxo_ids().await[0]).await;
	srv.stop().await.unwrap();
	carol.start_exit_all().await;
	carol.progress_exit().await;
	let parents = ctx.bitcoind().sync_client().get_raw_mempool().unwrap();
	assert!(!parents.is_empty(), "wallet did not autonomously broadcast funded parents");
	tokio::time::timeout(Duration::from_secs(15), async {
		loop {
			let relayed = relay.sync_client().get_raw_mempool().unwrap();
			if parents.iter().all(|txid| relayed.contains(txid)) { break; }
			tokio::time::sleep(Duration::from_millis(100)).await;
		}
	}).await.expect("independent default-policy relay did not accept parents");
	// Drop the backend's mempool and reopen the wallet through a new CLI call.
	ctx.bitcoind().restart_wiping_mempool().await;
	complete_exit(&ctx, &carol).await;
	// The old harness RPC enum predates Knots' `anchor` script type.
	let parent: serde_json::Value = ctx.bitcoind().sync_client().call("getrawtransaction",
		&[serde_json::json!(carol_vtxo.point().txid.to_string()), serde_json::json!(true)]).unwrap();
	let parent_block = parent["blockhash"].as_str().unwrap().parse().unwrap();
	ctx.bitcoind().sync_client().invalidate_block(&parent_block).unwrap();
	complete_exit(&ctx, &carol).await;
	carol.claim_all_exits(carol.get_onchain_address().await).await;
	ctx.generate_blocks(1).await;
	assert!(carol.onchain_balance().await > sat(18_000));
	assert!(carol.onchain_balance().await < sat(20_000));
	let before = alice.onchain_balance().await;
	alice.start_exit_all().await;
	complete_exit(&ctx, &alice).await;
	alice.claim_all_exits(alice.get_onchain_address().await).await;
	ctx.generate_blocks(1).await;
	assert!(alice.onchain_balance().await > before + sat(380_000));
	assert!(alice.onchain_balance().await < before + sat(392_000));
}

fn copy_wallet(source: &std::path::Path, destination: &std::path::Path) {
	std::fs::create_dir(destination).unwrap();
	for entry in std::fs::read_dir(source).unwrap() {
		let entry = entry.unwrap();
		let target = destination.join(entry.file_name());
		let kind = entry.file_type().unwrap();
		if kind.is_dir() { copy_wallet(&entry.path(), &target); }
		else {
			assert!(kind.is_file(), "unexpected non-file in private wallet backup");
			std::fs::copy(entry.path(), target).unwrap();
		}
	}
}

#[tokio::test]
async fn xbt_lifecycle_late_receipt_and_backup() {
	let ctx = TestContext::new("xbt/late_receipt_backup").await;
	let srv = ctx.captaind("server").no_vtxo_pool().funded(btc(10)).create().await;
	let alice = ctx.bark("alice", &srv).funded(sat(1_000_000)).create().await;
	let bob = ctx.bark("bob", &srv).create().await;
	alice.board_and_confirm_and_register(&ctx, sat(500_000)).await;
	let board = alice.raw_vtxo(alice.vtxo_ids().await[0]).await;
	alice.send_oor(&bob.address().await, sat(20_000)).await;
	let target = board.expiry_height().to_u32() - u32::from(board.exit_delta().to_u16()) - 10;
	let height = ctx.bitcoind().sync_client().get_block_count().unwrap() as u32;
	ctx.generate_blocks(target - height).await;
	// Late receipt is not silently discarded or advertised as safely spendable.
	bob.sync().await;
	assert_eq!(bob.spendable_balance_no_sync().await, Amount::ZERO);
	let retained = bob.vtxos_no_sync().await;
	assert_eq!(retained.len(), 1);
	assert_eq!(bob.raw_vtxo(retained[0].id).await.amount(), sat(20_000));
	let backup = bob.datadir().with_extension("complete-wallet-backup");
	std::fs::rename(bob.datadir(), &backup).unwrap();
	copy_wallet(&backup, bob.datadir());
	srv.stop().await.unwrap();
	complete_exit(&ctx, &bob).await;
	bob.claim_all_exits(bob.get_onchain_address().await).await;
	ctx.generate_blocks(1).await;
	assert!(bob.onchain_balance().await > sat(18_000));
}
