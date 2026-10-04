//! Generate development-only interoperability fixtures. Never use these keys for payments.
use ark::{Address, SECP};
use ark::sideflash::{Binding, ChainContext, SideflashAddress};
use bitcoin::{Network, secp256k1::{Keypair, SecretKey}};
use lightning::util::ser::Writeable;

fn main() {
	let key = |n| Keypair::from_secret_key(&SECP, &SecretKey::from_slice(&[n; 32]).unwrap());
	let user = key(1); let server = key(2); let relay = key(3);
	let mut vectors = Vec::new();
	for (name, network) in [("mainnet", Network::Bitcoin), ("regtest", Network::Regtest)] {
		let address = Address::new(network != Network::Bitcoin, server.public_key(),
			ark::VtxoPolicy::new_pubkey(user.public_key()),
			vec![ark::address::VtxoDelivery::ServerMailbox {
				blinded_id: ark::mailbox::BlindedMailboxIdentifier::from_pubkey(user.public_key()),
			}]);
		let offer = ark::bolt12_receive::create_offer(server.public_key(), relay.public_key(), network,
			"Test".into(), None).unwrap();
		let binding = Binding { chain: ChainContext::xbt(network).unwrap(), server: server.public_key(),
			address: address.clone(), offer: offer.encode(), revision: 1, not_before: 100, expires: 200 };
		let sig = binding.authorize(&user, 100).unwrap();
		let encoded = SideflashAddress::acknowledge(binding, sig, &server, 100).unwrap().encode().unwrap();
		vectors.push(serde_json::json!({"network":name,"address":encoded,"native_address":address.to_string(),
			"offer":offer.to_string(),"recipient_key":user.public_key().to_string(),
			"server_key":server.public_key().to_string(),"verify_at":100}));
	}
	println!("{}", serde_json::to_string_pretty(&serde_json::json!({"profile":"sideflash-v1-xbt", "vectors":vectors})).unwrap());
}
