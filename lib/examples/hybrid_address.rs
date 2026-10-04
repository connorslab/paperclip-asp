//! Public fixture keys only. Does not connect to an ASP or send payments.
use std::error::Error;
use bitcoin::secp256k1::{Keypair, SecretKey};
use lightning::offers::offer::OfferBuilder;
use ark::{Address, SECP};
use ark::address::VtxoDelivery;
use ark::mailbox::BlindedMailboxIdentifier;
use ark::interop::{Network, RecipientBinding};
use ark::hybrid_address::HybridAddress;

fn key(n: u8) -> Keypair { Keypair::from_secret_key(&SECP, &SecretKey::from_slice(&[n; 32]).expect("public fixture")) }
fn main() -> Result<(), Box<dyn Error>> {
	let args: Vec<String> = std::env::args().collect();
	if args.len() == 6 && args[1] == "decode" {
		let address = HybridAddress::decode(&args[2])?;
		println!("{:?}", address.route(Network::XbtRegtest, args[3].parse()?, args[4].parse()?, args[5].parse()?)?);
		return Ok(());
	}
	if args.len() != 2 || args[1] != "demo" { return Err("usage: hybrid_address demo | decode ADDRESS TRUSTED_DESTINATION_KEY LOCAL_ASP_KEY UNIX_TIME (regtest only)".into()); }
	let user = key(1); let asp = key(2); let other = key(3);
	let address = Address::builder().testnet(true).server_pubkey(asp.public_key()).pubkey_policy(user.public_key())
		.delivery(VtxoDelivery::ServerMailbox { blinded_id: BlindedMailboxIdentifier::from_pubkey(user.public_key()) }).into_address()?;
	let offer = OfferBuilder::new(asp.public_key()).chain(bitcoin::Network::Regtest).description("PUBLIC FIXTURE - DO NOT PAY".into()).build().map_err(|_| "fixture offer")?.to_string();
	let binding = RecipientBinding::sign(Network::XbtRegtest, asp.public_key(), address, offer, 200, 100, &user)?;
	let hybrid = HybridAddress::issue(binding, &asp, 100)?;
	let text = hybrid.encode()?;
	println!("PUBLIC FIXTURE; synthetic time 100, expires 200; no live receiving service");
	println!("destination ASP: {}\nother ASP: {}\naddress: {}", asp.public_key(), other.public_key(), text);
	let decoded = HybridAddress::decode(&text)?;
	println!("same ASP: {:?}", decoded.route(Network::XbtRegtest, asp.public_key(), asp.public_key(), 100)?);
	println!("other ASP: {:?}", decoded.route(Network::XbtRegtest, asp.public_key(), other.public_key(), 100)?);
	println!("expired rejected: {}", decoded.route(Network::XbtRegtest, asp.public_key(), other.public_key(), 200).is_err());
	Ok(())
}
