-- Forward-only admin panel foundation. Existing accounts keep the 'player' role.
--
-- `operator_audit_events.action` is a deliberately closed two-value CHECK covering the two
-- things the CLI does. Admin panel mutations get their own table instead of widening it, so
-- the operator ledger keeps meaning "the binary did this" and the admin ledger means "a
-- signed-in human did this".
ALTER TABLE accounts
    ADD COLUMN role TEXT NOT NULL DEFAULT 'player',
    ADD CONSTRAINT ck_accounts_role CHECK (role IN ('player', 'admin'));

-- Same shape as handover_sessions: nonsecret UUID selector, digest-only storage, canonical
-- privacy-minimized source prefix. A session is a bearer like any other and gets the same
-- treatment, including constant-time digest comparison in the application layer.
CREATE TABLE admin_sessions (
    id UUID PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    token_digest BYTEA NOT NULL,
    source_address_prefix TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    CONSTRAINT uq_admin_session_token_digest UNIQUE (token_digest),
    CONSTRAINT ck_admin_session_digest_length CHECK (octet_length(token_digest) = 32),
    CONSTRAINT ck_admin_session_source_prefix CHECK (
        (family(source_address_prefix::inet) = 4
            AND masklen(source_address_prefix::inet) = 24
            AND source_address_prefix = network(source_address_prefix::inet)::text)
        OR
        (family(source_address_prefix::inet) = 6
            AND masklen(source_address_prefix::inet) = 56
            AND source_address_prefix = network(source_address_prefix::inet)::text)
    ),
    CONSTRAINT ck_admin_session_expiry CHECK (expires_at > issued_at),
    CONSTRAINT ck_admin_session_seen CHECK (last_seen_at >= issued_at),
    CONSTRAINT ck_admin_session_revoked_time CHECK (revoked_at IS NULL OR revoked_at >= issued_at)
);

CREATE INDEX ix_admin_sessions_account_outstanding
    ON admin_sessions (account_id, expires_at)
    WHERE revoked_at IS NULL;

-- Open action vocabulary, unlike operator_audit_events: the panel grows verbs faster than a
-- migration cadence can track, and a bounded-length TEXT plus an append-only trigger already
-- gives the properties that CHECK was protecting.
CREATE TABLE admin_audit_events (
    id BIGSERIAL PRIMARY KEY,
    actor_account_id BIGINT NOT NULL REFERENCES accounts(id),
    action TEXT NOT NULL,
    target_account_id BIGINT REFERENCES accounts(id),
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_admin_audit_action CHECK (
        action ~ '^[a-z][a-z0-9_.]{2,63}$'
    ),
    CONSTRAINT ck_admin_audit_detail_object CHECK (jsonb_typeof(detail) = 'object')
);

CREATE INDEX ix_admin_audit_occurred ON admin_audit_events (occurred_at DESC, id DESC);
CREATE INDEX ix_admin_audit_target ON admin_audit_events (target_account_id, occurred_at DESC);

CREATE FUNCTION admin_audit_rows_are_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'admin audit rows are immutable'; END $$;
CREATE TRIGGER tr_admin_audit_immutable BEFORE UPDATE OR DELETE ON admin_audit_events
    FOR EACH ROW EXECUTE FUNCTION admin_audit_rows_are_immutable();
