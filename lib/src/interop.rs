//! Experimental recipient authorization for inter-server Lightning settlement.
//!
//! This module does not enable payments. Callers must pin the full peer identity,
//! negotiate capabilities, and verify the eventual invoice and recovery contract.

use std::str::FromStr;

use bitcoin::hashes::{sha256, Hash, HashEngine};
use bitcoin::secp256k1::{schnorr, Keypair, Message, PublicKey};

use crate::{Address, VtxoPolicy, SECP};
use crate::lightning::Offer;

/// Explicit fork identity. Address and invoice prefixes do not identify XBT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
	XbtMainnet,
	XbtRegtest,
}

impl Network {
	fn tag(self) -> &'static [u8] {
		match self {
			Self::XbtMainnet => b"paperclip/xbt/mainnet/unified-sighash/v1",
			Self::XbtRegtest => b"paperclip/xbt/regtest/unified-sighash/v1",
		}
	}
}

#[derive(Debug, thiserror::Error)]
#[error("invalid inter-server recipient: {0}")]
pub struct Error(&'static str);

/// A recipient authorizes one exact offer for an address and full server identity.
/// Transport adapters must bound input before they decode these fields.
#[derive(Clone)]
pub struct RecipientBinding {
	pub network: Network,
	pub server_pubkey: PublicKey,
	pub address: Address,
	pub offer: String,
	pub expires_at: u64,
	pub signature: schnorr::Signature,
}

impl RecipientBinding {
	pub const MAX_OFFER_BYTES: usize = 16_384;
	pub const MAX_LIFETIME_SECS: u64 = 86_400;

	fn message(&self) -> Result<Message, Error> {
		if self.offer.len() > Self::MAX_OFFER_BYTES {
			return Err(Error("offer too large"));
		}
		let mut engine = sha256::Hash::engine();
		engine.input(b"Paperclip inter-server recipient binding v1\0");
		// Length prefixes prevent ambiguity between adjacent variable fields.
		for field in [self.network.tag(), &self.server_pubkey.serialize(),
			self.address.to_string().as_bytes(), self.offer.as_bytes()]
		{
			engine.input(&(field.len() as u64).to_be_bytes());
			engine.input(field);
		}
		engine.input(&self.expires_at.to_be_bytes());
		Ok(Message::from_digest(sha256::Hash::from_engine(engine).to_byte_array()))
	}

	pub fn sign(
		network: Network, server_pubkey: PublicKey, address: Address, offer: String,
		expires_at: u64, now: u64, recipient_key: &Keypair,
	) -> Result<Self, Error> {
		// The temporary signature is replaced before this value can be returned.
		let mut binding = Self {
			network, server_pubkey, address, offer, expires_at,
			signature: SECP.sign_schnorr_no_aux_rand(&Message::from_digest([0; 32]), recipient_key),
		};
		binding.signature = SECP.sign_schnorr_with_aux_rand(
			&binding.message()?, recipient_key, &rand::random(),
		);
		binding.verify(network, server_pubkey, now)?;
		Ok(binding)
	}

	/// Verify against locally trusted parameters, never the response's own identity.
	pub fn verify(
		&self, expected_network: Network, pinned_server: PublicKey, now: u64,
	) -> Result<Offer, Error> {
		if self.network != expected_network || self.server_pubkey != pinned_server {
			return Err(Error("peer or network mismatch"));
		}
		if !self.address.ark_id().is_for_server(pinned_server)
			|| self.address.is_testnet() != (expected_network == Network::XbtRegtest)
		{
			return Err(Error("address mismatch"));
		}
		let remaining = self.expires_at.checked_sub(now).ok_or(Error("expired"))?;
		if remaining == 0 || remaining > Self::MAX_LIFETIME_SECS {
			return Err(Error("invalid lifetime"));
		}
		let key = match self.address.policy() {
			VtxoPolicy::Pubkey(policy) => policy.user_pubkey,
			_ => return Err(Error("unsupported recipient policy")),
		};
		SECP.verify_schnorr(&self.signature, &self.message()?, &key.x_only_public_key().0)
			.map_err(|_| Error("recipient signature mismatch"))?;
		Offer::from_str(&self.offer).map_err(|_| Error("invalid offer"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bitcoin::secp256k1::SecretKey;
	use lightning::offers::offer::OfferBuilder;
	use crate::mailbox::BlindedMailboxIdentifier;

	fn key(n: u8) -> Keypair {
		Keypair::from_secret_key(&SECP, &SecretKey::from_slice(&[n; 32]).unwrap())
	}

	fn binding() -> RecipientBinding {
		let user = key(1);
		let server = key(2).public_key();
		let address = Address::builder().server_pubkey(server)
			.pubkey_policy(user.public_key())
			.delivery(crate::address::VtxoDelivery::ServerMailbox {
				blinded_id: BlindedMailboxIdentifier::from_pubkey(user.public_key()),
			}).into_address().unwrap();
		let offer = OfferBuilder::new(user.public_key()).description("interop".into())
			.build().unwrap().to_string();
		RecipientBinding::sign(Network::XbtMainnet, server, address, offer, 200, 100, &user).unwrap()
	}

	#[test]
	fn interop_recipient_binding_checks_identity_and_expiry() {
		let b = binding();
		assert!(b.verify(Network::XbtMainnet, key(2).public_key(), 100).is_ok());
		assert!(b.verify(Network::XbtRegtest, key(2).public_key(), 100).is_err());
		assert!(b.verify(Network::XbtMainnet, key(3).public_key(), 100).is_err());
		assert!(b.verify(Network::XbtMainnet, key(2).public_key(), 200).is_err());
		assert!(b.verify(Network::XbtMainnet, key(2).public_key(), u64::MAX).is_err());
	}

	#[test]
	fn interop_recipient_binding_rejects_offer_substitution() {
		let mut b = binding();
		b.offer = OfferBuilder::new(key(3).public_key()).description("attacker".into())
			.build().unwrap().to_string();
		assert!(b.verify(Network::XbtMainnet, key(2).public_key(), 100).is_err());
	}

	#[test]
	fn interop_recipient_binding_rejects_extended_expiry() {
		let mut b = binding();
		b.expires_at += 1;
		assert!(b.verify(Network::XbtMainnet, key(2).public_key(), 100).is_err());
		b.expires_at = u64::MAX;
		assert!(b.verify(Network::XbtMainnet, key(2).public_key(), 100).is_err());
	}

	#[test]
	fn interop_recipient_binding_rejects_wrong_owner_and_oversize_offer() {
		let b = binding();
		assert!(RecipientBinding::sign(b.network, b.server_pubkey, b.address.clone(),
			b.offer.clone(), 200, 100, &key(3)).is_err());
		assert!(RecipientBinding::sign(b.network, b.server_pubkey, b.address,
			"a".repeat(RecipientBinding::MAX_OFFER_BYTES + 1), 200, 100, &key(1)).is_err());
	}
}
