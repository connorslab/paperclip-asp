CREATE TABLE lightning_failure_credit (
	htlc_vtxo_id TEXT PRIMARY KEY REFERENCES vtxo(vtxo_id),
	payment_hash TEXT NOT NULL,
	setup_reserve_sat BIGINT NOT NULL CHECK (setup_reserve_sat >= 0),
	approved BOOLEAN NOT NULL DEFAULT FALSE,
	refund_vtxo_id TEXT UNIQUE REFERENCES vtxo(vtxo_id),
	claim_reserve_sat BIGINT CHECK (claim_reserve_sat >= 0),
	reimbursement_vtxos BYTEA[],
	created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
	paid_at TIMESTAMPTZ,
	CHECK ((refund_vtxo_id IS NULL) = (claim_reserve_sat IS NULL)),
	CHECK ((reimbursement_vtxos IS NULL) = (paid_at IS NULL)),
	CHECK (reimbursement_vtxos IS NULL OR
		(approved AND refund_vtxo_id IS NOT NULL AND cardinality(reimbursement_vtxos) > 0))
);
