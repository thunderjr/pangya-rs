-- Issue #12: durable 0x003c offline-note bridge.
CREATE TABLE offline_notes (
    id BIGSERIAL PRIMARY KEY,
    sender_account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    recipient_account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    operation_id BYTEA NOT NULL,
    message BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    delivered_at TIMESTAMPTZ,
    CONSTRAINT uq_offline_notes_sender_operation UNIQUE (sender_account_id, operation_id),
    CONSTRAINT ck_offline_notes_operation_width CHECK (octet_length(operation_id) = 32),
    CONSTRAINT ck_offline_notes_message_width CHECK (octet_length(message) BETWEEN 1 AND 128)
);
CREATE INDEX ix_offline_notes_recipient_pending
    ON offline_notes (recipient_account_id, id)
    WHERE delivered_at IS NULL;
