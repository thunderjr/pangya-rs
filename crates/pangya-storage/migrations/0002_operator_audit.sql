CREATE TABLE operator_audit_events (
    id BIGSERIAL PRIMARY KEY,
    action TEXT NOT NULL,
    account_id BIGINT REFERENCES accounts(id),
    outcome TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_operator_audit_action CHECK (
        action IN ('account_create', 'public_bind_enabled')
    ),
    CONSTRAINT ck_operator_audit_outcome CHECK (
        outcome IN ('success', 'duplicate', 'invalid', 'failed')
    )
);
