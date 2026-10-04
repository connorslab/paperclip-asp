//! Recovery costs are recorded from validated transaction builders, never from
//! a client-supplied fee. Each HTLC can receive only one persisted grant.

use anyhow::Context;
use bitcoin::Amount;
use ark::{ProtocolEncoding, Vtxo, VtxoId};
use ark::lightning::PaymentHash;
use ark::vtxo::Full;
use super::Tx;

pub struct FailureCredit {
	pub htlc_id: VtxoId,
	pub payment_hash: PaymentHash,
	pub refund_id: VtxoId,
	pub amount: Amount,
	pub paid: Option<Vec<Vtxo<Full>>>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::config::Postgres;
	use crate::database::Db;
	use ark::lightning::Preimage;
	use ark::test_util::dummy::DummyTestVtxoSpec;
	use ark::ServerVtxo;

	#[tokio::test]
	async fn lightning_credit_is_atomic_and_survives_retry_and_reconnect() {
		// Explicitly opt in to a disposable database, never an operator config.
		if std::env::var("PAPERCLIP_CREDIT_TEST_DB").as_deref() != Ok("1") { return; }
		let config = Postgres {
			host: "127.0.0.1".into(), port: 5432, name: "paperclip_credit_test".into(),
			user: Some("postgres".into()), password: None, max_connections: 4,
			connection_timeout_secs: 10, idle_timeout_secs: 90,
		};
		let db = Db::connect(&config).await.unwrap();
		let build = |amount| DummyTestVtxoSpec {
			amount: Amount::from_sat(amount), fee: Amount::ZERO, ..Default::default()
		}.build().1;
		let input = build(20_000);
		let refund = build(16_000);
		let grant = build(10_000);
		let preimage = Preimage::random();
		let hash = preimage.compute_payment_hash();
		db.write(async |t| {
			t.upsert_vtxos([ServerVtxo::from(input.clone()), ServerVtxo::from(refund.clone())]).await?;
			t.record_lightning_setup_cost(input.id(), hash, Amount::from_sat(6000)).await?;
			t.record_lightning_refund_cost(input.id(), refund.id(), Amount::from_sat(4000)).await
		}).await.unwrap();
		assert!(db.read(async |t| t.lightning_failure_credit(input.id()).await).await.unwrap().is_none());
		db.write(async |t| t.approve_unstarted_expired_payment(&[input.id()]).await).await.unwrap();
		let credit = db.read(async |t| t.lightning_failure_credit(input.id()).await).await.unwrap().unwrap();
		assert_eq!(credit.amount.to_sat(), 10_000);
		assert!(db.write(async |t| t.record_lightning_setup_cost(input.id(), hash, Amount::from_sat(9000)).await).await.is_err());
		assert!(db.write(async |t| t.record_lightning_refund_cost(input.id(), input.id(), Amount::from_sat(4000)).await).await.is_err());
		assert!(db.write(async |t| t.complete_lightning_failure_credit(&credit, &[refund.clone()]).await).await.is_err());
		// Simulate a transaction failing after preparing the signed grant.
		let aborted: anyhow::Result<()> = db.write(async |t| {
			t.complete_lightning_failure_credit(&credit, &[grant.clone()]).await?;
			bail!("simulated pool spend failure")
		}).await;
		assert!(aborted.is_err());
		assert!(db.read(async |t| t.lightning_failure_credit(input.id()).await).await.unwrap().unwrap().paid.is_none());
		let (one, two) = tokio::join!(
			db.write(async |t| t.complete_lightning_failure_credit(&credit, &[grant.clone()]).await),
			db.write(async |t| t.complete_lightning_failure_credit(&credit, &[grant.clone()]).await),
		);
		assert_ne!(one.is_ok(), two.is_ok(), "exactly one concurrent claim commits");
		drop(db);
		let db = Db::connect(&config).await.unwrap();
		let recovered = db.read(async |t| t.lightning_failure_credit(input.id()).await).await.unwrap().unwrap();
		assert_eq!(recovered.paid.unwrap()[0].serialize(), grant.serialize());
		assert!(db.write(async |t| t.complete_lightning_failure_credit(&credit, &[grant.clone()]).await).await.is_err());
		// Settlement remains an independent source of truth, not merely the
		// attempt's mutable "failed" status.
		db.write(async |t| { t.store_htlc_settlement(preimage).await?; Ok(()) }).await.unwrap();
		assert!(db.write(async |t| t.complete_lightning_failure_credit(&credit, &[grant]).await).await.is_err());
	}
}

impl Tx<'_> {
	pub async fn record_lightning_setup_cost(
		&self, id: VtxoId, hash: PaymentHash, amount: Amount,
	) -> anyhow::Result<()> {
		let amount = i64::try_from(amount.to_sat())?;
		let row = self.query_one(
			"INSERT INTO lightning_failure_credit (htlc_vtxo_id, payment_hash, setup_reserve_sat)
			 VALUES ($1, $2, $3) ON CONFLICT (htlc_vtxo_id) DO UPDATE
			 SET htlc_vtxo_id = EXCLUDED.htlc_vtxo_id
			 RETURNING payment_hash, setup_reserve_sat",
			&[&id.to_string(), &hash.to_string(), &amount],
		).await?;
		ensure!(row.get::<_, String>("payment_hash") == hash.to_string()
			&& row.get::<_, i64>("setup_reserve_sat") == amount, "conflicting recovery receipt");
		Ok(())
	}

	pub async fn record_lightning_refund_cost(
		&self, id: VtxoId, refund: VtxoId, amount: Amount,
	) -> anyhow::Result<()> {
		// Missing rows are pre-upgrade HTLCs. Never invent their setup cost.
		let row = self.query_opt(
			"SELECT refund_vtxo_id FROM lightning_failure_credit WHERE htlc_vtxo_id=$1 FOR UPDATE",
			&[&id.to_string()],
		).await?;
		if let Some(row) = row {
			let existing: Option<String> = row.get(0);
			ensure!(existing.is_none() || existing.as_deref() == Some(refund.to_string().as_str()),
				"conflicting revocation recipient");
			self.execute("UPDATE lightning_failure_credit SET refund_vtxo_id=$2, claim_reserve_sat=$3
				WHERE htlc_vtxo_id=$1", &[&id.to_string(), &refund.to_string(), &i64::try_from(amount.to_sat())?]).await?;
		}
		Ok(())
	}

	pub async fn approve_local_lightning_failure(&self, attempt_id: i64) -> anyhow::Result<()> {
		// Only the completed xpay call can authorize this, after proving there
		// are no CLN sendpays. Join by attempt, never just by reused invoice hash.
		self.execute("UPDATE lightning_failure_credit c SET approved=TRUE
			FROM lightning_payment_attempt_htlc_vtxo a
			WHERE a.lightning_payment_attempt_id=$1 AND c.htlc_vtxo_id=a.vtxo_id",
			&[&attempt_id]).await?;
		Ok(())
	}

	pub async fn approve_unstarted_expired_payment(&self, ids: &[VtxoId]) -> anyhow::Result<()> {
		let ids = ids.iter().map(ToString::to_string).collect::<Vec<_>>();
		self.execute("UPDATE lightning_failure_credit c SET approved=TRUE
			WHERE c.htlc_vtxo_id=ANY($1) AND NOT EXISTS
			(SELECT 1 FROM lightning_payment_attempt_htlc_vtxo a WHERE a.vtxo_id=c.htlc_vtxo_id)",
			&[&ids]).await?;
		Ok(())
	}

	pub async fn lightning_failure_credit(&self, id: VtxoId) -> anyhow::Result<Option<FailureCredit>> {
		let row = self.query_opt("SELECT payment_hash, refund_vtxo_id,
			setup_reserve_sat + claim_reserve_sat AS amount, reimbursement_vtxos
			FROM lightning_failure_credit
			WHERE htlc_vtxo_id=$1 AND approved AND refund_vtxo_id IS NOT NULL",
			&[&id.to_string()]).await?;
		row.map(|row| {
			let encoded: Option<Vec<Vec<u8>>> = row.get("reimbursement_vtxos");
			Ok(FailureCredit {
				htlc_id: id,
				payment_hash: row.get::<_, String>("payment_hash").parse()?,
				refund_id: row.get::<_, String>("refund_vtxo_id").parse()?,
				amount: Amount::from_sat(u64::try_from(row.get::<_, i64>("amount"))?),
				paid: encoded.map(|v| v.iter().map(|b| Vtxo::deserialize(b))
					.collect::<Result<Vec<_>, _>>()).transpose()?,
			})
		}).transpose()
	}

	pub async fn complete_lightning_failure_credit(
		&self, credit: &FailureCredit, outputs: &[Vtxo<Full>],
	) -> anyhow::Result<()> {
		self.ensure_not_settled(credit.payment_hash).await?;
		let refund = self.get_user_vtxos_by_id(&[credit.refund_id]).await?;
		let policy = refund.first().context("missing refund recipient")?.vtxo.policy();
		ensure!(outputs.iter().all(|v| v.policy() == policy), "reimbursement recipient mismatch");
		let total = outputs.iter().try_fold(Amount::ZERO, |n, v| n.checked_add(v.amount()))
			.context("reimbursement amount overflow")?;
		ensure!(total == credit.amount, "reimbursement amount mismatch");
		let encoded = outputs.iter().map(|v| v.serialize()).collect::<Vec<_>>();
		let updated = self.execute("UPDATE lightning_failure_credit SET reimbursement_vtxos=$2, paid_at=now()
			WHERE htlc_vtxo_id=$1 AND approved AND refund_vtxo_id=$3
			AND reimbursement_vtxos IS NULL AND payment_hash=$4
			AND setup_reserve_sat + claim_reserve_sat=$5",
			&[&credit.htlc_id.to_string(), &encoded, &credit.refund_id.to_string(),
				&credit.payment_hash.to_string(), &i64::try_from(credit.amount.to_sat())?]).await?;
		ensure!(updated == 1, "reimbursement already committed or no longer eligible");
		Ok(())
	}
}
