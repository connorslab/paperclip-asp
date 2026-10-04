//! Regtest-only HTLC fixture with PUBLIC deterministic keys. Never fund on mainnet.
//! This tests a leaf contract, not an Ark VTXO or a complete cross-server swap.
use std::{env, error::Error, str::FromStr};
use bitcoin::{absolute, transaction, Address, Amount, Network, OutPoint, ScriptBuf,
	Sequence, Transaction, TxIn, TxOut, Witness};
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::hashes::{sha256, Hash};
use bitcoin::opcodes::all::*;
use bitcoin::script::Builder;
use bitcoin::secp256k1::{Keypair, Message, SecretKey, XOnlyPublicKey};
use bitcoin::taproot::{LeafVersion, TaprootBuilder};
use bitcoin_ext::unified;
use ark::SECP;
use ark::experimental_swap::{SwapContract, SwapPath};

fn main() -> Result<(), Box<dyn Error>> {
	let mut args: Vec<String> = env::args().collect();
	let candidate = args.get(1).map_or(false, |s| s.starts_with("candidate-"));
	if candidate { args[1] = args[1].trim_start_matches("candidate-").to_owned(); }
	if args.len() < 4 { return Err("usage: address HEIGHT source|destination | spend HEIGHT TXID VOUT SATS DEST MODE LOCKTIME PUBLIC-REGTEST-KEYS source|destination".into()); }
	let deadline: u32 = args[2].parse()?;
	if deadline == 0 || deadline >= 500_000_000 { return Err("height required".into()); }
	let csv: u16 = args.get(if args[1] == "address" { 4 } else { 11 })
		.map(|s| s.parse()).transpose()?.unwrap_or(0);
	let side = if args[1] == "address" { args.get(3) } else { args.get(10) }.ok_or("missing side")?;
	let (claim_id, refund_id) = match side.as_str() {
		"source" => (2, 1), // Alice -> provider
		"destination" => (3, 2), // provider -> Bob
		_ => return Err("invalid side".into()),
	};
	let claim = Keypair::from_secret_key(&SECP, &SecretKey::from_slice(&[claim_id; 32])?);
	let refund = Keypair::from_secret_key(&SECP, &SecretKey::from_slice(&[refund_id; 32])?);
	let secret = [42u8; 32];
	let hash = sha256::Hash::hash(&secret);
	let delayed = || if csv == 0 { Builder::new() } else {
		Builder::new().push_int(csv.into()).push_opcode(OP_CSV).push_opcode(OP_DROP)
	};
	let success_script = delayed()
		.push_opcode(OP_SIZE).push_int(32).push_opcode(OP_EQUALVERIFY)
		.push_opcode(OP_SHA256).push_slice(hash.to_byte_array()).push_opcode(OP_EQUALVERIFY)
		.push_x_only_key(&claim.x_only_public_key().0).push_opcode(OP_CHECKSIG).into_script();
	let refund_script = delayed().push_int(deadline.into()).push_opcode(OP_CLTV).push_opcode(OP_DROP)
		.push_x_only_key(&refund.x_only_public_key().0).push_opcode(OP_CHECKSIG)
		.into_script();
	// BIP341 NUMS internal key: no known key-path secret.
	let nums = XOnlyPublicKey::from_str("50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0")?;
	let mut tree = TaprootBuilder::new().add_leaf(1, success_script.clone())?
		.add_leaf(1, refund_script.clone())?
		.finalize(&SECP, nums).map_err(|_| "taproot tree")?;
	let server = Keypair::from_secret_key(&SECP, &SecretKey::from_slice(&[if side == "source" { 4 } else { 5 }; 32])?);
	let contract = if candidate {
		let contract = SwapContract::new(claim.public_key(), refund.public_key(), server.public_key(), hash, deadline, csv)?;
		tree = contract.taproot()?;
		Some(contract)
	} else { None };
	let address = Address::p2tr_tweaked(tree.output_key(), Network::Regtest);
	if args[1] == "address" { println!("{address}"); return Ok(()); }
	if args[1] != "spend" || !(11..=12).contains(&args.len()) { return Err("invalid fixture command".into()); }
	let amount: u64 = args[5].parse()?;
	let output = Address::from_str(&args[6])?.require_network(Network::Regtest)?;
	let mode = args[7].as_str();
	if !matches!(mode, "success" | "refund" | "wrong-secret" | "wrong-key" | "wrong-server" | "recover-claim" | "recover-refund") { return Err("invalid branch".into()); }
	let is_refund = matches!(mode, "refund" | "recover-refund");
	let recovery = matches!(mode, "recover-claim" | "recover-refund");
	let script = if let Some(contract) = &contract {
		contract.script(match (is_refund, recovery) {
			(false, false) => SwapPath::Claim, (true, false) => SwapPath::Refund,
			(false, true) => SwapPath::RecoverClaim, (true, true) => SwapPath::RecoverRefund,
		})
	} else if is_refund { refund_script } else { success_script };
	let locktime: u32 = args[8].parse()?;
	// Last argument makes the deliberate use of publicly known keys explicit.
	if args[9] != "PUBLIC-REGTEST-KEYS" { return Err("regtest acknowledgement required".into()); }
	let prevout = TxOut { value: Amount::from_sat(amount), script_pubkey: address.script_pubkey() };
	let mut tx = Transaction {
		version: transaction::Version::TWO, lock_time: absolute::LockTime::from_height(locktime)?,
		input: vec![TxIn { previous_output: OutPoint { txid: args[3].parse()?, vout: args[4].parse()? },
			script_sig: ScriptBuf::new(), sequence: if csv == 0 || (candidate && !recovery) { Sequence::ENABLE_LOCKTIME_NO_RBF }
				else { Sequence::from_height(csv) }, witness: Witness::new() }],
		output: vec![TxOut { value: Amount::from_sat(amount.checked_sub(1000).ok_or("amount too small")?),
			script_pubkey: output.script_pubkey() }],
	};
	let leaf = bitcoin::TapLeafHash::from_script(&script, LeafVersion::TapScript);
	let digest = unified::digest(&tx, 0, &[prevout], unified::ALL, unified::Execution {
		script_type: 3, script_code: None, annex: None, leaf: Some((leaf, u32::MAX)),
	})?;
	let key = if is_refund || mode == "wrong-key" { &refund } else { &claim };
	let sig = SECP.sign_schnorr_no_aux_rand(&Message::from_digest(digest.to_byte_array()), key);
	let witness = &mut tx.input[0].witness;
	if candidate && !recovery {
		let server_key = if mode == "wrong-server" { &refund } else { &server };
		let sig = SECP.sign_schnorr_no_aux_rand(&Message::from_digest(digest.to_byte_array()), server_key);
		witness.push(unified::signature(&sig));
	}
	witness.push(unified::signature(&sig));
	if !is_refund {
		witness.push(if mode == "wrong-secret" { [43u8; 32] } else { secret });
	}
	witness.push(script.as_bytes());
	witness.push(tree.control_block(&(script, LeafVersion::TapScript)).ok_or("missing control block")?.serialize());
	println!("{}", serialize_hex(&tx));
	Ok(())
}
