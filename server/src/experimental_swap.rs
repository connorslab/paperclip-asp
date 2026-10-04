//! Disabled-by-default, regtest-only swap admission and settlement.
use std::collections::HashMap;
use anyhow::{Context, ensure};
use bitcoin::{Network, Amount};
use bitcoin::secp256k1::{PublicKey, schnorr};
use ark::{Vtxo, VtxoId, VtxoPolicy, ServerVtxo};
use ark::vtxo::Full;
use ark::arkoor::package::{ArkoorPackageBuilder, ArkoorPackageCosignRequest, ArkoorPackageCosignResponse};
use ark::experimental_swap::SettlementBuilder;
use crate::Server;
use crate::database::tree::VtxoTreeUpdate;

impl Server {
	pub(crate) fn check_swap_enabled(&self) -> anyhow::Result<()> {
		ensure!(self.config.experimental_swaps && self.config.network == Network::Regtest,
			"experimental swaps require explicitly enabled regtest");
		Ok(())
	}

	pub(crate) async fn lock_swap(&self, request: ArkoorPackageCosignRequest<VtxoId>) -> anyhow::Result<ArkoorPackageCosignResponse> {
		self.check_swap_enabled()?;
		ensure!(request.requests.len() == 1, "swap experiment requires one input");
		let ids = request.inputs().cloned().collect::<Vec<_>>();
		let states = self.db.read(async |t| t.get_user_vtxos_by_id(&ids).await).await?;
		ensure!(states.len() == 1 && matches!(states[0].vtxo.policy(), VtxoPolicy::Pubkey(_)), "ordinary input required");
		let request = request.set_vtxos(states.into_iter().map(|v| v.vtxo))?;
		let req = &request.requests[0];
		ensure!(req.use_checkpoint && req.isolated_outputs.is_empty()
			&& req.exit_funding == Some(ark::exit_policy::small_anchor_transfer_funding()), "unsupported swap funding");
		let builder = ArkoorPackageBuilder::from_cosign_request(request).map_err(|e| anyhow!("invalid swap request: {e:?}"))?;
		let b = &builder.builders[0];
		let input = b.input();
		ensure!(input.amount() <= Amount::from_sat(1_000_000), "regtest swap size limit");
		ensure!(input.exit_depth() < self.config.max_vtxo_exit_depth, "refresh required");
		let tip = self.chain_tip().height;
		let mut swaps = 0;
		for output in b.all_outputs() {
			ensure!(output.total_amount >= bitcoin_ext::P2TR_DUST + ark::exit_policy::paperclip_policy().claim_fee,
				"insufficient recovery value");
			match &output.policy {
				VtxoPolicy::Pubkey(_) => {},
				VtxoPolicy::ExperimentalSwap(p) => {
					swaps += 1;
					p.check_context(input.server_pubkey(), input.exit_delta(), input.expiry_height()).map_err(anyhow::Error::msg)?;
					let margin = u32::from(input.exit_depth()) + 2 * u32::from(p.delay()) + 24;
					ensure!(p.deadline() > tip.to_u32().saturating_add(margin), "swap timeout too close");
					ensure!(input.expiry_height().to_u32() > p.deadline().saturating_add(margin), "swap exceeds checkpoint lifetime");
					ensure!(output.total_amount >= ark::exit_policy::small_anchor_transfer_funding().per_transaction()
						+ bitcoin_ext::P2TR_DUST + ark::exit_policy::paperclip_policy().claim_fee, "swap cannot fund settlement");
				},
				_ => bail!("unsupported swap output"),
			}
		}
		ensure!(swaps == 1 && b.all_outputs().count() <= 2, "one swap and optional change required");
		let spends: HashMap<_, _> = builder.spend_info().collect();
		let states = self.db.read(async |t| t.get_user_vtxos_by_id(&ids).await).await?;
		for state in states { state.check_spendable_for_oor(tip, *spends.get(&state.vtxo_id).context("missing spend")?)?; }
		let (signed, _) = self.cosign_oor_with_builder(builder).await?;
		Ok(signed.cosign_response())
	}

	pub(crate) async fn settle_swap(&self, id: VtxoId, recipient: PublicKey, refund: bool,
		preimage: Option<[u8; 32]>, signature: schnorr::Signature,
	) -> anyhow::Result<Vtxo<Full>> {
		self.check_swap_enabled()?;
		let _guard = self.vtxos_in_flux.try_lock([id]).map_err(|_| anyhow!("swap input in use"))?;
		let state = self.db.read(async |t| t.get_user_vtxo_by_id(id).await).await?;
		let tip = self.chain_tip().height;
		let p = match state.vtxo.policy() { VtxoPolicy::ExperimentalSwap(p) => p, _ => bail!("not a swap") };
		let mut builder = SettlementBuilder::new(state.vtxo.clone(), recipient, refund, preimage).map_err(anyhow::Error::msg)?;
		builder.set_participant_signature(signature).map_err(anyhow::Error::msg)?;
		let txid = builder.transaction().compute_txid();
		state.check_swap_spendable(tip, txid)?;
		// A lost response must remain recoverable after the deadline, even if
		// the resulting VTXO has since been spent. Return the committed proof.
		if state.oor_spent_txid == Some(txid) {
			let output_id = VtxoId::from(bitcoin::OutPoint::new(txid, 0));
			return Ok(self.db.read(async |t| t.get_user_vtxo_by_id(output_id).await).await?.vtxo);
		}
		if refund { ensure!(tip.to_u32() >= p.deadline(), "refund not mature"); }
		else { ensure!(tip.to_u32().saturating_add(12) < p.deadline(), "claim window closed; recovery required"); }
		ensure!(tip < state.vtxo.expiry_height(), "swap expired");
		builder.sign_server(self.server_key.leak_ref()).map_err(anyhow::Error::msg)?;
		let output = builder.finish().map_err(anyhow::Error::msg)?;
		let anchor = self.db.read(async |t| t.get_virtual_transaction_by_txid(output.chain_anchor().txid).await).await?
			.context("missing anchor")?;
		output.validate(anchor.signed_tx().context("unsigned anchor")?)?;
		let update = VtxoTreeUpdate::new()
			.upsert_signed_tx(output.transactions().map(|v| v.tx))
			.insert_spendable_vtxos([ServerVtxo::from(output.clone())])
			.mark_vtxos_oor_spent([(id, txid)]);
		// No signature leaves the process until the conditional spend and output
		// commit atomically. Conflicting transactions fail the SQL state guard.
		self.db.write(async |t| { t.execute_vtxo_tree_update(update).await?; Ok(()) }).await?;
		Ok(output)
	}
}
