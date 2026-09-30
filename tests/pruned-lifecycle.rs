//! Exercise wallet and ASP through the same private pruned chain adapter.
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use ark_testing::{btc, sat, TestContext};
use ark_testing::util::FutureExt;
use bitcoincore_rpc::RpcApi;
use server::vtxopool::VtxoTarget;

struct Adapter(Child);
impl Drop for Adapter {
	fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

#[tokio::test]
async fn xbt_pruned_lifecycle() {
	let ctx = TestContext::new("xbt/pruned-lifecycle").await;
	let chain: serde_json::Value = ctx.bitcoind().sync_client().call("getblockchaininfo", &[]).unwrap();
	assert_eq!(chain["pruned"], true);
	let indexes: serde_json::Value = ctx.bitcoind().sync_client().call("getindexinfo", &[]).unwrap();
	assert!(indexes.get("txindex").is_none());
	let listener = TcpListener::bind("127.0.0.1:0").unwrap();
	let port = listener.local_addr().unwrap().port(); drop(listener);
	let url = format!("http://127.0.0.1:{port}");
	let cookie = ctx.bitcoind().rpc_cookie();
	let log = std::fs::File::create(ctx.datadir.join("pruned-adapter.log")).unwrap();
	let _adapter = Adapter(Command::new("python3")
		.arg(std::env::var("PAPERCLIP_PRUNED_ADAPTER").unwrap())
		.args(["--node-url", &ctx.bitcoind().rpc_url(), "--port", &port.to_string(), "--regtest", "--first-height", "0"])
		.arg("--node-cookie").arg(&cookie).arg("--client-cookie").arg(&cookie)
		.arg("--database").arg(ctx.datadir.join("chain-index.sqlite"))
		.stdin(Stdio::null()).stdout(log.try_clone().unwrap()).stderr(log).spawn().unwrap());
	let rpc = bitcoincore_rpc::Client::new(&url, bitcoincore_rpc::Auth::CookieFile(cookie.clone())).unwrap();
	tokio::time::timeout(Duration::from_secs(60), async {
		loop {
			let info: Result<serde_json::Value, _> = rpc.call("getpaperclipindexinfo", &[]);
			if info.is_ok_and(|v| v["synced"] == true) { break; }
			tokio::time::sleep(Duration::from_millis(100)).await;
		}
	}).await.unwrap();
	let ln = ctx.new_lightning_setup("pruned-ln").await;
	let asp_url = url.clone();
	let srv = ctx.captaind("asp").lightningd(&ln.internal).funded(btc(2)).cfg(move |c| {
		c.bitcoind.url = asp_url; c.bitcoind.cookie = Some(cookie);
		c.bitcoind.rpc_user = None; c.bitcoind.rpc_pass = None;
		c.experimental_funded_lightning = true;
		c.vtxopool.vtxo_targets = vec![VtxoTarget { amount: sat(200_000), count: 2 }];
	}).create().await;
	srv.wait_for_vtxopool(&ctx).await;
	let alice_url = url.clone();
	let alice = ctx.bark("alice", &srv).funded(sat(200_000)).cfg(move |c| {
		c.bitcoind_address = Some(alice_url); c.bitcoind_zmq_address = None;
	}).create().await;
	let bob = ctx.bark("bob", &srv).cfg(move |c| {
		c.bitcoind_address = Some(url); c.bitcoind_zmq_address = None;
	}).create().await;
	alice.board_and_confirm_and_register(&ctx, sat(100_000)).await;
	alice.send_oor(&bob.address().await, sat(20_000)).await;
	assert_eq!(bob.spendable_balance().await, sat(20_000));
	ln.sync().await;
	let invoice = ln.external.invoice(Some(sat(20_000)), "pruned-send", "Pruned backend BOLT11").await;
	alice.pay_lightning_wait(invoice, None).await;
	let offer = ln.external.offer(Some(sat(10_000)), Some("Pruned backend BOLT12")).await;
	alice.pay_lightning_wait(offer, None).await;
	let before = alice.spendable_balance().await;
	let incoming = alice.bolt11_invoice(sat(30_000)).await;
	tokio::join!(
		ln.external.pay_bolt11(&incoming.invoice),
		alice.lightning_receive(&incoming.invoice).wait_millis(60_000),
	);
	assert_eq!(alice.spendable_balance().await, before + sat(26_000));
	srv.stop().await.unwrap();
	bob.start_exit_all().await;
	ark_testing::exit::complete_exit(&ctx, &bob).await;
	bob.claim_all_exits(bob.get_onchain_address().await).await;
	ctx.generate_blocks(1).await;
	assert!(bob.onchain_balance().await > sat(18_000));
}
