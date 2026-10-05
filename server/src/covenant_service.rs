//! Opt-in test-network service experiment. Never uses the normal Ark database or keys.
//! The ASP funds authorized rounds; watchman reacts to observed stale exits.
//! Signed transactions are fsynced before broadcast and reused after restart.

use std::fs::{self, File, OpenOptions};
use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use anyhow::{ensure, Context};
use bitcoin::consensus::encode::{deserialize_hex, serialize_hex};
use bitcoin::secp256k1::{Keypair, Secp256k1, SecretKey};
use bitcoin::{Address, Amount, Network, OutPoint, Transaction, TxOut, Txid};
use bitcoin_ext::covenant::{Permit, State, PROFILE};
use bitcoin_ext::rpc::{self, Auth, Client, RpcApi};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role { Asp, Watchman }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
	experimental_test_only: bool,
	network: String,
	state_dir: PathBuf,
	rpc_url: String,
	cookie_file: PathBuf,
	funding_wallet: String,
	server_key_file: PathBuf,
	max_funding_sat: u64,
	#[serde(default = "poll_default")]
	poll_ms: u64,
	#[serde(default)]
	test_failpoint: Option<String>,
}
fn poll_default() -> u64 { 1000 }

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enrollment {
	pub funding: OutPoint,
	pub permits: Vec<Permit>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Step {
	funding_hex: String,
	unroll_hex: String,
	cost_sat: u64,
	funding_confirmations: i64,
	refund_hex: Option<String>,
	refund_confirmations: i64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Job {
	request: Enrollment,
	steps: Vec<Step>,
}

#[derive(Serialize, Deserialize)]
struct Journal {
	version: u32,
	profile: String,
	genesis: String,
	server: String,
	funding_wallet: String,
	jobs: Vec<Job>,
	fired_failpoints: Vec<String>,
}

struct Service {
	cfg: Config,
	rpc: Client,
	key: SecretKey,
	server: String,
	blocks: RefCell<Vec<(String, Vec<Txid>)>>,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
	let mut bytes = Vec::new();
	File::open(path)?.take(1_048_577).read_to_end(&mut bytes)?;
	ensure!(bytes.len() <= 1_048_576, "input exceeds laboratory size limit");
	Ok(serde_json::from_slice(&bytes)?)
}

fn private_file(path: &Path) -> anyhow::Result<File> {
	let mut opts = OpenOptions::new();
	opts.read(true).write(true).create(true);
	#[cfg(unix)] {
		use std::os::unix::fs::OpenOptionsExt;
		opts.mode(0o600);
	}
	Ok(opts.open(path)?)
}

fn atomic_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
	let bytes = serde_json::to_vec_pretty(value)?;
	if fs::read(path).ok().as_deref() == Some(bytes.as_slice()) { return Ok(()); }
	let tmp = path.with_extension("tmp");
	let mut f = private_file(&tmp)?;
	f.set_len(0)?;
	f.write_all(&bytes)?;
	f.sync_all()?;
	fs::rename(&tmp, path)?;
	File::open(path.parent().context("missing parent")?)?.sync_all()?;
	Ok(())
}

fn tx(raw: &str) -> anyhow::Result<Transaction> { Ok(deserialize_hex(raw)?) }
fn amount(value: &Value) -> anyhow::Result<Amount> {
	Ok(Amount::from_btc(value.as_f64().context("missing amount")?)?)
}

impl Service {
	fn open(path: &Path) -> anyhow::Result<Self> {
		let cfg: Config = read_json(path)?;
		ensure!(cfg.experimental_test_only && (cfg.network == "regtest" || cfg.network == "signet"), "requires explicit test-network opt-in");
		let uri: http::Uri = cfg.rpc_url.parse()?;
		ensure!(uri.scheme_str() == Some("http") && uri.host() == Some("127.0.0.1")
			&& uri.path() == "/" && uri.query().is_none(), "only local test-network RPC is supported");
		ensure!(cfg.funding_wallet.starts_with("covenant-test-") && cfg.funding_wallet.len() < 80
			&& cfg.funding_wallet.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'), "requires dedicated test wallet");
		ensure!((10_000..=10_000_000).contains(&cfg.max_funding_sat), "invalid laboratory budget");
		ensure!((100..=5000).contains(&cfg.poll_ms), "invalid polling interval");
		if let Some(point) = &cfg.test_failpoint {
			ensure!(point == "after-funding-journal" || point == "after-refund-journal", "unknown failpoint");
		}
		#[cfg(unix)] {
			use std::os::unix::fs::PermissionsExt;
			ensure!(fs::metadata(&cfg.server_key_file)?.permissions().mode() & 0o077 == 0, "server key must be owner-only");
		}
		let key = SecretKey::from_str(fs::read_to_string(&cfg.server_key_file)?.trim())?;
		let server = Keypair::from_secret_key(&Secp256k1::new(), &key).x_only_public_key().0.to_string();
		let rpc = Client::new(&format!("{}wallet/{}", cfg.rpc_url.trim_end_matches('/').to_owned()+"/", cfg.funding_wallet),
			Auth::CookieFile(cfg.cookie_file.clone()))?;
		let s = Self { cfg, rpc, key, server, blocks: RefCell::new(Vec::new()) };
		s.height()?;
		fs::create_dir_all(&s.cfg.state_dir)?;
		crate::fs_perms::harden(&s.cfg.state_dir, 0o700)?;
		Ok(s)
	}

	fn height(&self) -> anyhow::Result<u32> {
		let chain: Value = self.rpc.call("getblockchaininfo", &[])?;
		ensure!(chain["chain"] == self.cfg.network, "wrong test network");
		if self.cfg.network == "signet" {
			ensure!(chain["signet_challenge"] == bitcoin_ext::covenant::CHALLENGE, "wrong signet challenge");
		}
		ensure!(chain["initialblockdownload"] == false, "node is not ready");
		Ok(chain["blocks"].as_u64().context("missing height")?.try_into()?)
	}

	fn lock(&self) -> anyhow::Result<File> {
		let f = private_file(&self.cfg.state_dir.join("journal.lock"))?;
		f.lock()?;
		Ok(f)
	}

	fn load(&self) -> anyhow::Result<Journal> {
		let genesis: String = self.rpc.call("getblockhash", &[json!(0)])?;
		let path = self.cfg.state_dir.join("journal.json");
		let journal = if path.exists() { read_json(&path)? } else {
			Journal { version: 1, profile: PROFILE.into(), genesis: genesis.clone(), server: self.server.clone(),
				funding_wallet: self.cfg.funding_wallet.clone(), jobs: vec![], fired_failpoints: vec![] }
		};
		ensure!(journal.version == 1 && journal.profile == PROFILE && journal.genesis == genesis
			&& journal.server == self.server && journal.funding_wallet == self.cfg.funding_wallet, "journal identity mismatch");
		Ok(journal)
	}

	fn save(&self, journal: &Journal) -> anyhow::Result<()> {
		atomic_json(&self.cfg.state_dir.join("journal.json"), journal)
	}

	fn status(&self, id: Txid) -> anyhow::Result<Option<i64>> {
		// Bounded test-chain observer: no txindex, public explorer, or node restart.
		// Rebuild on process start; discard disconnected blocks before every lookup.
		let height = self.height()? as usize;
		ensure!(height < 10_000, "test observer height limit reached");
		let mut blocks = self.blocks.borrow_mut();
		while !blocks.is_empty() {
			let last = blocks.len()-1;
			if last <= height {
				let hash: String = self.rpc.call("getblockhash", &[json!(last)])?;
				if hash == blocks[last].0 { break; }
			}
			blocks.pop();
		}
		while blocks.len() <= height {
			let hash: String = self.rpc.call("getblockhash", &[json!(blocks.len())])?;
			let block: Value = self.rpc.call("getblock", &[json!(hash), json!(1)])?;
			let ids = serde_json::from_value(block["tx"].clone())?;
			blocks.push((hash, ids));
		}
		for (h, (_, ids)) in blocks.iter().enumerate().rev() {
			if ids.contains(&id) { return Ok(Some((height-h+1) as i64)); }
		}
		let value: Value = match self.rpc.call("getrawtransaction", &[json!(id), json!(true)]) {
			Ok(v) => v,
			Err(rpc::Error::JsonRpc(rpc::jsonrpc::Error::Rpc(e))) if e.code == -5 => return Ok(None),
			Err(e) => return Err(e.into()),
		};
		let n = value["confirmations"].as_i64().unwrap_or(0);
		Ok(if n < 0 { None } else { Some(n) })
	}

	fn unspent(&self, point: OutPoint, expected: &TxOut, confirmed: bool) -> anyhow::Result<bool> {
		let v: Value = self.rpc.call("gettxout", &[json!(point.txid), json!(point.vout), json!(true)])?;
		if v.is_null() { return Ok(false); }
		ensure!(amount(&v["value"])? == expected.value
			&& v["scriptPubKey"]["hex"] == expected.script_pubkey.to_hex_string(), "backing output does not match permit");
		Ok(!confirmed || v["confirmations"].as_u64().unwrap_or(0) >= 1)
	}

	fn relay(&self, raw: &str) -> anyhow::Result<()> {
		let id = tx(raw)?.compute_txid();
		if self.status(id)?.is_some() { return Ok(()); }
		let accepted: Value = self.rpc.call("testmempoolaccept", &[json!([raw])])?;
		ensure!(accepted[0]["allowed"] == true, "persisted transaction is not currently admissible");
		let sent: Txid = self.rpc.call("sendrawtransaction", &[json!(raw)])?;
		ensure!(sent == id, "broadcast identity mismatch");
		Ok(())
	}

	fn prepare_funding(&self, state: &State) -> anyhow::Result<Step> {
		let expected = state.funding_output()?;
		let address = Address::from_script(&expected.script_pubkey,
			if self.cfg.network == "signet" { Network::Signet } else { Network::Regtest })?;
		let raw: String = self.rpc.call("createrawtransaction", &[json!([]), json!({address.to_string(): expected.value.to_btc()})])?;
		// A dedicated wallet plus the inter-process journal lock prevents parallel
		// selection. No RPC send occurs until the signed result is on stable storage.
		let funded: Value = self.rpc.call("fundrawtransaction", &[json!(raw), json!({"fee_rate":1,"changePosition":1})])?;
		let fee = amount(&funded["fee"])?.to_sat();
		ensure!(fee <= 10_000, "funding fee exceeds test bound");
		let signed: Value = self.rpc.call("signrawtransactionwithwallet", &[funded["hex"].clone()])?;
		ensure!(signed["complete"] == true, "incomplete funding signature");
		let hex = signed["hex"].as_str().context("missing signed transaction")?.to_owned();
		let transaction = tx(&hex)?;
		ensure!(transaction.output.first() == Some(&expected) && transaction.output.len() <= 2, "funding output changed");
		let unroll = state.signed_unroll(OutPoint { txid: transaction.compute_txid(), vout: 0 })?;
		Ok(Step { funding_hex: hex, unroll_hex: serialize_hex(&unroll), cost_sat: expected.value.to_sat()+fee,
			funding_confirmations: 0, refund_hex: None, refund_confirmations: 0 })
	}

	fn failpoint(&self, journal: &mut Journal, point: &str) -> anyhow::Result<()> {
		if self.cfg.test_failpoint.as_deref() == Some(point) && !journal.fired_failpoints.iter().any(|p| p == point) {
			journal.fired_failpoints.push(point.into());
			self.save(journal)?;
			std::process::exit(86);
		}
		Ok(())
	}

	fn tick(&self, role: Role) -> anyhow::Result<()> {
		let _lock = self.lock()?;
		let height = self.height()?;
		let mut journal = self.load()?;
		for j in 0..journal.jobs.len() {
			let request = journal.jobs[j].request.clone();
			// Reconcile every prepared parent before preparing any further funding.
			for i in 0..journal.jobs[j].steps.len() {
				let raw = journal.jobs[j].steps[i].funding_hex.clone();
				if role == Role::Asp { self.relay(&raw)?; }
				journal.jobs[j].steps[i].funding_confirmations = self.status(tx(&raw)?.compute_txid())?.unwrap_or(-1);
				if let Some(raw) = &journal.jobs[j].steps[i].refund_hex {
					let n = self.status(tx(raw)?.compute_txid())?.unwrap_or(-1);
					journal.jobs[j].steps[i].refund_confirmations = n;
				}
			}
			self.save(&journal)?;
			match role {
				Role::Asp => {
					let index = journal.jobs[j].steps.len();
					if index >= request.permits.len() { continue; }
					let permit = &request.permits[index];
					if height < permit.not_before || height.saturating_add(3) >= permit.old.expiry { continue; }
					if journal.jobs[j].steps.iter().any(|s| s.funding_confirmations < 1) { continue; }
					let initial = &request.permits[0].old;
					let original = initial.signed_unroll(request.funding)?;
					// The original confirmed tree or its exact unroll must still exist.
					if index == 0 && !self.unspent(request.funding, &initial.funding_output()?, true)?
						&& !self.unspent(OutPoint { txid: original.compute_txid(), vout: 0 }, &initial.output()?, true)? { continue; }
					if index > 0 {
						let previous = &journal.jobs[j].steps[index-1];
						let root = OutPoint { txid: tx(&previous.funding_hex)?.compute_txid(), vout: 0 };
						let leaf_raw = previous.refund_hex.as_ref().unwrap_or(&previous.unroll_hex);
						let leaf = OutPoint { txid: tx(leaf_raw)?.compute_txid(), vout: 0 };
						if !self.unspent(root, &permit.old.funding_output()?, true)?
							&& !self.unspent(leaf, &permit.old.output()?, true)? { continue; }
					}
					let spent: u64 = journal.jobs.iter().flat_map(|j| &j.steps).map(|s| s.cost_sat).sum();
					if spent + permit.new.funding_output()?.value.to_sat() + 10_000 > self.cfg.max_funding_sat { continue; }
					let step = self.prepare_funding(&permit.new)?;
					journal.jobs[j].steps.push(step);
					self.save(&journal)?;
					self.failpoint(&mut journal, "after-funding-journal")?;
				},
				Role::Watchman => {
					for i in 0..journal.jobs[j].steps.len() {
						if journal.jobs[j].steps[i].funding_confirmations < 1 { break; }
						let permit = &request.permits[i];
						let old = if i == 0 {
							permit.old.signed_unroll(request.funding)?.compute_txid()
						} else if let Some(raw) = &journal.jobs[j].steps[i-1].refund_hex { tx(raw)?.compute_txid() }
						else { break; };
						if let Some(raw) = &journal.jobs[j].steps[i].refund_hex {
							// Parent replay is safe and deterministic after a reorg.
							if i == 0 { self.relay(&serialize_hex(&permit.old.signed_unroll(request.funding)?))?; }
							self.relay(&journal.jobs[j].steps[i].unroll_hex)?;
							self.relay(raw)?;
							continue;
						}
						if self.status(old)?.is_none() || height < permit.not_before { break; }
						let old_point = OutPoint { txid: old, vout: 0 };
						if !self.unspent(old_point, &permit.old.output()?, false)? { break; }
						let unroll_raw = &journal.jobs[j].steps[i].unroll_hex;
						self.relay(unroll_raw)?;
						let new_point = OutPoint { txid: tx(unroll_raw)?.compute_txid(), vout: 0 };
						ensure!(self.unspent(new_point, &permit.new.output()?, false)?, "replacement backing is unavailable");
						let refund = permit.refund(old_point, new_point, self.key)?;
						journal.jobs[j].steps[i].refund_hex = Some(serialize_hex(&refund));
						self.save(&journal)?;
						self.failpoint(&mut journal, "after-refund-journal")?;
						break;
					}
				},
			}
		}
		self.save(&journal)?;
		// Recovery contains public state and signed transactions, never owner or server keys.
		atomic_json(&self.cfg.state_dir.join("recovery.json"), &journal)?;
		Ok(())
	}
}

pub fn enroll(config: &Path, request: &Path) -> anyhow::Result<()> {
	let s = Service::open(config)?;
	let enrollment: Enrollment = read_json(request)?;
	let _lock = s.lock()?;
	let height = s.height()?;
	ensure!(!enrollment.permits.is_empty() && enrollment.permits.len() <= 8, "requires one to eight permits");
	ensure!(!enrollment.funding.is_null(), "missing backing");
	for (i,p) in enrollment.permits.iter().enumerate() {
		p.verify()?;
		ensure!(p.old.server.to_string() == s.server && p.new.server.to_string() == s.server, "wrong service key");
		if i > 0 { ensure!(enrollment.permits[i-1].new == p.old, "disconnected permit chain"); }
	}
	let mut journal = s.load()?;
	if let Some(existing) = journal.jobs.iter().find(|j| j.request.funding == enrollment.funding) {
		ensure!(serde_json::to_value(&existing.request)? == serde_json::to_value(&enrollment)?, "backing already enrolled with different permits");
		println!("Enrollment already recorded");
		return Ok(());
	}
	ensure!(journal.jobs.len() < 32, "laboratory enrollment limit reached");
	let old = &enrollment.permits[0].old;
	ensure!(height.saturating_add(12) < old.expiry, "insufficient enrollment reaction window");
	ensure!(s.unspent(enrollment.funding, &old.funding_output()?, true)?, "old backing must be confirmed and unspent");
	journal.jobs.push(Job { request: enrollment, steps: vec![] });
	s.save(&journal)?;
	println!("Enrollment recorded");
	Ok(())
}

pub fn run(config: &Path, role: Role) -> anyhow::Result<()> {
	let s = Service::open(config)?;
	// Surface corrupt state and wrong identity before entering the retry loop.
	{ let _lock = s.lock()?; s.load()?; }
	loop {
		if s.tick(role).is_err() {
			// Config, secrets and raw requests must not appear in daemon logs.
			eprintln!("Covenant service paused this tick: node or persisted-state check failed");
		}
		thread::sleep(Duration::from_millis(s.cfg.poll_ms));
	}
}
