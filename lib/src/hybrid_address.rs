//! Experimental address codec and authenticated route selection. Never executes payments.
use std::io::{Cursor, Read};
use bitcoin::bech32::{self, Bech32m, Hrp};
use bitcoin::hashes::{sha256, Hash};
use bitcoin::secp256k1::{schnorr, Keypair, Message, PublicKey};
use crate::encode::{ReadExt, WriteExt};
use crate::interop::{Network, RecipientBinding};
use crate::{Address, SECP};

#[derive(Debug, thiserror::Error)]
#[error("invalid hybrid address: {0}")]
pub struct Error(&'static str);

/// The selected destination is authenticated, not an authorization to pay.
#[derive(Debug, PartialEq, Eq)]
pub enum Route { Ark(String), Bolt12(String) }

/// A recipient binding countersigned by the destination ASP.
#[derive(Clone)]
pub struct HybridAddress { binding: RecipientBinding, acknowledgment: schnorr::Signature }

impl HybridAddress {
	pub const MAX_TEXT: usize = 4096;
	fn body(binding: &RecipientBinding) -> Result<Vec<u8>, Error> {
		let mut b = vec![1, match binding.network { Network::XbtMainnet => 0, Network::XbtRegtest => 1 }];
		b.extend(binding.server_pubkey.serialize());
		b.extend(binding.expires_at.to_be_bytes());
		for s in [binding.address.to_string(), binding.offer.clone()] {
			if s.len() > 1024 { return Err(Error("field too large")); }
			b.emit_compact_size(s.len() as u64).map_err(|_| Error("encoding"))?;
			b.extend(s.as_bytes());
		}
		b.extend(binding.signature.serialize());
		Ok(b)
	}
	fn message(binding: &RecipientBinding) -> Result<Message, Error> {
		let mut b = b"Ark hybrid ASP acknowledgment v1\0".to_vec();
		b.extend(Self::body(binding)?);
		Ok(Message::from_digest(sha256::Hash::hash(&b).to_byte_array()))
	}
	pub fn issue(binding: RecipientBinding, asp: &Keypair, now: u64) -> Result<Self, Error> {
		binding.verify(binding.network, asp.public_key(), now).map_err(|_| Error("recipient binding"))?;
		let acknowledgment = SECP.sign_schnorr_with_aux_rand(&Self::message(&binding)?, asp, &rand::random());
		let value = Self { binding, acknowledgment };
		value.encode()?;
		Ok(value)
	}
	pub fn encode(&self) -> Result<String, Error> {
		let mut bytes = Self::body(&self.binding)?;
		bytes.extend(self.acknowledgment.serialize());
		let text = bech32::encode::<Bech32m>(Hrp::parse("hyark").map_err(|_| Error("prefix"))?, &bytes)
			.map_err(|_| Error("encoding too large"))?;
		if text.len() > Self::MAX_TEXT { return Err(Error("address too large")); }
		Ok(text)
	}
	/// Parsing does not establish trust. Call route with externally pinned identities.
	pub fn decode(text: &str) -> Result<Self, Error> {
		if text.len() > Self::MAX_TEXT { return Err(Error("address too large")); }
		let checked = bech32::primitives::decode::CheckedHrpstring::new::<Bech32m>(text)
			.map_err(|_| Error("checksum or encoding"))?;
		if !checked.hrp().as_str().eq_ignore_ascii_case("hyark") { return Err(Error("prefix")); }
		let bytes: Vec<u8> = checked.byte_iter().collect();
		let mut r = Cursor::new(bytes.as_slice());
		let mut tag = [0; 2]; r.read_exact(&mut tag).map_err(|_| Error("truncated"))?;
		if tag[0] != 1 { return Err(Error("version")); }
		let network = match tag[1] { 0 => Network::XbtMainnet, 1 => Network::XbtRegtest, _ => return Err(Error("network")) };
		let mut pk = [0; 33]; r.read_exact(&mut pk).map_err(|_| Error("truncated"))?;
		let server_pubkey = PublicKey::from_slice(&pk).map_err(|_| Error("ASP key"))?;
		let mut expiry = [0; 8]; r.read_exact(&mut expiry).map_err(|_| Error("truncated"))?;
		let mut read_string = || -> Result<String, Error> {
			let len = r.read_compact_size().map_err(|_| Error("length"))?;
			if len > 1024 { return Err(Error("field too large")); }
			let mut b = vec![0; len as usize]; r.read_exact(&mut b).map_err(|_| Error("truncated"))?;
			String::from_utf8(b).map_err(|_| Error("UTF-8"))
		};
		let address: Address = read_string()?.parse().map_err(|_| Error("Ark address"))?;
		let offer = read_string()?;
		let mut sig = [0; 64]; r.read_exact(&mut sig).map_err(|_| Error("truncated"))?;
		let signature = schnorr::Signature::from_slice(&sig).map_err(|_| Error("recipient signature"))?;
		r.read_exact(&mut sig).map_err(|_| Error("truncated"))?;
		let acknowledgment = schnorr::Signature::from_slice(&sig).map_err(|_| Error("ASP signature"))?;
		if r.position() != bytes.len() as u64 { return Err(Error("trailing bytes")); }
		let binding = RecipientBinding { network, server_pubkey, address, offer, expires_at: u64::from_be_bytes(expiry), signature };
		let value = Self { binding, acknowledgment };
		// Reject alternative length encodings or text spellings inside the signed payload.
		if value.encode()? != text.to_ascii_lowercase() { return Err(Error("noncanonical")); }
		Ok(value)
	}
	pub fn route(&self, network: Network, trusted_destination: PublicKey, local_asp: PublicKey, now: u64) -> Result<Route, Error> {
		let offer = self.binding.verify(network, trusted_destination, now).map_err(|_| Error("recipient, network, identity or expiry"))?;
		let chain = bitcoin::constants::ChainHash::using_genesis_block(match network {
			Network::XbtMainnet => bitcoin::Network::Bitcoin, Network::XbtRegtest => bitcoin::Network::Regtest,
		});
		if !offer.chains().contains(&chain) { return Err(Error("offer chain")); }
		if offer.absolute_expiry().is_some_and(|expiry| expiry.as_secs() <= now) { return Err(Error("offer expired")); }
		SECP.verify_schnorr(&self.acknowledgment, &Self::message(&self.binding)?, &trusted_destination.x_only_public_key().0)
			.map_err(|_| Error("ASP acknowledgment"))?;
		if local_asp == trusted_destination { Ok(Route::Ark(self.binding.address.to_string())) }
		else { Ok(Route::Bolt12(self.binding.offer.clone())) }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bitcoin::secp256k1::SecretKey;
	use lightning::offers::offer::OfferBuilder;
	use crate::mailbox::BlindedMailboxIdentifier;
	fn key(n: u8) -> Keypair { Keypair::from_secret_key(&SECP, &SecretKey::from_slice(&[n; 32]).unwrap()) }
	fn fixture() -> HybridAddress {
		let user = key(1); let asp = key(2);
		let address = Address::builder().server_pubkey(asp.public_key()).pubkey_policy(user.public_key())
			.delivery(crate::address::VtxoDelivery::ServerMailbox { blinded_id: BlindedMailboxIdentifier::from_pubkey(user.public_key()) }).into_address().unwrap();
		let offer = OfferBuilder::new(asp.public_key()).description("hybrid demo".into()).build().unwrap().to_string();
		HybridAddress::issue(RecipientBinding::sign(Network::XbtMainnet, asp.public_key(), address, offer, 200, 100, &user).unwrap(), &asp, 100).unwrap()
	}
	#[test]
	fn hybrid_roundtrip_and_routes() {
		let h = fixture(); let text = h.encode().unwrap(); let decoded = HybridAddress::decode(&text).unwrap();
		assert_eq!(decoded.route(Network::XbtMainnet, key(2).public_key(), key(2).public_key(), 100).unwrap(), Route::Ark(h.binding.address.to_string()));
		assert_eq!(decoded.route(Network::XbtMainnet, key(2).public_key(), key(3).public_key(), 100).unwrap(), Route::Bolt12(h.binding.offer));
		assert!(HybridAddress::decode(&text.to_uppercase()).is_ok());
	}
	#[test]
	fn hybrid_rejects_network_identity_expiry_and_tampering() {
		let mut h = fixture();
		for (net, peer, now) in [(Network::XbtRegtest, key(2), 100), (Network::XbtMainnet, key(3), 100), (Network::XbtMainnet, key(2), 200)] {
			assert!(h.route(net, peer.public_key(), key(4).public_key(), now).is_err());
		}
		h.binding.offer = OfferBuilder::new(key(4).public_key()).build().unwrap().to_string();
		assert!(h.route(Network::XbtMainnet, key(2).public_key(), key(2).public_key(), 100).is_err());
		let mut h = fixture(); h.acknowledgment = fixture().binding.signature;
		assert!(h.route(Network::XbtMainnet, key(2).public_key(), key(3).public_key(), 100).is_err());
	}
	#[test]
	fn hybrid_rejects_malformed_payloads() {
		let h = fixture(); let text = h.encode().unwrap();
		for end in 0..text.len() { assert!(HybridAddress::decode(&text[..end]).is_err()); }
		assert!(HybridAddress::decode(&"a".repeat(HybridAddress::MAX_TEXT + 1)).is_err());
		let mut bytes = SelfContained::bytes(&h);
		bytes.push(0);
		assert!(HybridAddress::decode(&bech32::encode::<Bech32m>(Hrp::parse("hyark").unwrap(), &bytes).unwrap()).is_err());
		bytes[0] = 2;
		assert!(HybridAddress::decode(&bech32::encode::<Bech32m>(Hrp::parse("hyark").unwrap(), &bytes).unwrap()).is_err());
	}
	#[test]
	fn hybrid_rejects_signed_wrong_chain_offer() {
		let mut h = fixture();
		h.binding.offer = OfferBuilder::new(key(2).public_key()).chain(bitcoin::Network::Regtest).build().unwrap().to_string();
		let b = RecipientBinding::sign(h.binding.network, h.binding.server_pubkey, h.binding.address,
			h.binding.offer, 200, 100, &key(1)).unwrap();
		let h = HybridAddress::issue(b, &key(2), 100).unwrap();
		assert!(h.route(Network::XbtMainnet, key(2).public_key(), key(3).public_key(), 100).is_err());
	}
	struct SelfContained;
	impl SelfContained { fn bytes(h: &HybridAddress) -> Vec<u8> { let mut b = HybridAddress::body(&h.binding).unwrap(); b.extend(h.acknowledgment.serialize()); b } }
}
