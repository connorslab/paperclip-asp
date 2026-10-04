ALTER TABLE lightning_failure_credit
	ADD COLUMN inline_reimbursement BOOLEAN NOT NULL DEFAULT TRUE,
	ADD COLUMN legacy_mailbox TEXT,
	ADD COLUMN last_retry_at TIMESTAMPTZ;
