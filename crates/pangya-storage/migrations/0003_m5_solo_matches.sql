-- Forward-only local synthetic M5 checkpoint persistence.
CREATE TABLE matches (
    id UUID PRIMARY KEY,
    result_commit_key UUID NOT NULL,
    mode TEXT NOT NULL DEFAULT 'solo_practice',
    course_id BIGINT NOT NULL,
    hole SMALLINT NOT NULL,
    par SMALLINT NOT NULL,
    catalog_sha256 BYTEA NOT NULL,
    seed BYTEA NOT NULL,
    weather TEXT NOT NULL,
    reward_formula TEXT NOT NULL DEFAULT 'solo-v1',
    status TEXT NOT NULL DEFAULT 'loading',
    abort_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    committed_at TIMESTAMPTZ,
    aborted_at TIMESTAMPTZ,
    CONSTRAINT uq_matches_result_commit_key UNIQUE (result_commit_key),
    CONSTRAINT uq_matches_id_result_key UNIQUE (id, result_commit_key),
    CONSTRAINT ck_matches_mode CHECK (mode = 'solo_practice'),
    CONSTRAINT ck_matches_course_id CHECK (course_id BETWEEN 1 AND 4294967295),
    CONSTRAINT ck_matches_hole CHECK (hole = 1),
    CONSTRAINT ck_matches_par CHECK (par BETWEEN 1 AND 10),
    CONSTRAINT ck_matches_catalog_sha256 CHECK (octet_length(catalog_sha256) = 32),
    CONSTRAINT ck_matches_seed CHECK (octet_length(seed) = 32),
    CONSTRAINT ck_matches_weather CHECK (weather IN ('clear', 'cloudy', 'rain')),
    CONSTRAINT ck_matches_reward_formula CHECK (reward_formula = 'solo-v1'),
    CONSTRAINT ck_matches_status CHECK (
        status IN ('loading', 'in_game', 'results_pending', 'committed', 'aborted')
    ),
    CONSTRAINT ck_matches_abort_reason CHECK (
        abort_reason IS NULL OR abort_reason IN (
            'disconnect', 'loading_timeout', 'shutdown', 'startup_recovery'
        )
    ),
    CONSTRAINT ck_matches_terminal_consistency CHECK (
        (status = 'committed' AND committed_at IS NOT NULL AND aborted_at IS NULL
            AND abort_reason IS NULL)
        OR
        (status = 'aborted' AND committed_at IS NULL AND aborted_at IS NOT NULL
            AND abort_reason IS NOT NULL)
        OR
        (status IN ('loading', 'in_game', 'results_pending') AND committed_at IS NULL
            AND aborted_at IS NULL AND abort_reason IS NULL)
    )
);

CREATE TABLE match_players (
    match_id UUID NOT NULL,
    account_id BIGINT NOT NULL,
    strokes SMALLINT,
    score SMALLINT,
    quit BOOLEAN NOT NULL DEFAULT FALSE,
    pang_reward BIGINT,
    experience_reward BIGINT,
    pang_balance_after BIGINT,
    experience_balance_after BIGINT,
    PRIMARY KEY (match_id, account_id),
    CONSTRAINT uq_match_players_solo UNIQUE (match_id),
    CONSTRAINT fk_match_players_match FOREIGN KEY (match_id)
        REFERENCES matches(id),
    CONSTRAINT fk_match_players_account FOREIGN KEY (account_id)
        REFERENCES accounts(id),
    CONSTRAINT ck_match_players_strokes CHECK (strokes IS NULL OR strokes > 0),
    CONSTRAINT ck_match_players_rewards CHECK (
        (strokes IS NULL AND score IS NULL AND pang_reward IS NULL
            AND experience_reward IS NULL AND pang_balance_after IS NULL
            AND experience_balance_after IS NULL)
        OR
        (strokes IS NOT NULL AND score IS NOT NULL AND pang_reward IS NOT NULL
            AND experience_reward IS NOT NULL AND pang_balance_after IS NOT NULL
            AND experience_balance_after IS NOT NULL
            AND pang_reward > 0 AND experience_reward > 0
            AND pang_balance_after >= 0 AND experience_balance_after >= 0)
    )
);

CREATE TABLE currency_ledger (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id),
    match_id UUID NOT NULL,
    idempotency_key UUID NOT NULL,
    currency TEXT NOT NULL,
    delta BIGINT NOT NULL,
    reason TEXT NOT NULL,
    balance_after BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_currency_ledger_idempotency UNIQUE (idempotency_key),
    CONSTRAINT fk_currency_ledger_match_key FOREIGN KEY (match_id, idempotency_key)
        REFERENCES matches(id, result_commit_key),
    CONSTRAINT fk_currency_ledger_player FOREIGN KEY (match_id, account_id)
        REFERENCES match_players(match_id, account_id),
    CONSTRAINT ck_currency_ledger_currency CHECK (currency = 'pang'),
    CONSTRAINT ck_currency_ledger_delta CHECK (delta > 0),
    CONSTRAINT ck_currency_ledger_reason CHECK (reason = 'solo-v1'),
    CONSTRAINT ck_currency_ledger_balance CHECK (balance_after BETWEEN 0 AND 9223372036854775807)
);

CREATE TABLE progression_ledger (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id),
    match_id UUID NOT NULL,
    idempotency_key UUID NOT NULL,
    progression TEXT NOT NULL,
    delta BIGINT NOT NULL,
    reason TEXT NOT NULL,
    balance_after BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_progression_ledger_idempotency UNIQUE (idempotency_key),
    CONSTRAINT fk_progression_ledger_match_key FOREIGN KEY (match_id, idempotency_key)
        REFERENCES matches(id, result_commit_key),
    CONSTRAINT fk_progression_ledger_player FOREIGN KEY (match_id, account_id)
        REFERENCES match_players(match_id, account_id),
    CONSTRAINT ck_progression_ledger_kind CHECK (progression = 'experience'),
    CONSTRAINT ck_progression_ledger_delta CHECK (delta > 0),
    CONSTRAINT ck_progression_ledger_reason CHECK (reason = 'solo-v1'),
    CONSTRAINT ck_progression_ledger_balance CHECK (balance_after BETWEEN 0 AND 9223372036854775807)
);

CREATE TABLE match_audit_events (
    id BIGSERIAL PRIMARY KEY,
    match_id UUID NOT NULL REFERENCES matches(id),
    account_id BIGINT NOT NULL REFERENCES accounts(id),
    event TEXT NOT NULL,
    outcome TEXT NOT NULL,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_match_audit_event UNIQUE (match_id, event),
    CONSTRAINT fk_match_audit_event_player FOREIGN KEY (match_id, account_id)
        REFERENCES match_players(match_id, account_id),
    CONSTRAINT ck_match_audit_event CHECK (event IN ('started', 'aborted', 'committed')),
    CONSTRAINT ck_match_audit_outcome CHECK (outcome = 'success'),
    CONSTRAINT ck_match_audit_reason CHECK (
        (event = 'aborted' AND reason IN (
            'disconnect', 'loading_timeout', 'shutdown', 'startup_recovery'
        )) OR (event <> 'aborted' AND reason IS NULL)
    )
);

CREATE INDEX ix_matches_nonterminal ON matches (created_at, id)
    WHERE status IN ('loading', 'in_game', 'results_pending');
CREATE INDEX ix_match_players_account ON match_players (account_id, match_id);
CREATE INDEX ix_currency_ledger_account ON currency_ledger (account_id, created_at);
CREATE INDEX ix_progression_ledger_account ON progression_ledger (account_id, created_at);

CREATE FUNCTION reject_immutable_match_history_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'match history rows are immutable';
END
$$;

CREATE TRIGGER currency_ledger_immutable BEFORE UPDATE OR DELETE ON currency_ledger
    FOR EACH ROW EXECUTE FUNCTION reject_immutable_match_history_mutation();
CREATE TRIGGER progression_ledger_immutable BEFORE UPDATE OR DELETE ON progression_ledger
    FOR EACH ROW EXECUTE FUNCTION reject_immutable_match_history_mutation();
CREATE TRIGGER match_audit_events_immutable BEFORE UPDATE OR DELETE ON match_audit_events
    FOR EACH ROW EXECUTE FUNCTION reject_immutable_match_history_mutation();
