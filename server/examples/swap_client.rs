//! Public-key fixture client for isolated regtest ASP integration. Never real funds.
use std::{env, fs, path::Path, str::FromStr};
use anyhow::{Context, ensure};
use bitcoin::{Address, Amount, Network, Transaction, OutPoint};
use bitcoin::hashes::{sha256, Hash};
use bitcoin::secp256k1::{Keypair, PublicKey, SecretKey};
use bitcoin::consensus::{deserialize, serialize};
use bitcoin::hex::FromHex;
use bitcoin_ext::{BlockDelta, BlockHeight};
use ark::{ProtocolEncoding, Vtxo, VtxoPolicy, SECP};
use ark::vtxo::Full;
use ark::board::BoardBuilder;
use ark::arkoor::{ArkoorDestination, ArkoorPackageBuilder};
use ark::experimental_swap::{SwapContract, SettlementBuilder};
use server_rpc::{ArkServiceClient, RequestExt, protos};
use serde_json::{json, Value};

fn key(n: u8) -> Keypair { Keypair::from_secret_key(&SECP, &SecretKey::from_slice(&[n; 32]).expect("fixture key")) }
fn req<T>(v: T) -> tonic::Request<T> { let mut r = tonic::Request::new(v); r.set_pver(server_rpc::MAX_PROTOCOL_VERSION); r }
fn read_vtxo(path: &Path) -> anyhow::Result<Vtxo<Full>> { Ok(Vtxo::deserialize_hex(fs::read_to_string(path)?.trim())?) }
fn write_vtxo(path: &Path, v: &Vtxo<Full>) -> anyhow::Result<()> { fs::write(path, v.serialize_hex())?; Ok(()) }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let args: Vec<String> = env::args().collect();
	ensure!(args.len() >= 5 && args[1] == "PUBLIC-REGTEST-KEYS", "fixture acknowledgement required");
	let action = &args[2]; let url = &args[3]; let dir = Path::new(&args[4]);
	ensure!(url.starts_with("http://127.0.0.1:"), "fixture only connects to loopback");
	let mut client = ArkServiceClient::connect(url.clone()).await?;
	let info: ark::ArkInfo = client.get_ark_info(req(protos::Empty {})).await?.into_inner().try_into()?;
	ensure!(info.network == Network::Regtest, "not regtest");
	fs::create_dir_all(dir)?;
	if action == "init" {
		let id: u8 = args[5].parse()?; let expiry: u32 = args[6].parse()?;
		let builder = BoardBuilder::new(key(id).public_key(), BlockHeight::new(expiry), info.server_pubkey, info.vtxo_exit_delta);
		let address = Address::from_script(&builder.funding_script_pubkey(), Network::Regtest)?;
		fs::write(dir.join("config.json"), json!({"key":id,"expiry":expiry,"server":info.server_pubkey.to_string(),
			"delta":info.vtxo_exit_delta.to_u16()}).to_string())?;
		println!("{}", json!({"address":address.to_string()})); return Ok(());
	}
	let cfg: Value = serde_json::from_slice(&fs::read(dir.join("config.json"))?)?;
	let owner = key(cfg["key"].as_u64().context("key")? as u8);
	ensure!(info.server_pubkey == PublicKey::from_str(cfg["server"].as_str().context("server")?)?, "server identity changed");
	let funding: Transaction = deserialize(&Vec::<u8>::from_hex(fs::read_to_string(dir.join("funding.hex"))?.trim())?)?;
	if action == "board" {
		let budget = ark::exit_policy::paperclip_funding();
		let initial = BoardBuilder::new(owner.public_key(), BlockHeight::new(cfg["expiry"].as_u64().unwrap() as u32),
			info.server_pubkey, BlockDelta::new(cfg["delta"].as_u64().unwrap() as u16)).with_exit_format(budget.format());
		let (index, output) = funding.output.iter().enumerate().find(|(_,o)| o.script_pubkey == initial.funding_script_pubkey()).context("funding output")?;
		let point = OutPoint::new(funding.compute_txid(), index as u32);
		let builder = initial.set_funded_funding_details(output.value, budget.anchor(), budget.miner_fee(), point)?.generate_user_nonces();
		let response = client.request_board_cosign(req(protos::BoardCosignRequest {
			amount: output.value.to_sat(), utxo: serialize(&point), expiry_height: cfg["expiry"].as_u64().unwrap() as u32,
			user_pubkey: owner.public_key().serialize().to_vec(), pub_nonce: builder.user_pub_nonce().serialize().to_vec(),
			funding_tx: serialize(&funding), exit_profile: ark::exit_policy::PAPERCLIP_EXIT_PROFILE,
		})).await?.into_inner().try_into()?;
		let vtxo = builder.build_vtxo(&response, &owner)?; vtxo.validate(&funding)?;
		client.register_board_vtxo(req(protos::BoardVtxoRequest { board_vtxo: vtxo.serialize() })).await?;
		write_vtxo(&dir.join("input.hex"), &vtxo)?;
		println!("{}", json!({"board_sats":vtxo.amount().to_sat()})); return Ok(());
	}
	if action == "lock" {
		let input = read_vtxo(&dir.join("input.hex"))?;
		let claimant = key(args[5].parse()?).public_key(); let deadline: u32 = args[6].parse()?; let amount: u64 = args[7].parse()?;
		let contract = SwapContract::new(claimant, owner.public_key(), info.server_pubkey, sha256::Hash::hash(&[42;32]), deadline, input.exit_delta().to_u16())?;
		let (builder, fee) = ArkoorPackageBuilder::new_funded_swap(vec![input], ArkoorDestination {
			total_amount: Amount::from_sat(amount), policy: VtxoPolicy::ExperimentalSwap(contract),
		}, VtxoPolicy::new_pubkey(owner.public_key()))?;
		let user = builder.generate_user_nonces(&[owner])?;
		let request: protos::ArkoorPackageCosignRequest = user.cosign_request().into();
		// The server commits the exact spending transaction before releasing its signature.
		let response = client.experimental_swap_lock(req(request.clone())).await?.into_inner().try_into()?;
		let outputs = user.user_cosign(&[owner], response)?.build_signed_vtxos();
		for output in &outputs { output.validate(&funding)?; }
		client.register_vtxo_transactions(req(protos::RegisterVtxoTransactionsRequest { vtxos: outputs.iter().map(|v| v.serialize()).collect() })).await?;
		let locked = outputs.iter().find(|v| matches!(v.policy(), VtxoPolicy::ExperimentalSwap(_))).context("lock")?;
		write_vtxo(&dir.join("locked.hex"), locked)?;
		println!("{}", json!({"locked_sats":locked.amount().to_sat(),"lock_reserve":fee.to_sat(),"id":locked.id().to_string()})); return Ok(());
	}
	if action == "settle" || action == "refund" {
		let locked = read_vtxo(&dir.join("locked.hex"))?;
		let participant = key(args[5].parse()?); let recipient = key(args[6].parse()?).public_key();
		let refund = action == "refund"; let preimage = if refund { None } else { Some([42;32]) };
		let mut builder = SettlementBuilder::new(locked.clone(), recipient, refund, preimage).map_err(anyhow::Error::msg)?;
		builder.sign_participant(&participant).map_err(anyhow::Error::msg)?;
		let response = client.experimental_swap_settle(req(protos::ExperimentalSwapSettleRequest {
			input_id: locked.id().to_bytes().to_vec(), recipient: recipient.serialize().to_vec(), refund,
			preimage: preimage.map(|p| p.to_vec()), participant_signature: builder.participant_signature().unwrap().serialize().to_vec(),
		})).await?.into_inner();
		let output = Vtxo::<Full>::deserialize(&response.vtxo)?; output.validate(&funding)?;
		ensure!(output.point().txid == builder.transaction().compute_txid(), "unexpected settlement");
		write_vtxo(&dir.join("settled.hex"), &output)?;
		println!("{}", json!({"settled_sats":output.amount().to_sat(),"id":output.id().to_string()})); return Ok(());
	}
	if action == "spend" {
		let input = read_vtxo(&dir.join("settled.hex"))?; let signer = key(args[5].parse()?);
		let (builder, fee) = ArkoorPackageBuilder::new_funded_payment_with_funding(vec![input], ArkoorDestination {
			total_amount: Amount::from_sat(5000), policy: VtxoPolicy::new_pubkey(key(9).public_key()),
		}, VtxoPolicy::new_pubkey(signer.public_key()), ark::exit_policy::small_anchor_transfer_funding())?;
		let user = builder.generate_user_nonces(&[signer])?;
		let response = client.request_arkoor_cosign(req(user.cosign_request().into())).await?.into_inner().try_into()?;
		let outputs = user.user_cosign(&[signer], response)?.build_signed_vtxos();
		for output in &outputs { output.validate(&funding)?; }
		client.register_vtxo_transactions(req(protos::RegisterVtxoTransactionsRequest { vtxos: outputs.iter().map(|v| v.serialize()).collect() })).await?;
		let received = outputs.iter().find(|v| v.user_pubkey() == key(9).public_key()).context("recipient output")?;
		write_vtxo(&dir.join("received.hex"), received)?;
		fs::write(dir.join("recovery.json"), serde_json::to_vec(&received.transactions()
			.map(|v| bitcoin::consensus::encode::serialize_hex(&v.tx)).collect::<Vec<_>>())?)?;
		println!("{}", json!({"ordinary_payment_sats":5000,"reserve":fee.to_sat()})); return Ok(());
	}
	anyhow::bail!("unknown fixture action")
}
