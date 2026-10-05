use std::io::{self, Read};
use std::str::FromStr;

use bitcoin::consensus::encode::serialize_hex;
use bitcoin::secp256k1::{Keypair, Secp256k1, SecretKey};
use bitcoin::{Address, Network, OutPoint, ScriptBuf};
use crate::covenant::{self, Permit, State, CHALLENGE, PROFILE};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn field<T: DeserializeOwned>(v: &Value, name: &str) -> Result<T> {
	Ok(serde_json::from_value(v.get(name).ok_or("missing field")?.clone())?)
}
fn secret(v: &Value, name: &str) -> Result<SecretKey> {
	Ok(SecretKey::from_str(&field::<String>(v,name)?)?)
}
fn point(v: &Value, name: &str) -> Result<OutPoint> {
	Ok(OutPoint::from_str(&field::<String>(v,name)?)?)
}
fn run(role: &str) -> Result<Value> {
	let mut bytes = Vec::new();
	io::stdin().take(1_048_577).read_to_end(&mut bytes)?;
	if bytes.len() > 1_048_576 { return Err("request too large".into()); }
	let v: Value = serde_json::from_slice(&bytes)?;
	if v["profile"] != PROFILE || v["challenge"] != CHALLENGE { return Err("wrong experiment profile/network".into()); }
	match (role, field::<String>(&v,"command")?.as_str()) {
		("wallet", "pubkey") => {
			let key = Keypair::from_secret_key(&Secp256k1::new(), &secret(&v,"secret")?);
			Ok(json!({"pubkey": key.x_only_public_key().0.to_string()}))
		},
		("wallet", "authorize") => {
			let p = Permit::authorize(field(&v,"old")?,field(&v,"new")?,field(&v,"not_before")?,
				[secret(&v,"old_secret")?,secret(&v,"new_secret")?])?;
			Ok(serde_json::to_value(p)?)
		},
		("wallet", "claim") => {
			let tx = covenant::claim(&field(&v,"state")?, point(&v,"outpoint")?,
				ScriptBuf::from_hex(&field::<String>(&v,"destination")?)?,secret(&v,"secret")?)?;
			Ok(json!({"hex":serialize_hex(&tx),"txid":tx.compute_txid(),"fee_sat":covenant::CLAIM_FEE}))
		},
		("asp", "verify") => {
			let permit: Permit = field(&v,"permit")?; permit.verify()?;
			Ok(json!({"valid":true}))
		},
		("asp", "round") => {
			let state: State = field(&v,"state")?;
			let output = state.funding_output()?;
			let mut response = json!({"address":Address::from_script(&output.script_pubkey,Network::Signet)?.to_string(),
				"funding_sat":output.value.to_sat(),"script_pubkey":output.script_pubkey.to_hex_string(),
				"exit_script_pubkey":state.output()?.script_pubkey.to_hex_string(),
				"expiry":state.expiry,"claim_height":state.expiry+u32::from(state.exit_delay)});
			if v.get("funding").is_some() {
				let tx = state.signed_unroll(point(&v,"funding")?)?;
				response["hex"] = json!(serialize_hex(&tx)); response["txid"] = json!(tx.compute_txid());
			}
			Ok(response)
		},
		("asp", "refund") => {
			let permit: Permit = field(&v,"permit")?;
			let tx = permit.refund(point(&v,"old_outpoint")?,point(&v,"new_outpoint")?,secret(&v,"secret")?)?;
			Ok(json!({"hex":serialize_hex(&tx),"txid":tx.compute_txid(),"fee_sat":covenant::REFUND_FEE}))
		},
		_ => Err("unsupported command for this role".into()),
	}
}
pub fn execute(role: &str) -> std::result::Result<(), String> {
	match run(role) {
		Ok(v) => { println!("{}",v); Ok(()) },
		Err(_) => Err("Invalid experimental request; no signing result produced".into()),
	}
}
