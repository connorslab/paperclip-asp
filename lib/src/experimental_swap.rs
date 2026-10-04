//! Experimental swap policy and settlement, used only by opt-in regtest APIs.
//!
//! Cooperative spends require the participant AND the ASP. Recovery spends
//! require only the participant after CSV. No participant/ASP key-path bypass
//! is available. Refunds always require CLTV, including cooperative refunds.

use std::{io, str::FromStr};

use bitcoin::{ScriptBuf, taproot, Amount, Transaction, TxOut, Witness};
use bitcoin::hashes::{sha256, Hash};
use bitcoin::opcodes::all::*;
use bitcoin::script::Builder;
use bitcoin::secp256k1::{PublicKey, XOnlyPublicKey, schnorr, Message, Keypair};

use crate::SECP;
use crate::vtxo::policy::{check_block_delta, check_block_height};
use crate::vtxo::policy::{Policy, VtxoPolicyKind};
use crate::vtxo::policy::clause::{HashDelaySignClause, DelayedTimelockSignClause, VtxoClause, TapScriptClause};
use bitcoin_ext::{BlockDelta, BlockHeight};
use bitcoin_ext::unified;
use crate::{ProtocolEncoding, ProtocolDecodingError, ReadExt, WriteExt};
use crate::{Vtxo, VtxoPolicy};
use crate::vtxo::Full;
use crate::vtxo::genesis::{GenesisItem, GenesisTransition};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SwapError {
	#[error("invalid swap height or recovery delay")]
	Timing,
	#[error("swap participants and server must have distinct keys")]
	Keys,
	#[error("invalid swap taproot tree")]
	Tree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapPath { Claim, Refund, RecoverClaim, RecoverRefund }

/// Version-one experimental contract; existing production policy tags are unchanged.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SwapContract {
	claimant: PublicKey,
	refund: PublicKey,
	server: PublicKey,
	hash: sha256::Hash,
	deadline: u32,
	delay: u16,
}

impl SwapContract {
	pub fn claimant(&self) -> PublicKey { self.claimant }
	pub fn refund(&self) -> PublicKey { self.refund }
	pub fn server(&self) -> PublicKey { self.server }
	pub fn deadline(&self) -> u32 { self.deadline }
	pub fn delay(&self) -> u16 { self.delay }
	pub fn hash(&self) -> sha256::Hash { self.hash }
	pub fn check_context(&self, server: PublicKey, delta: BlockDelta, expiry: BlockHeight) -> Result<(), &'static str> {
		if server != self.server || delta.to_u16() != self.delay { return Err("swap context mismatch"); }
		if self.deadline >= expiry.to_u32() { return Err("swap refund after VTXO expiry"); }
		Ok(())
	}
	pub fn new(claimant: PublicKey, refund: PublicKey, server: PublicKey,
		hash: sha256::Hash, deadline: u32, delay: u16,
	) -> Result<Self, SwapError> {
		check_block_height(deadline).map_err(|_| SwapError::Timing)?;
		check_block_delta(delay).map_err(|_| SwapError::Timing)?;
		if deadline == 0 || delay == 0 { return Err(SwapError::Timing); }
		let keys = [claimant.x_only_public_key().0, refund.x_only_public_key().0,
			server.x_only_public_key().0];
		if keys[0] == keys[1] || keys[0] == keys[2] || keys[1] == keys[2] {
			return Err(SwapError::Keys);
		}
		Ok(Self { claimant, refund, server, hash, deadline, delay })
	}

	pub fn script(&self, path: SwapPath) -> ScriptBuf {
		if path == SwapPath::RecoverClaim { return self.claim_recovery().tapscript(); }
		if path == SwapPath::RecoverRefund { return self.refund_recovery().tapscript(); }
		let refund = path == SwapPath::Refund;
		let mut script = Builder::new();
		if refund {
			script = script.push_int(self.deadline.into()).push_opcode(OP_CLTV).push_opcode(OP_DROP);
		} else {
			script = script.push_opcode(OP_SIZE).push_int(32).push_opcode(OP_EQUALVERIFY)
				.push_opcode(OP_SHA256).push_slice(self.hash.to_byte_array()).push_opcode(OP_EQUALVERIFY);
		}
		let owner = if refund { self.refund } else { self.claimant };
		script = script.push_x_only_key(&owner.x_only_public_key().0);
		script.push_opcode(OP_CHECKSIGVERIFY).push_x_only_key(&self.server.x_only_public_key().0)
			.push_opcode(OP_CHECKSIG).into_script()
	}

	fn claim_recovery(&self) -> HashDelaySignClause {
		HashDelaySignClause { pubkey: self.claimant, hash: self.hash, block_delta: BlockDelta::new(self.delay) }
	}

	fn refund_recovery(&self) -> DelayedTimelockSignClause {
		DelayedTimelockSignClause { pubkey: self.refund, timelock_height: BlockHeight::new(self.deadline),
			block_delta: BlockDelta::new(self.delay) }
	}

	pub fn taproot(&self) -> Result<taproot::TaprootSpendInfo, SwapError> {
		// BIP341 NUMS point: no known secret for an unconditional key-path spend.
		let nums = XOnlyPublicKey::from_str(
			"50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0")
			.map_err(|_| SwapError::Tree)?;
		let mut tree = taproot::TaprootBuilder::new();
		for path in [SwapPath::Claim, SwapPath::Refund, SwapPath::RecoverClaim, SwapPath::RecoverRefund] {
			tree = tree.add_leaf(2, self.script(path)).map_err(|_| SwapError::Tree)?;
		}
		tree.finalize(&SECP, nums).map_err(|_| SwapError::Tree)
	}
}

impl ProtocolEncoding for SwapContract {
	fn encode<W: io::Write + ?Sized>(&self, w: &mut W) -> Result<(), io::Error> {
		w.emit_u8(1)?;
		self.claimant.encode(w)?; self.refund.encode(w)?; self.server.encode(w)?;
		self.hash.encode(w)?; w.emit_u32(self.deadline)?; w.emit_u16(self.delay)
	}
	fn decode<R: io::Read + ?Sized>(r: &mut R) -> Result<Self, ProtocolDecodingError> {
		if r.read_u8()? != 1 { return Err(ProtocolDecodingError::invalid("unknown swap version")); }
		Self::new(PublicKey::decode(r)?, PublicKey::decode(r)?, PublicKey::decode(r)?,
			sha256::Hash::decode(r)?, r.read_u32()?, r.read_u16()?)
			.map_err(|e| ProtocolDecodingError::invalid_err(e, "swap contract"))
	}
}

/// Cooperative claim/refund proof carried in the recipient's VTXO ancestry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapTransition {
	pub contract: SwapContract,
	pub refund: bool,
	pub preimage: Option<[u8; 32]>,
	pub participant_sig: Option<schnorr::Signature>,
	pub server_sig: Option<schnorr::Signature>,
}

impl SwapTransition {
	pub fn path(&self) -> SwapPath { if self.refund { SwapPath::Refund } else { SwapPath::Claim } }
	pub fn input_txout(&self, amount: Amount) -> TxOut {
		TxOut { value: amount, script_pubkey: ScriptBuf::new_p2tr_tweaked(
			self.contract.taproot().expect("fixed validated tree").output_key()) }
	}
	pub fn witness(&self) -> Witness {
		let (Some(participant), Some(server)) = (self.participant_sig, self.server_sig) else { return Witness::new(); };
		if !self.refund && self.preimage.is_none() { return Witness::new(); }
		let script = self.contract.script(self.path());
		let tree = self.contract.taproot().expect("fixed validated tree");
		let mut witness = Witness::new();
		witness.push(unified::signature(&server)); witness.push(unified::signature(&participant));
		if !self.refund { if let Some(p) = self.preimage { witness.push(p); } }
		witness.push(script.as_bytes());
		witness.push(tree.control_block(&(script, taproot::LeafVersion::TapScript)).expect("known leaf").serialize());
		witness
	}
	pub fn digest(&self, tx: &Transaction, prevout: &TxOut) -> Result<Message, &'static str> {
		if tx.input.len() != 1 { return Err("swap must have one input"); }
		let script = self.contract.script(self.path());
		let hash = unified::digest(tx, 0, &[prevout.clone()], unified::ALL, unified::Execution {
			script_type: 3, script_code: None, annex: None,
			leaf: Some((bitcoin::TapLeafHash::from_script(&script, taproot::LeafVersion::TapScript), u32::MAX)),
		}).map_err(|_| "invalid swap sighash")?;
		Ok(Message::from_digest(hash.to_byte_array()))
	}
	pub fn validate_sigs(&self, tx: &Transaction, prevout: &TxOut,
		server: PublicKey, delta: BlockDelta, expiry: BlockHeight,
	) -> Result<(), &'static str> {
		self.contract.check_context(server, delta, expiry)?;
		if self.input_txout(prevout.value) != *prevout { return Err("swap previous output mismatch"); }
		if self.refund {
			if self.preimage.is_some() || tx.lock_time.to_consensus_u32() != self.contract.deadline {
				return Err("invalid swap refund");
			}
		} else if self.preimage.map(|p| sha256::Hash::hash(&p)) != Some(self.contract.hash) {
			return Err("incorrect swap secret");
		}
		let msg = self.digest(tx, prevout)?;
		let owner = if self.refund { self.contract.refund } else { self.contract.claimant };
		SECP.verify_schnorr(&self.participant_sig.ok_or("missing participant signature")?, &msg,
			&owner.x_only_public_key().0).map_err(|_| "invalid participant signature")?;
		SECP.verify_schnorr(&self.server_sig.ok_or("missing server signature")?, &msg,
			&server.x_only_public_key().0).map_err(|_| "invalid server signature")
	}
}

impl ProtocolEncoding for SwapTransition {
	fn encode<W: io::Write + ?Sized>(&self, w: &mut W) -> Result<(), io::Error> {
		self.contract.encode(w)?; w.emit_u8(u8::from(self.refund))?;
		match self.preimage { Some(p) => { w.emit_u8(1)?; w.emit_slice(&p)?; }, None => w.emit_u8(0)? }
		self.participant_sig.encode(w)?; self.server_sig.encode(w)
	}
	fn decode<R: io::Read + ?Sized>(r: &mut R) -> Result<Self, ProtocolDecodingError> {
		let contract = SwapContract::decode(r)?;
		let refund = match r.read_u8()? { 0 => false, 1 => true, _ => return Err(ProtocolDecodingError::invalid("swap path")) };
		let preimage = match r.read_u8()? { 0 => None, 1 => Some(r.read_byte_array()?), _ => return Err(ProtocolDecodingError::invalid("swap secret tag")) };
		if refund && preimage.is_some() { return Err(ProtocolDecodingError::invalid("refund with secret")); }
		Ok(Self { contract, refund, preimage, participant_sig: Option::decode(r)?, server_sig: Option::decode(r)? })
	}
}

/// Deterministic claim-all transaction, with one funded recovery step.
/// Signatures authorize the exact destination and amount. Admission/spend state
/// and current chain height must still be enforced by the hosting ASP.
pub struct SettlementBuilder {
	input: Vtxo<Full>,
	transition: SwapTransition,
	recipient: PublicKey,
}

impl SettlementBuilder {
	pub fn new(input: Vtxo<Full>, recipient: PublicKey, refund: bool, preimage: Option<[u8; 32]>) -> Result<Self, &'static str> {
		let contract = match input.policy() {
			VtxoPolicy::ExperimentalSwap(p) => p.clone(), _ => return Err("not a swap VTXO"),
		};
		contract.check_context(input.server_pubkey(), input.exit_delta(), input.expiry_height())?;
		if !input.has_funded_exit() { return Err("unfunded swap ancestry"); }
		let funding = crate::exit_policy::small_anchor_transfer_funding();
		if input.amount() < funding.per_transaction() + bitcoin_ext::P2TR_DUST + crate::exit_policy::paperclip_policy().claim_fee {
			return Err("swap cannot fund settlement and recovery");
		}
		if refund && preimage.is_some() { return Err("refund with secret"); }
		if !refund && preimage.map(|p| sha256::Hash::hash(&p)) != Some(contract.hash) { return Err("incorrect swap secret"); }
		Ok(Self { input, recipient, transition: SwapTransition {
			contract, refund, preimage, participant_sig: None, server_sig: None,
		} })
	}
	pub fn input(&self) -> &Vtxo<Full> { &self.input }
	pub fn participant_signature(&self) -> Option<schnorr::Signature> { self.transition.participant_sig }
	fn owner(&self) -> PublicKey {
		if self.transition.refund { self.transition.contract.refund } else { self.transition.contract.claimant }
	}
	fn step(&self) -> GenesisItem {
		let funding = crate::exit_policy::small_anchor_transfer_funding();
		GenesisItem { exit_format: funding.format(), miner_fee: funding.miner_fee(), fee_amount: funding.anchor(),
			transition: GenesisTransition::ExperimentalSwap(self.transition.clone()), output_idx: 0, other_outputs: vec![] }
	}
	pub fn transaction(&self) -> Transaction {
		let policy = VtxoPolicy::new_pubkey(self.recipient);
		let amount = self.input.amount() - crate::exit_policy::small_anchor_transfer_funding().per_transaction();
		let output = Policy::txout(&policy, amount, self.input.server_pubkey(), self.input.exit_delta(), self.input.expiry_height());
		self.step().tx(self.input.point(), output, self.input.server_pubkey(), self.input.expiry_height())
	}
	pub fn sign_participant(&mut self, key: &Keypair) -> Result<(), &'static str> {
		if key.public_key() != self.owner() { return Err("wrong swap participant"); }
		let msg = self.transition.digest(&self.transaction(), &self.input.txout())?;
		self.transition.participant_sig = Some(SECP.sign_schnorr_no_aux_rand(&msg, key)); Ok(())
	}
	pub fn set_participant_signature(&mut self, sig: schnorr::Signature) -> Result<(), &'static str> {
		let msg = self.transition.digest(&self.transaction(), &self.input.txout())?;
		SECP.verify_schnorr(&sig, &msg, &self.owner().x_only_public_key().0).map_err(|_| "invalid swap participant signature")?;
		self.transition.participant_sig = Some(sig); Ok(())
	}
	pub fn sign_server(&mut self, key: &Keypair) -> Result<(), &'static str> {
		if key.public_key() != self.input.server_pubkey() { return Err("wrong swap server"); }
		self.set_participant_signature(self.transition.participant_sig.ok_or("missing participant signature")?)?;
		let msg = self.transition.digest(&self.transaction(), &self.input.txout())?;
		self.transition.server_sig = Some(SECP.sign_schnorr_no_aux_rand(&msg, key)); Ok(())
	}
	pub fn finish(self) -> Result<Vtxo<Full>, &'static str> {
		let tx = self.transaction();
		self.transition.validate_sigs(&tx, &self.input.txout(), self.input.server_pubkey(),
			self.input.exit_delta(), self.input.expiry_height())?;
		let mut genesis = self.input.genesis.clone(); genesis.items.push(self.step());
		Ok(Vtxo { policy: VtxoPolicy::new_pubkey(self.recipient), amount: tx.output[0].value,
			server_pubkey: self.input.server_pubkey(), expiry_height: self.input.expiry_height(),
			exit_delta: self.input.exit_delta(), anchor_point: self.input.chain_anchor(),
			point: bitcoin::OutPoint::new(tx.compute_txid(), 0), genesis })
	}
}

impl Policy for SwapContract {
	fn validate_context(&self, server: PublicKey, delta: BlockDelta, expiry: BlockHeight) -> Result<(), &'static str> {
		self.check_context(server, delta, expiry)
	}
	fn policy_type(&self) -> VtxoPolicyKind { VtxoPolicyKind::ExperimentalSwap }
	fn taproot(&self, _server: PublicKey, _delta: BlockDelta, _expiry: BlockHeight) -> taproot::TaprootSpendInfo {
		// Fixed-depth tree and constant valid NUMS key; parameters validated at construction.
		SwapContract::taproot(self).expect("validated fixed swap tree")
	}
	fn clauses(&self, _delta: BlockDelta, _expiry: BlockHeight, _server: PublicKey) -> Vec<VtxoClause> {
		vec![self.claim_recovery().into(), self.refund_recovery().into()]
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bitcoin::secp256k1::{Keypair, SecretKey};

	fn key(n: u8) -> Keypair {
		Keypair::from_secret_key(&SECP, &SecretKey::from_slice(&[n; 32]).unwrap())
	}
	fn alice_keypair() -> Keypair { key(1) }
	fn bob_keypair() -> Keypair { key(2) }
	fn server_keypair() -> Keypair { key(3) }

	#[test]
	fn swap_candidate_validates_keys_and_timing() {
		let alice = alice_keypair().public_key();
		let bob = bob_keypair().public_key();
		let server = server_keypair().public_key();
		let hash = sha256::Hash::hash(&[42; 32]);
		assert!(SwapContract::new(bob, alice, server, hash, 500, 3).is_ok());
		for (height, delay) in [(0, 3), (500_000_000, 3), (500, 0), (500, u16::MAX)] {
			assert_eq!(SwapContract::new(bob, alice, server, hash, height, delay).unwrap_err(), SwapError::Timing);
		}
		assert_eq!(SwapContract::new(alice, alice, server, hash, 500, 3).unwrap_err(), SwapError::Keys);
		// Opposite compressed-key parity still represents the same BIP340 signer.
		assert_eq!(SwapContract::new(alice.negate(&SECP), alice, server, hash, 500, 3).unwrap_err(), SwapError::Keys);
		assert_eq!(SwapContract::new(bob, alice, bob, hash, 500, 3).unwrap_err(), SwapError::Keys);
	}

	#[test]
	fn swap_encoding_rejects_unknown_versions_and_truncation() {
		let p = SwapContract::new(bob_keypair().public_key(), alice_keypair().public_key(), server_keypair().public_key(),
			sha256::Hash::hash(&[42;32]), 500, 3).unwrap();
		let bytes = p.serialize();
		assert_eq!(SwapContract::deserialize(&bytes).unwrap(), p);
		for len in 0..bytes.len() { assert!(SwapContract::deserialize(&bytes[..len]).is_err()); }
		let mut unknown = bytes.clone(); unknown[0] = 2;
		assert!(SwapContract::deserialize(&unknown).is_err());
		let mut trailing = bytes; trailing.push(0);
		assert!(SwapContract::deserialize(&trailing).is_err());
		assert!(p.check_context(server_keypair().public_key(), BlockDelta::new(4), BlockHeight::new(1000)).is_err());
		assert!(p.check_context(alice_keypair().public_key(), BlockDelta::new(3), BlockHeight::new(1000)).is_err());
		assert!(p.check_context(server_keypair().public_key(), BlockDelta::new(3), BlockHeight::new(500)).is_err());
	}

	#[test]
	fn swap_candidate_commits_every_role_and_term() {
		let alice = alice_keypair().public_key();
		let bob = bob_keypair().public_key();
		let server = server_keypair().public_key();
		let hash = sha256::Hash::hash(&[42; 32]);
		let base = SwapContract::new(bob, alice, server, hash, 500, 3).unwrap();
		let output = base.taproot().unwrap().output_key();
		for changed in [
			SwapContract::new(alice, bob, server, hash, 500, 3),
			SwapContract::new(bob, server, alice, hash, 500, 3),
			SwapContract::new(bob, alice, server, sha256::Hash::hash(&[43; 32]), 500, 3),
			SwapContract::new(bob, alice, server, hash, 501, 3),
			SwapContract::new(bob, alice, server, hash, 500, 4),
		] {
			assert_ne!(changed.unwrap().taproot().unwrap().output_key(), output);
		}
		let tree = base.taproot().unwrap();
		for path in [SwapPath::Claim, SwapPath::Refund, SwapPath::RecoverClaim, SwapPath::RecoverRefund] {
			assert!(tree.control_block(&(base.script(path), taproot::LeafVersion::TapScript)).is_some());
		}
	}
}
