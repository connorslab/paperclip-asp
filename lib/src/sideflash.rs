//! Experimental Sideflash address authentication. Does not execute payments.
//! New bindings use compact v1. Legacy v0 remains readable with its own signatures.

use std::io::Cursor;
use bitcoin::bech32::{self, Bech32m, Hrp};
use bitcoin::hashes::{sha256, Hash};
use bitcoin::secp256k1::{schnorr::Signature, Keypair, Message, PublicKey};
use lightning::offers::offer::Offer;
use crate::{Address, VtxoPolicy, SECP};
use crate::encode::ReadExt;

// Bech32m's code length is 1023 symbols. With "sfl1" and six checksum
// symbols, at most 633 full payload bytes fit in this profile.
pub const MAX_PAYLOAD: usize = 633;
pub const MAX_TEXT: usize = 1023;

#[derive(Debug, thiserror::Error)]
#[error("invalid Sideflash address: {0}")]
pub struct Error(&'static str);

/// Both fields must come from the caller's configured network profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainContext {
	pub genesis: [u8; 32],
	pub fork_id: [u8; 32],
}

impl ChainContext {
	/// Development XBT profile. The discriminator identifies this protocol profile;
	/// it does not alter Lightning payment hashes or transaction signature rules.
	pub fn xbt(network: bitcoin::Network) -> Result<Self, Error> {
		if !matches!(network, bitcoin::Network::Bitcoin | bitcoin::Network::Regtest) {
			return Err(Error("unsupported XBT network"));
		}
		Ok(Self {
			genesis: bitcoin::constants::ChainHash::using_genesis_block(network).to_bytes(),
			fork_id: sha256::Hash::hash(b"Sideflash/XBT/blake2b-unified-sighash/v0").to_byte_array(),
		})
	}
	fn from_profile(profile: u64) -> Result<Self, Error> {
		Self::xbt(match profile {
			0 => bitcoin::Network::Bitcoin,
			1 => bitcoin::Network::Regtest,
			_ => return Err(Error("unknown network profile")),
		})
	}
	fn profile(self) -> Result<u64, Error> {
		for profile in [0, 1] {
			if self == Self::from_profile(profile)? { return Ok(profile); }
		}
		Err(Error("unsupported XBT chain profile"))
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Encoding { Legacy, Compact }

impl Encoding {
	fn recipient_domain(self) -> &'static [u8] {
		match self { Self::Legacy => b"Sideflash/recipient/v0\0", Self::Compact => b"Sideflash/recipient/v1\0" }
	}
	fn server_domain(self) -> &'static [u8] {
		match self { Self::Legacy => b"Sideflash/server/v0\0", Self::Compact => b"Sideflash/server/v1\0" }
	}
}

#[derive(Clone)]
pub struct Binding {
	pub chain: ChainContext,
	pub server: PublicKey,
	pub address: Address,
	/// Exact decoded BOLT12 offer TLV bytes.
	pub offer: Vec<u8>,
	pub revision: u64,
	pub not_before: u64,
	pub expires: u64,
}

/// Authentication is not proof of recipient delivery or current liquidity.
#[derive(Clone)]
pub struct SideflashAddress {
	encoding: Encoding,
	binding: Binding,
	recipient_signature: Signature,
	server_signature: Signature,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Route { NativeArk(String), LightningOffer(Vec<u8>) }

// Fixed canonical CBOR array, or a legacy map. This does not change Ark ProtocolEncoding.
fn head(out: &mut Vec<u8>, major: u8, n: u64) {
	let tag = major << 5;
	match n {
		0..=23 => out.push(tag | n as u8),
		24..=255 => out.extend([tag | 24, n as u8]),
		256..=65535 => { out.push(tag | 25); out.extend((n as u16).to_be_bytes()); },
		65536..=4294967295 => { out.push(tag | 26); out.extend((n as u32).to_be_bytes()); },
		_ => { out.push(tag | 27); out.extend(n.to_be_bytes()); },
	}
}
fn bytes(out: &mut Vec<u8>, value: &[u8]) { head(out, 2, value.len() as u64); out.extend(value); }
fn message(domain: &[u8], body: &[u8]) -> Message {
	let mut data = domain.to_vec(); data.extend(body);
	Message::from_digest(sha256::Hash::hash(&data).to_byte_array())
}

impl Binding {
	fn body(&self, encoding: Encoding, recipient: Option<&Signature>, server: Option<&Signature>) -> Result<Vec<u8>, Error> {
		let mut address = vec![u8::from(self.address.is_testnet()), 1];
		self.address.encode_payload(&mut address).map_err(|_| Error("Ark encoding"))?;
		if address.len() > 1024 || self.offer.len() > 1024 { return Err(Error("field size")); }
		let mut out = Vec::new();
		let legacy = encoding == Encoding::Legacy;
		let count = if server.is_some() { 11 } else if recipient.is_some() { 10 } else { 9 };
		head(&mut out, if legacy { 5 } else { 4 }, count + u64::from(legacy));
		let field = |out: &mut Vec<u8>, key| { if legacy { head(out, 0, key); } };
		field(&mut out, 0); head(&mut out, 0, if legacy { 0 } else { 1 });
		field(&mut out, 1); head(&mut out, 0, 0);
		if legacy {
			field(&mut out, 2); bytes(&mut out, &self.chain.genesis);
			field(&mut out, 3); bytes(&mut out, &self.chain.fork_id);
		} else {
			head(&mut out, 0, self.chain.profile()?);
		}
		for (k, value) in [(4, self.server.serialize().as_slice()), (5, address.as_slice()), (6, self.offer.as_slice())] {
			field(&mut out, k); bytes(&mut out, value);
		}
		for (k, n) in [(7, self.revision), (8, self.not_before), (9, self.expires)] {
			field(&mut out, k); head(&mut out, 0, n);
		}
		if let Some(sig) = recipient { field(&mut out, 10); bytes(&mut out, sig.as_ref()); }
		if let Some(sig) = server { field(&mut out, 11); bytes(&mut out, sig.as_ref()); }
		if out.len() > MAX_PAYLOAD { return Err(Error("payload size")); }
		Ok(out)
	}
	fn validate(&self, now: u64) -> Result<PublicKey, Error> {
		self.body(Encoding::Compact, None, None)?;
		if self.chain != ChainContext::xbt(bitcoin::Network::Bitcoin)?
			&& self.chain != ChainContext::xbt(bitcoin::Network::Regtest)? {
			return Err(Error("unsupported XBT chain profile"));
		}
		if self.not_before > now || now >= self.expires { return Err(Error("validity interval")); }
		if !self.address.is_for_server(self.server) { return Err(Error("Ark server mismatch")); }
		let mainnet = bitcoin::constants::ChainHash::using_genesis_block(bitcoin::Network::Bitcoin).to_bytes();
		if self.address.is_testnet() == (self.chain.genesis == mainnet) { return Err(Error("Ark network mismatch")); }
		let offer = Offer::try_from(self.offer.clone()).map_err(|_| Error("BOLT12 offer"))?;
		if !offer.offer_features().requires_blake2b_identity() || offer.offer_features().requires_unknown_bits() {
			return Err(Error("XBT offer features"));
		}
		if !offer.chains().iter().any(|c| c.to_bytes() == self.chain.genesis) {
			return Err(Error("offer chain"));
		}
		if offer.absolute_expiry().is_some_and(|t| t.as_secs() <= now) { return Err(Error("offer expired")); }
		match self.address.policy() {
			VtxoPolicy::Pubkey(policy) => Ok(policy.user_pubkey),
			_ => Err(Error("unsupported recipient policy")),
		}
	}
	/// Authorize compact v1. The server never needs the recipient secret key.
	pub fn authorize(&self, key: &Keypair, now: u64) -> Result<Signature, Error> {
		if self.validate(now)? != key.public_key() { return Err(Error("recipient key")); }
		Ok(SECP.sign_schnorr_with_aux_rand(&message(Encoding::Compact.recipient_domain(), &self.body(Encoding::Compact, None, None)?), key, &rand::random()))
	}
}

impl SideflashAddress {
	/// Return the wire version. Decoded legacy addresses retain version 0.
	pub fn version(&self) -> u8 { if self.encoding == Encoding::Legacy { 0 } else { 1 } }

	pub fn acknowledge(binding: Binding, recipient_signature: Signature, server: &Keypair, now: u64) -> Result<Self, Error> {
		let recipient = binding.validate(now)?;
		if server.public_key() != binding.server { return Err(Error("server key")); }
		SECP.verify_schnorr(&recipient_signature, &message(Encoding::Compact.recipient_domain(), &binding.body(Encoding::Compact, None, None)?),
			&recipient.x_only_public_key().0).map_err(|_| Error("recipient signature"))?;
		let server_signature = SECP.sign_schnorr_with_aux_rand(&message(Encoding::Compact.server_domain(),
			&binding.body(Encoding::Compact, Some(&recipient_signature), None)?), server, &rand::random());
		let result = Self { encoding: Encoding::Compact, binding, recipient_signature, server_signature };
		result.encode()?;
		Ok(result)
	}
	pub fn encode(&self) -> Result<String, Error> {
		bech32::encode::<Bech32m>(Hrp::parse("sfl").map_err(|_| Error("prefix"))?,
			&self.binding.body(self.encoding, Some(&self.recipient_signature), Some(&self.server_signature))?)
			.map_err(|_| Error("Bech32m encoding"))
	}
	/// Parse only. Call `route` with an independently authenticated server key.
	pub fn decode(text: &str) -> Result<Self, Error> {
		if text.len() > MAX_TEXT { return Err(Error("text size")); }
		let checked = bech32::primitives::decode::CheckedHrpstring::new::<Bech32m>(text)
			.map_err(|_| Error("checksum or encoding"))?;
		if !checked.hrp().as_str().eq_ignore_ascii_case("sfl") { return Err(Error("prefix")); }
		let payload: Vec<u8> = checked.byte_iter().take(MAX_PAYLOAD.saturating_add(1)).collect();
		if payload.len() > MAX_PAYLOAD { return Err(Error("payload size")); }
		let mut r = Reader(&payload);
		let encoding = match payload.first() {
			Some(0xac) => { r.number(5)?; Encoding::Legacy },
			Some(0x8b) => { r.number(4)?; Encoding::Compact },
			_ => return Err(Error("container or field count")),
		};
		r.field(encoding, 0)?;
		if r.number(0)? != if encoding == Encoding::Legacy { 0 } else { 1 } { return Err(Error("version")); }
		r.field(encoding, 1)?; if r.number(0)? != 0 { return Err(Error("required features")); }
		let chain = if encoding == Encoding::Legacy {
			r.key(2)?; let genesis = r.blob()?.try_into().map_err(|_| Error("genesis"))?;
			r.key(3)?; let fork_id = r.blob()?.try_into().map_err(|_| Error("fork id"))?;
			ChainContext { genesis, fork_id }
		} else {
			ChainContext::from_profile(r.number(0)?)?
		};
		r.field(encoding, 4)?; let server = PublicKey::from_slice(r.blob()?).map_err(|_| Error("server key"))?;
		r.field(encoding, 5)?; let native = r.blob()?;
		let (flags, payload) = native.split_at_checked(2).ok_or(Error("Ark header"))?;
		if flags[0] > 1 || flags[1] != 1 { return Err(Error("Ark profile")); }
		// Bound native length-prefixed fields before the general Ark decoder
		// allocates from their lengths. The remaining envelope is the hard bound.
		let mut native_fields = payload.get(4..).ok_or(Error("Ark identity"))?;
		while !native_fields.is_empty() {
			let mut cursor = Cursor::new(native_fields);
			let size = usize::try_from(cursor.read_compact_size().map_err(|_| Error("Ark field length"))?)
				.map_err(|_| Error("Ark field length"))?;
			let offset = usize::try_from(cursor.position()).map_err(|_| Error("Ark field offset"))?;
			let end = offset.checked_add(size).ok_or(Error("Ark field overflow"))?;
			native_fields = native_fields.get(end..).ok_or(Error("Ark field truncated"))?;
		}
		let address = Address::decode_payload(flags[0] == 1, payload.iter().copied()).map_err(|_| Error("Ark address"))?;
		r.field(encoding, 6)?; let offer = r.blob()?.to_vec();
		r.field(encoding, 7)?; let revision = r.number(0)?;
		r.field(encoding, 8)?; let not_before = r.number(0)?;
		r.field(encoding, 9)?; let expires = r.number(0)?;
		r.field(encoding, 10)?; let recipient_signature = Signature::from_slice(r.blob()?).map_err(|_| Error("recipient signature"))?;
		r.field(encoding, 11)?; let server_signature = Signature::from_slice(r.blob()?).map_err(|_| Error("server signature"))?;
		if !r.0.is_empty() { return Err(Error("trailing bytes")); }
		let result = Self { encoding, binding: Binding { chain, server, address, offer,
			revision, not_before, expires }, recipient_signature, server_signature };
		if result.encode()? != text.to_ascii_lowercase() { return Err(Error("noncanonical payload")); }
		Ok(result)
	}
	/// Extract an authenticated offer without a local Ark server or wallet.
	/// The caller must authenticate the address and pin its destination server.
	/// This checks the signed binding, not revocation, liquidity or Ark delivery.
	/// The receiver must prepare safe Ark delivery before invoice settlement.
	pub fn verified_offer(&self, chain: ChainContext, pinned_server: PublicKey, now: u64) -> Result<Vec<u8>, Error> {
		let b = &self.binding;
		if b.chain != chain || b.server != pinned_server { return Err(Error("chain or pinned identity")); }
		let recipient = b.validate(now)?;
		SECP.verify_schnorr(&self.recipient_signature, &message(self.encoding.recipient_domain(), &b.body(self.encoding, None, None)?),
			&recipient.x_only_public_key().0).map_err(|_| Error("recipient signature"))?;
		SECP.verify_schnorr(&self.server_signature, &message(self.encoding.server_domain(), &b.body(self.encoding, Some(&self.recipient_signature), None)?),
			&pinned_server.x_only_public_key().0).map_err(|_| Error("server signature"))?;
		Ok(b.offer.clone())
	}
	/// Route selection is a candidate only: remote use still needs freshness and delivery checks.
	pub fn route(&self, chain: ChainContext, pinned_server: PublicKey, local_server: PublicKey, now: u64) -> Result<Route, Error> {
		let offer = self.verified_offer(chain, pinned_server, now)?;
		Ok(if local_server == pinned_server { Route::NativeArk(self.binding.address.to_string()) } else { Route::LightningOffer(offer) })
	}
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
	fn field(&mut self, encoding: Encoding, legacy_key: u64) -> Result<(), Error> {
		if encoding == Encoding::Legacy { self.key(legacy_key)?; }
		Ok(())
	}

	fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
		let value = self.0.get(..n).ok_or(Error("truncated CBOR"))?;
		self.0 = self.0.get(n..).ok_or(Error("truncated CBOR"))?; Ok(value)
	}
	fn number(&mut self, major: u8) -> Result<u64, Error> {
		let tag = self.take(1)?.first().copied().ok_or(Error("CBOR header"))?;
		if tag >> 5 != major { return Err(Error("CBOR type")); }
		let (len, min) = match tag & 31 { n @ 0..=23 => return Ok(n as u64), 24 => (1, 24),
			25 => (2, 256), 26 => (4, 65536), 27 => (8, 4294967296), _ => return Err(Error("CBOR length")) };
		let mut n = 0u64; for byte in self.take(len)? { n = (n << 8) | u64::from(*byte); }
		if n < min { return Err(Error("nonminimal CBOR")); } Ok(n)
	}
	fn key(&mut self, expected: u64) -> Result<(), Error> {
		if self.number(0)? != expected { return Err(Error("field order")); } Ok(())
	}
	fn blob(&mut self) -> Result<&'a [u8], Error> {
		let len = usize::try_from(self.number(2)?).map_err(|_| Error("length overflow"))?;
		if len > 1024 { return Err(Error("field size")); } self.take(len)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bitcoin::secp256k1::SecretKey;
	use lightning::offers::offer::OfferBuilder;
	use lightning::util::ser::Writeable;
	fn key(n: u8) -> Keypair { Keypair::from_secret_key(&SECP, &SecretKey::from_slice(&[n; 32]).unwrap()) }
	fn address(user: PublicKey) -> Address {
		Address::builder().server_pubkey(key(2).public_key()).pubkey_policy(user)
			.delivery(crate::address::VtxoDelivery::ServerMailbox {
				blinded_id: crate::mailbox::BlindedMailboxIdentifier::from_pubkey(user),
			}).into_address().unwrap()
	}
	fn fixture() -> SideflashAddress {
		let user = key(1); let server = key(2);
		let binding = Binding {
			chain: ChainContext::xbt(bitcoin::Network::Bitcoin).unwrap(),
			server: server.public_key(), address: address(user.public_key()),
			offer: crate::bolt12_receive::create_offer(server.public_key(), key(3).public_key(), bitcoin::Network::Bitcoin,
				"Sideflash test".into(), None).unwrap().encode(),
			revision: 1, not_before: 100, expires: 200,
		};
		let sig = binding.authorize(&user, 100).unwrap();
		SideflashAddress::acknowledge(binding, sig, &server, 100).unwrap()
	}
	#[test]
	fn sideflash_roundtrip_and_routes() {
		let h = fixture(); let text = h.encode().unwrap(); assert!(text.starts_with("sfl1"));
		let decoded = SideflashAddress::decode(&text).unwrap();
		assert_eq!(decoded.version(), 1);
		for local in [key(2), key(3)] {
			assert_eq!(h.route(h.binding.chain, key(2).public_key(), local.public_key(), 100).unwrap(),
				decoded.route(h.binding.chain, key(2).public_key(), local.public_key(), 100).unwrap());
		}
		assert!(SideflashAddress::decode(&text.to_uppercase()).is_ok());
		assert_eq!(decoded.verified_offer(h.binding.chain, key(2).public_key(), 100).unwrap(), h.binding.offer);
	}
	#[test]
	fn sideflash_rejects_substitution_and_wrong_context() {
		let h = fixture();
		for field in 0..7 {
			let mut bad = h.clone();
			match field {
				0 => bad.binding.offer = OfferBuilder::new(key(3).public_key()).build().unwrap().encode(),
				1 => bad.binding.revision = 2,
				2 => bad.binding.expires = 300,
				3 => bad.recipient_signature = h.server_signature,
				4 => bad.server_signature = h.recipient_signature,
				5 => bad.binding.chain.fork_id = [0; 32],
				_ => bad.binding.address = address(key(3).public_key()),
			}
			assert!(bad.route(h.binding.chain, key(2).public_key(), key(3).public_key(), 100).is_err());
		}
		for now in [99, 200, u64::MAX] { assert!(h.route(h.binding.chain, key(2).public_key(), key(2).public_key(), now).is_err()); }
		assert!(h.route(h.binding.chain, key(3).public_key(), key(2).public_key(), 100).is_err());
		assert!(h.binding.authorize(&key(3), 100).is_err());
	}
	#[test]
	fn sideflash_shared_vectors_and_network_separation() {
		for (version, source) in [(0, include_str!("../../tests/vectors/sideflash-v0.json")),
			(1, include_str!("../../tests/vectors/sideflash-v1.json"))] {
			let fixture: serde_json::Value = serde_json::from_str(source).unwrap();
			for vector in fixture["vectors"].as_array().unwrap() {
				let network = if vector["network"] == "mainnet" { bitcoin::Network::Bitcoin } else { bitcoin::Network::Regtest };
				let text = vector["address"].as_str().unwrap();
				let decoded = SideflashAddress::decode(text).unwrap();
				assert_eq!(decoded.version(), version);
				assert_eq!(decoded.encode().unwrap(), text);
				let expected = match (version, network) {
					(0, bitcoin::Network::Bitcoin) => 911,
					(0, _) => 967,
					(1, bitcoin::Network::Bitcoin) => 785,
					_ => 841,
				};
				assert_eq!(text.len(), expected);
				let offer = decoded.verified_offer(ChainContext::xbt(network).unwrap(), decoded.binding.server, 100).unwrap();
				assert_eq!(Offer::try_from(offer).unwrap().to_string(), vector["offer"].as_str().unwrap());
				let server = vector["server_key"].as_str().unwrap().parse().unwrap();
				assert_eq!(decoded.route(ChainContext::xbt(network).unwrap(), server, server, 100).unwrap(),
					Route::NativeArk(vector["native_address"].as_str().unwrap().to_owned()));
				let other = if network == bitcoin::Network::Bitcoin { bitcoin::Network::Regtest } else { bitcoin::Network::Bitcoin };
				assert!(decoded.verified_offer(ChainContext::xbt(other).unwrap(), server, 100).is_err());
			}
		}
		assert!(ChainContext::xbt(bitcoin::Network::Signet).is_err());
		assert!(ChainContext::xbt(bitcoin::Network::Testnet).is_err());
	}
	#[test]
	fn sideflash_rejects_copied_legacy_signatures() {
		let fixture: serde_json::Value = serde_json::from_str(include_str!("../../tests/vectors/sideflash-v0.json")).unwrap();
		let legacy = SideflashAddress::decode(fixture["vectors"][0]["address"].as_str().unwrap()).unwrap();
		assert!(SideflashAddress::acknowledge(legacy.binding.clone(), legacy.recipient_signature, &key(2), 100).is_err());
		let mut changed = legacy.clone(); changed.encoding = Encoding::Compact;
		assert!(changed.verified_offer(legacy.binding.chain, key(2).public_key(), 100).is_err());
		let signature = legacy.binding.authorize(&key(1), 100).unwrap();
		let upgraded = SideflashAddress::acknowledge(legacy.binding.clone(), signature, &key(2), 100).unwrap();
		assert_eq!(upgraded.version(), 1);
		assert_eq!(upgraded.verified_offer(legacy.binding.chain, key(2).public_key(), 100).unwrap(), legacy.binding.offer);
		assert_eq!(legacy.encode().unwrap().len() - upgraded.encode().unwrap().len(), 126);
		let mut bad = upgraded.clone(); bad.server_signature = legacy.server_signature;
		assert!(bad.verified_offer(legacy.binding.chain, key(2).public_key(), 100).is_err());
	}
	#[test]
	fn sideflash_lightning_only_rejects_invalid_binding() {
		let h = fixture();
		assert!(h.verified_offer(h.binding.chain, key(3).public_key(), 100).is_err());
		assert!(h.verified_offer(h.binding.chain, key(2).public_key(), 200).is_err());
		let mut changed = h.clone(); changed.server_signature = h.recipient_signature;
		assert!(changed.verified_offer(h.binding.chain, key(2).public_key(), 100).is_err());
		let mut binding = h.binding.clone();
		binding.offer = OfferBuilder::new(key(2).public_key()).build().unwrap().encode();
		assert!(binding.authorize(&key(1), 100).is_err());
		binding.offer = OfferBuilder::new(key(2).public_key()).chain(bitcoin::Network::Regtest).build().unwrap().encode();
		assert!(binding.authorize(&key(1), 100).is_err());
	}
	#[test]
	fn sideflash_rejects_malformed_and_noncanonical_data() {
		let h = fixture(); let text = h.encode().unwrap();
		for end in 0..text.len() { assert!(SideflashAddress::decode(&text[..end]).is_err()); }
		assert!(SideflashAddress::decode(&"s".repeat(MAX_TEXT + 1)).is_err());
		let original = h.binding.body(h.encoding, Some(&h.recipient_signature), Some(&h.server_signature)).unwrap();
		let wrap = |b: &[u8]| bech32::encode::<Bech32m>(Hrp::parse("sfl").unwrap(), b).unwrap();
		let mut bad = original.clone(); bad.push(0); assert!(SideflashAddress::decode(&wrap(&bad)).is_err());
		let mut bad = original.clone(); bad[1] = 0; assert!(SideflashAddress::decode(&wrap(&bad)).is_err());
		let mut bad = original.clone(); bad[2] = 1; assert!(SideflashAddress::decode(&wrap(&bad)).is_err());
		let mut bad = original.clone(); bad[3] = 2; assert!(SideflashAddress::decode(&wrap(&bad)).is_err());
		let mut bad = original.clone(); bad.splice(1..2, [24, 1]); assert!(SideflashAddress::decode(&wrap(&bad)).is_err());
		for n in 0..original.len() { assert!(SideflashAddress::decode(&wrap(&original[..n])).is_err()); }
	}
}
