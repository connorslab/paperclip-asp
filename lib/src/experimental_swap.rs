//! Experimental swap scripts. Not a negotiated VTXO policy or a server API.
//!
//! Cooperative spends require the participant AND the ASP. Recovery spends
//! require only the participant after CSV. No participant/ASP key-path bypass
//! is available. Refunds always require CLTV, including cooperative refunds.

use std::str::FromStr;

use bitcoin::{ScriptBuf, taproot};
use bitcoin::hashes::{sha256, Hash};
use bitcoin::opcodes::all::*;
use bitcoin::script::Builder;
use bitcoin::secp256k1::{PublicKey, XOnlyPublicKey};

use crate::SECP;
use crate::vtxo::policy::{check_block_delta, check_block_height};
use crate::vtxo::policy::{Policy, VtxoPolicyKind};
use crate::vtxo::policy::clause::{HashDelaySignClause, DelayedTimelockSignClause, VtxoClause, TapScriptClause};
use bitcoin_ext::{BlockDelta, BlockHeight};

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

/// Version-one candidate, deliberately separate from production policy encoding.
#[derive(Debug, Clone)]
pub struct SwapContract {
	claimant: PublicKey,
	refund: PublicKey,
	server: PublicKey,
	hash: sha256::Hash,
	deadline: u32,
	delay: u16,
}

impl SwapContract {
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

impl Policy for SwapContract {
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
