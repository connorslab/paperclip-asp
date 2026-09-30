//! Capability-based XBT backend admission, independent of CLN distributor.
//! Network names are shared with Bitcoin and cannot identify this fork alone.

use cln_rpc::GetinfoOurFeatures;

fn has_bit(features: &[u8], bit: usize) -> bool {
	features.len().checked_sub(bit / 8 + 1)
		.and_then(|index| features.get(index))
		.is_some_and(|byte| byte & (1 << (bit % 8)) != 0)
}

pub(super) fn require_xbt_features(features: Option<&GetinfoOurFeatures>) -> Result<(), &'static str> {
	let features = features.ok_or("CLN did not report its XBT features")?;
	for field in [&features.init, &features.node] {
		if !has_bit(field, 512) || has_bit(field, 513) {
			return Err("CLN must require XBT identity bit 512");
		}
		if !has_bit(field, 515) || has_bit(field, 514) {
			return Err("CLN must advertise unified signatures with peer feature bit 515");
		}
	}
	if !has_bit(&features.invoice, 512) || has_bit(&features.invoice, 513) {
		return Err("CLN invoices must require XBT identity bit 512");
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn flags(bits: &[usize]) -> Vec<u8> {
		let len = bits.iter().max().map_or(0, |bit| bit / 8 + 1);
		let mut bytes = vec![0; len];
		for bit in bits { bytes[len - 1 - bit / 8] |= 1 << (bit % 8); }
		bytes
	}

	#[test]
	fn xbt_backend_accepts_unrelated_features_and_padding() {
		let mut compatible = GetinfoOurFeatures {
			init: flags(&[0, 17, 512, 515, 1025]),
			node: flags(&[17, 512, 515]),
			invoice: flags(&[17, 512]), channel: vec![],
		};
		compatible.node.insert(0, 0);
		assert!(require_xbt_features(Some(&compatible)).is_ok());
	}

	#[test]
	fn xbt_backend_features_require_current_identity_and_signatures() {
		let valid = GetinfoOurFeatures {
			init: flags(&[512, 515]), node: flags(&[512, 515]),
			invoice: flags(&[512]), channel: vec![],
		};
		assert!(require_xbt_features(Some(&valid)).is_ok());
		assert!(require_xbt_features(None).is_err());
		for bits in [&[][..], &[68, 71], &[513, 515], &[512], &[512, 514], &[512, 513, 515]] {
			let mut invalid = valid.clone();
			invalid.init = flags(bits);
			assert!(require_xbt_features(Some(&invalid)).is_err());
		}
		let mut invalid = valid.clone();
		invalid.node.clear();
		assert!(require_xbt_features(Some(&invalid)).is_err());
		let mut invalid = valid;
		invalid.invoice = flags(&[513]);
		assert!(require_xbt_features(Some(&invalid)).is_err());
		assert!(!has_bit(&[], usize::MAX));
	}
}
