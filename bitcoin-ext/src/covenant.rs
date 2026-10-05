//! Experimental single-balance offline refreshes for Paperclip's covenant signet.
//! Not the production Ark wire protocol. No mainnet use or implicit migration.
//!
//! A refund consumes BOTH old and newly issued exit outputs, recreates the new
//! user's output, and returns the duplicate backing to the server. The user signs
//! TEMPLATEHASH with CSFS before either concrete input is known. The server signs
//! the actual inputs with SIGHASH_UNIFIED. The user always retains a timed claim.

use std::str::FromStr;

use bitcoin::absolute::LockTime;
use bitcoin::consensus::serialize;
use bitcoin::hashes::{sha256, Hash};
use bitcoin::opcodes::{all::*, Opcode};
use bitcoin::script::Builder;
use bitcoin::secp256k1::{schnorr::Signature, Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};
use bitcoin::taproot::{LeafVersion, TaprootBuilder, TaprootSpendInfo};
use bitcoin::{Amount, OutPoint, ScriptBuf, Sequence, TapLeafHash, Transaction, TxIn, TxOut, Witness};
use serde::{Deserialize, Serialize};

use crate::unified;

pub const PROFILE: &str = "paperclip-signet-offline-refresh-v1";
pub const CHALLENGE: &str = "2102396d38e3ff703be31a2d97317835f4e9645b5ae5ea2d2d1ba406afa7ab185b5fac";
const NUMS: &str = "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";
pub const TREE_FEE: u64 = 1000;
pub const REFUND_FEE: u64 = 1000;
pub const CLAIM_FEE: u64 = 1000;
const MAX_VALUE: u64 = 10_000_000;

#[derive(Debug, thiserror::Error)]
#[error("invalid experimental covenant request: {0}")]
pub struct Error(pub &'static str);
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
	pub owner: XOnlyPublicKey,
	pub server: XOnlyPublicKey,
	pub amount_sat: u64,
	pub expiry: u32,
	pub exit_delay: u16,
}

impl State {
	pub fn validate(&self) -> Result<()> {
		if !(10_000..=MAX_VALUE).contains(&self.amount_sat) { return Err(Error("amount outside lab limits")); }
		if !(1..500_000_000 - 1000).contains(&self.expiry) || !(6..=1000).contains(&self.exit_delay) {
			return Err(Error("invalid expiry or reaction delay"));
		}
		if self.owner == self.server { return Err(Error("owner and server must be different")); }
		Ok(())
	}

	pub fn refund_script(&self) -> ScriptBuf {
		Builder::new().push_opcode(Opcode::from(0xce)).push_x_only_key(&self.owner)
			.push_opcode(Opcode::from(0xcc)).push_opcode(OP_VERIFY)
			.push_x_only_key(&self.server).push_opcode(OP_CHECKSIG).into_script()
	}

	pub fn claim_script(&self) -> ScriptBuf {
		Builder::new().push_int(i64::from(self.expiry) + i64::from(self.exit_delay))
			.push_opcode(OP_CLTV).push_opcode(OP_DROP).push_x_only_key(&self.owner)
			.push_opcode(OP_CHECKSIG).into_script()
	}

	pub fn leaf(&self) -> Result<TaprootSpendInfo> {
		self.validate()?;
		tap_tree(self.refund_script(), self.claim_script())
	}

	pub fn output(&self) -> Result<TxOut> {
		Ok(TxOut { value: Amount::from_sat(self.amount_sat), script_pubkey: ScriptBuf::new_p2tr_tweaked(self.leaf()?.output_key()) })
	}

	pub fn unroll(&self, funding: OutPoint) -> Result<Transaction> {
		Ok(transaction(0, vec![funding], vec![self.output()?]))
	}

	pub fn tree_script(&self) -> Result<ScriptBuf> {
		let hash = template_hash(&self.unroll(OutPoint::null())?, 0)?;
		Ok(Builder::new().push_opcode(Opcode::from(0xce)).push_slice(hash)
			.push_opcode(OP_EQUAL).into_script())
	}

	pub fn tree(&self) -> Result<TaprootSpendInfo> {
		tap_tree(self.tree_script()?, Builder::new().push_int(i64::from(self.expiry))
			.push_opcode(OP_CLTV).push_opcode(OP_DROP).push_x_only_key(&self.server)
			.push_opcode(OP_CHECKSIG).into_script())
	}

	pub fn funding_output(&self) -> Result<TxOut> {
		self.validate()?;
		Ok(TxOut { value: Amount::from_sat(self.amount_sat + TREE_FEE), script_pubkey: ScriptBuf::new_p2tr_tweaked(self.tree()?.output_key()) })
	}

	pub fn signed_unroll(&self, funding: OutPoint) -> Result<Transaction> {
		if funding.is_null() { return Err(Error("missing round funding outpoint")); }
		let mut tx = self.unroll(funding)?;
		tx.input[0].witness = script_witness(vec![], &self.tree()?, self.tree_script()?)?;
		Ok(tx)
	}
}

// Keep all key-path spending unavailable: neither party knows the internal key.
fn tap_tree(a: ScriptBuf, b: ScriptBuf) -> Result<TaprootSpendInfo> {
	let nums = XOnlyPublicKey::from_str(NUMS).map_err(|_| Error("NUMS key"))?;
	TaprootBuilder::new().add_leaf(1, a).map_err(|_| Error("tap tree"))?
		.add_leaf(1, b).map_err(|_| Error("tap tree"))?
		.finalize(&Secp256k1::new(), nums).map_err(|_| Error("tap tree"))
}

fn transaction(height: u32, inputs: Vec<OutPoint>, output: Vec<TxOut>) -> Transaction {
	Transaction { version: bitcoin::transaction::Version::TWO,
		lock_time: LockTime::from_consensus(height), output,
		input: inputs.into_iter().map(|previous_output| TxIn {
			previous_output, script_sig: ScriptBuf::new(), sequence: Sequence(0xfffffffd),
			witness: Witness::new(),
		}).collect(),
	}
}

/// BIP446 as implemented by the pinned signet, with no annex or scriptSig.
/// Unlike UnifiedSighash, this deliberately omits outpoints and input amounts.
pub fn template_hash(tx: &Transaction, input: usize) -> Result<[u8; 32]> {
	if input >= tx.input.len() || tx.input.iter().any(|i| !i.script_sig.is_empty()) {
		return Err(Error("template context"));
	}
	let mut data = serialize(&tx.version);
	data.extend(serialize(&tx.lock_time));
	let seq: Vec<u8> = tx.input.iter().flat_map(|i| serialize(&i.sequence)).collect();
	data.extend(sha256::Hash::hash(&seq).to_byte_array());
	let outputs: Vec<u8> = tx.output.iter().flat_map(serialize).collect();
	data.extend(sha256::Hash::hash(&outputs).to_byte_array());
	data.push(0); // no annex
	data.extend(u32::try_from(input).map_err(|_| Error("input index"))?.to_le_bytes());
	let tag = sha256::Hash::hash(b"TemplateHash").to_byte_array();
	let mut message = tag.to_vec();
	message.extend(tag);
	message.extend(data);
	Ok(sha256::Hash::hash(&message).to_byte_array())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permit {
	pub profile: String,
	pub challenge: String,
	pub old: State,
	pub new: State,
	pub not_before: u32,
	pub signatures: [Signature; 2],
	/// Off-chain authorization binds metadata that TEMPLATEHASH deliberately omits.
	pub binding: Signature,
}

impl Permit {
	fn binding_hash(old: &State, new: &State, not_before: u32) -> [u8; 32] {
		let tag = sha256::Hash::hash(b"Paperclip/OfflineRefreshPermitV1").to_byte_array();
		let mut data = tag.to_vec();
		data.extend(tag);
		data.extend(PROFILE.as_bytes());
		data.extend(CHALLENGE.as_bytes());
		for state in [old, new] {
			data.extend(state.owner.serialize()); data.extend(state.server.serialize());
			data.extend(state.amount_sat.to_le_bytes()); data.extend(state.expiry.to_le_bytes());
			data.extend(state.exit_delay.to_le_bytes());
		}
		data.extend(not_before.to_le_bytes());
		sha256::Hash::hash(&data).to_byte_array()
	}

	pub fn unsigned(old: State, new: State, not_before: u32) -> Result<Transaction> {
		old.validate()?;
		new.validate()?;
		if old.server != new.server || old.owner == new.owner || old.exit_delay != new.exit_delay {
			return Err(Error("refresh requires a new owner key and the same server and delay"));
		}
		if new.expiry <= old.expiry || new.expiry - old.expiry > 100_000
			|| not_before == 0 || not_before >= old.expiry
			|| new.amount_sat > old.amount_sat || old.amount_sat - new.amount_sat > 1000 {
			return Err(Error("invalid refresh window or fee"));
		}
		// The server recovers duplicate backing, never the new user's allocation.
		let reimbursement = TxOut { value: Amount::from_sat(old.amount_sat - REFUND_FEE),
			script_pubkey: Builder::new().push_int(1).push_x_only_key(&old.server).into_script() };
		Ok(transaction(not_before, vec![OutPoint::null(), OutPoint::null()], vec![new.output()?, reimbursement]))
	}

	pub fn authorize(old: State, new: State, not_before: u32, secrets: [SecretKey; 2]) -> Result<Self> {
		let tx = Self::unsigned(old.clone(), new.clone(), not_before)?;
		let secp = Secp256k1::new();
		let keys = secrets.map(|s| Keypair::from_secret_key(&secp, &s));
		if keys[0].x_only_public_key().0 != old.owner || keys[1].x_only_public_key().0 != new.owner {
			return Err(Error("owner secret does not match policy"));
		}
		let signatures = [0, 1].map(|i| secp.sign_schnorr_no_aux_rand(
			&Message::from_digest(template_hash(&tx, i).expect("two inputs")), &keys[i]));
		let binding = secp.sign_schnorr_no_aux_rand(
			&Message::from_digest(Self::binding_hash(&old, &new, not_before)), &keys[0]);
		Ok(Self { profile: PROFILE.into(), challenge: CHALLENGE.into(), old, new, not_before, signatures, binding })
	}

	pub fn verify(&self) -> Result<()> {
		if self.profile != PROFILE || self.challenge != CHALLENGE { return Err(Error("wrong test network or profile")); }
		Secp256k1::verification_only().verify_schnorr(&self.binding,
			&Message::from_digest(Self::binding_hash(&self.old, &self.new, self.not_before)),
			&self.old.owner).map_err(|_| Error("invalid permit metadata authorization"))?;
		let tx = Self::unsigned(self.old.clone(), self.new.clone(), self.not_before)?;
		for (i, key) in [self.old.owner, self.new.owner].iter().enumerate() {
			Secp256k1::verification_only().verify_schnorr(&self.signatures[i],
				&Message::from_digest(template_hash(&tx, i)?), key).map_err(|_| Error("invalid offline authorization"))?;
		}
		Ok(())
	}

	pub fn refund(&self, old: OutPoint, new: OutPoint, server: SecretKey) -> Result<Transaction> {
		self.verify()?;
		if old == new || old.is_null() || new.is_null() { return Err(Error("refund requires two distinct real outpoints")); }
		let mut tx = Self::unsigned(self.old.clone(), self.new.clone(), self.not_before)?;
		tx.input[0].previous_output = old;
		tx.input[1].previous_output = new;
		let prevouts = [self.old.output()?, self.new.output()?];
		for (i, state) in [&self.old, &self.new].iter().enumerate() {
			let script = state.refund_script();
			let sig = unified_signature(&tx, i, &prevouts, &script, server, state.server)?;
			// CSFS consumes the top signature; CHECKSIG then consumes the server signature.
			tx.input[i].witness = script_witness(vec![sig, self.signatures[i].as_ref().to_vec()], &state.leaf()?, script)?;
		}
		Ok(tx)
	}
}

fn unified_signature(tx: &Transaction, index: usize, prevouts: &[TxOut], script: &ScriptBuf,
	secret: SecretKey, expected: XOnlyPublicKey) -> Result<Vec<u8>> {
	let secp = Secp256k1::new();
	let key = Keypair::from_secret_key(&secp, &secret);
	if key.x_only_public_key().0 != expected { return Err(Error("signing key mismatch")); }
	let hash = unified::digest(tx, index, prevouts, unified::ALL, unified::Execution {
		script_type: 3, script_code: None, annex: None,
		leaf: Some((TapLeafHash::from_script(script, LeafVersion::TapScript), u32::MAX)),
	}).map_err(|_| Error("unified sighash context"))?;
	Ok(unified::signature(&secp.sign_schnorr_no_aux_rand(&Message::from_digest(hash.to_byte_array()), &key)).to_vec())
}

fn script_witness(mut stack: Vec<Vec<u8>>, tap: &TaprootSpendInfo, script: ScriptBuf) -> Result<Witness> {
	let control = tap.control_block(&(script.clone(), LeafVersion::TapScript)).ok_or(Error("missing control block"))?;
	stack.push(script.into_bytes());
	stack.push(control.serialize());
	Ok(Witness::from_slice(&stack))
}

pub fn claim(state: &State, outpoint: OutPoint, destination: ScriptBuf, owner: SecretKey) -> Result<Transaction> {
	state.validate()?;
	if !destination.is_p2tr() && !destination.is_p2wpkh() { return Err(Error("claim destination must be P2TR or P2WPKH")); }
	if outpoint.is_null() { return Err(Error("missing exit outpoint")); }
	let mut tx = transaction(state.expiry + u32::from(state.exit_delay), vec![outpoint],
		vec![TxOut { value: Amount::from_sat(state.amount_sat - CLAIM_FEE), script_pubkey: destination }]);
	let script = state.claim_script();
	let sig = unified_signature(&tx, 0, &[state.output()?], &script, owner, state.owner)?;
	tx.input[0].witness = script_witness(vec![sig], &state.leaf()?, script)?;
	Ok(tx)
}

#[cfg(test)]
mod tests {
	use super::*;
	fn key(n: u8) -> SecretKey { SecretKey::from_slice(&[n;32]).unwrap() }
	fn state(n: u8, expiry: u32) -> State {
		let pk = |n| Keypair::from_secret_key(&Secp256k1::new(), &key(n)).x_only_public_key().0;
		State { owner: pk(n), server: pk(9), amount_sat: 100_000, expiry, exit_delay: 12 }
	}
	#[test]
	fn covenant_permit_rejects_changed_recipient_amount_time_network() {
		let p = Permit::authorize(state(1, 200), state(2, 300), 180, [key(1), key(2)]).unwrap();
		p.verify().unwrap();
		let mut q = p.clone(); q.new.owner = state(3,300).owner; assert!(q.verify().is_err());
		let mut q = p.clone(); q.new.amount_sat -= 1; assert!(q.verify().is_err());
		let mut q = p.clone(); q.not_before -= 1; assert!(q.verify().is_err());
		let mut q = p.clone(); q.challenge = "mainnet".into(); assert!(q.verify().is_err());
		let mut q = p.clone(); q.old.expiry -= 1; assert!(q.verify().is_err());
		let mut q = p; q.new.expiry -= 1; assert!(q.verify().is_err());
	}
	#[test]
	fn covenant_template_rebinds_outpoints_but_not_outputs_or_input_index() {
		let mut tx = Permit::unsigned(state(1,200), state(2,300), 180).unwrap();
		let h = template_hash(&tx,0).unwrap();
		tx.input[0].previous_output.vout = 123;
		assert_eq!(h, template_hash(&tx,0).unwrap());
		assert_ne!(h, template_hash(&tx,1).unwrap());
		tx.output[0].value -= Amount::from_sat(1);
		assert_ne!(h, template_hash(&tx,0).unwrap());
	}
	#[test]
	fn covenant_rejects_same_key_expiry_and_excessive_fees() {
		assert!(Permit::unsigned(state(1,200),state(1,300),180).is_err());
		assert!(Permit::unsigned(state(1,200),state(2,200),180).is_err());
		assert!(Permit::unsigned(state(1,200),state(2,300),200).is_err());
		let mut new = state(2,300); new.amount_sat -= 1001;
		assert!(Permit::unsigned(state(1,200),new,180).is_err());
		let mut invalid = state(1,200); invalid.amount_sat = u64::MAX;
		assert!(invalid.funding_output().is_err());
		assert!(state(1,200).signed_unroll(OutPoint::null()).is_err());
	}
}
