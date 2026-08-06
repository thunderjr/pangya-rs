-- Forward-only synthetic M6 exactly-two stroke aggregate and course-record projection.
-- Existing M5 rows are backfilled in place and retain their solo-v1 authority.

ALTER TABLE matches DROP CONSTRAINT ck_matches_mode;
ALTER TABLE matches ADD CONSTRAINT ck_matches_mode
    CHECK (mode IN ('solo_practice', 'stroke_two'));
ALTER TABLE matches DROP CONSTRAINT ck_matches_reward_formula;
ALTER TABLE matches ADD CONSTRAINT ck_matches_reward_formula CHECK (
    (mode = 'solo_practice' AND reward_formula = 'solo-v1') OR
    (mode = 'stroke_two' AND reward_formula = 'stroke-two-v1')
);

ALTER TABLE match_players DROP CONSTRAINT uq_match_players_solo;
ALTER TABLE match_players
    ADD COLUMN participant_order SMALLINT,
    ADD COLUMN player_result_key UUID,
    ADD COLUMN place SMALLINT,
    ADD COLUMN completion TEXT;

UPDATE match_players AS mp
SET participant_order = 0,
    player_result_key = m.result_commit_key,
    place = CASE WHEN m.status = 'committed' THEN 1 ELSE NULL END,
    completion = CASE WHEN m.status = 'committed' THEN 'holed' ELSE NULL END
FROM matches AS m
WHERE m.id = mp.match_id;

ALTER TABLE match_players
    ALTER COLUMN participant_order SET NOT NULL,
    ALTER COLUMN player_result_key SET NOT NULL,
    ADD CONSTRAINT uq_match_players_order UNIQUE (match_id, participant_order),
    ADD CONSTRAINT uq_match_players_player_result_key UNIQUE (player_result_key),
    ADD CONSTRAINT uq_match_players_match_player_key
        UNIQUE (match_id, player_result_key),
    ADD CONSTRAINT uq_match_players_authority
        UNIQUE (match_id, account_id, player_result_key),
    ADD CONSTRAINT ck_match_players_order CHECK (participant_order IN (0, 1)),
    ADD CONSTRAINT ck_match_players_place CHECK (place IS NULL OR place IN (1, 2)),
    ADD CONSTRAINT ck_match_players_completion CHECK (
        completion IS NULL OR completion IN (
            'holed', 'stroke_cap', 'give_up', 'disconnect', 'turn_timeout', 'game_timeout'
        )
    );

ALTER TABLE match_players DROP CONSTRAINT ck_match_players_strokes;
ALTER TABLE match_players ADD CONSTRAINT ck_match_players_strokes
    CHECK (strokes IS NULL OR strokes >= 0);
ALTER TABLE match_players DROP CONSTRAINT ck_match_players_rewards;
ALTER TABLE match_players ADD CONSTRAINT ck_match_players_rewards CHECK (
    (strokes IS NULL AND score IS NULL AND place IS NULL AND completion IS NULL
        AND pang_reward IS NULL AND experience_reward IS NULL
        AND pang_balance_after IS NULL AND experience_balance_after IS NULL)
    OR
    (strokes IS NOT NULL AND place IS NOT NULL AND completion IS NOT NULL
        AND pang_reward IS NOT NULL AND experience_reward IS NOT NULL
        AND pang_balance_after IS NOT NULL AND experience_balance_after IS NOT NULL
        AND pang_balance_after >= 0 AND experience_balance_after >= 0
        AND (
            (completion IN ('holed', 'stroke_cap') AND strokes > 0 AND score IS NOT NULL
                AND pang_reward > 0 AND experience_reward > 0 AND NOT quit)
            OR
            (completion IN ('give_up', 'disconnect', 'turn_timeout', 'game_timeout')
                AND score IS NULL AND pang_reward = 0 AND experience_reward = 0 AND quit)
        ))
);

ALTER TABLE currency_ledger DROP CONSTRAINT fk_currency_ledger_match_key;
ALTER TABLE currency_ledger DROP CONSTRAINT fk_currency_ledger_player;
ALTER TABLE currency_ledger ADD CONSTRAINT fk_currency_ledger_player_authority
    FOREIGN KEY (match_id, account_id, idempotency_key)
    REFERENCES match_players(match_id, account_id, player_result_key);
ALTER TABLE currency_ledger DROP CONSTRAINT ck_currency_ledger_reason;
ALTER TABLE currency_ledger ADD CONSTRAINT ck_currency_ledger_reason
    CHECK (reason IN ('solo-v1', 'stroke-two-v1'));

ALTER TABLE progression_ledger DROP CONSTRAINT fk_progression_ledger_match_key;
ALTER TABLE progression_ledger DROP CONSTRAINT fk_progression_ledger_player;
ALTER TABLE progression_ledger ADD CONSTRAINT fk_progression_ledger_player_authority
    FOREIGN KEY (match_id, account_id, idempotency_key)
    REFERENCES match_players(match_id, account_id, player_result_key);
ALTER TABLE progression_ledger DROP CONSTRAINT ck_progression_ledger_reason;
ALTER TABLE progression_ledger ADD CONSTRAINT ck_progression_ledger_reason
    CHECK (reason IN ('solo-v1', 'stroke-two-v1'));

-- Settlement shape and immutable ledgers are conditional on the parent authority.
CREATE FUNCTION validate_match_player_settlement() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    parent_mode TEXT;
    parent_formula TEXT;
    parent_result_key UUID;
BEGIN
    SELECT mode, reward_formula, result_commit_key
    INTO parent_mode, parent_formula, parent_result_key
    FROM matches WHERE id = NEW.match_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'match player parent authority is invalid';
    END IF;

    IF parent_mode = 'solo_practice' AND parent_formula = 'solo-v1' THEN
        IF NEW.participant_order IS DISTINCT FROM 0
            OR NEW.player_result_key IS DISTINCT FROM parent_result_key
            OR (
                (NEW.strokes IS NULL AND NEW.score IS NULL AND NEW.place IS NULL
                    AND NEW.completion IS NULL AND NEW.pang_reward IS NULL
                    AND NEW.experience_reward IS NULL AND NEW.pang_balance_after IS NULL
                    AND NEW.experience_balance_after IS NULL)
                OR
                (NEW.strokes > 0 AND NEW.score IS NOT NULL
                    AND NEW.place IS NOT DISTINCT FROM 1
                    AND NEW.completion IS NOT DISTINCT FROM 'holed'
                    AND NOT NEW.quit AND NEW.pang_reward > 0
                    AND NEW.experience_reward > 0
                    AND NEW.pang_balance_after IS NOT NULL
                    AND NEW.experience_balance_after IS NOT NULL)
            ) IS NOT TRUE
        THEN
            RAISE EXCEPTION 'solo match player settlement is invalid';
        END IF;
    ELSIF parent_mode = 'stroke_two' AND parent_formula = 'stroke-two-v1' THEN
        IF NEW.participant_order NOT IN (0, 1)
            OR NEW.player_result_key IS NULL
            OR NEW.player_result_key IS NOT DISTINCT FROM parent_result_key
            OR (
                (NEW.strokes IS NULL AND NEW.score IS NULL AND NEW.place IS NULL
                    AND NEW.completion IS NULL AND NEW.pang_reward IS NULL
                    AND NEW.experience_reward IS NULL AND NEW.pang_balance_after IS NULL
                    AND NEW.experience_balance_after IS NULL)
                OR
                (NEW.place IN (1, 2) AND NEW.completion IS NOT NULL
                    AND NEW.pang_balance_after IS NOT NULL
                    AND NEW.experience_balance_after IS NOT NULL
                    AND (
                        (NEW.completion IN ('holed', 'stroke_cap') AND NEW.strokes > 0
                            AND NEW.score IS NOT NULL AND NOT NEW.quit
                            AND NEW.pang_reward > 0 AND NEW.experience_reward > 0)
                        OR
                        (NEW.completion IN (
                                'give_up', 'disconnect', 'turn_timeout', 'game_timeout'
                            )
                            AND NEW.strokes IS NOT NULL AND NEW.score IS NULL AND NEW.quit
                            AND NEW.pang_reward = 0 AND NEW.experience_reward = 0)
                    ))
            ) IS NOT TRUE
        THEN
            RAISE EXCEPTION 'stroke match player settlement is invalid';
        END IF;
    ELSE
        RAISE EXCEPTION 'match player parent mode/formula is invalid';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER match_players_settlement_checked
    BEFORE INSERT OR UPDATE ON match_players
    FOR EACH ROW EXECUTE FUNCTION validate_match_player_settlement();

CREATE FUNCTION validate_match_ledger_authority() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    parent_mode TEXT;
    parent_formula TEXT;
    parent_result_key UUID;
    player_order SMALLINT;
    player_key UUID;
BEGIN
    SELECT m.mode, m.reward_formula, m.result_commit_key,
           mp.participant_order, mp.player_result_key
    INTO parent_mode, parent_formula, parent_result_key, player_order, player_key
    FROM matches m
    JOIN match_players mp ON mp.match_id = m.id
    WHERE m.id = NEW.match_id
      AND mp.account_id = NEW.account_id
      AND mp.player_result_key = NEW.idempotency_key;
    IF NOT FOUND
        OR player_key IS NULL
        OR player_key IS DISTINCT FROM NEW.idempotency_key
        OR (
            (parent_mode = 'solo_practice' AND parent_formula = 'solo-v1'
                AND NEW.reason = 'solo-v1' AND player_order = 0
                AND player_key IS NOT DISTINCT FROM parent_result_key)
            OR
            (parent_mode = 'stroke_two' AND parent_formula = 'stroke-two-v1'
                AND NEW.reason = 'stroke-two-v1' AND player_order IN (0, 1)
                AND player_key IS DISTINCT FROM parent_result_key)
        ) IS NOT TRUE
    THEN
        RAISE EXCEPTION 'match ledger authority is invalid';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER currency_ledger_authority_checked
    BEFORE INSERT OR UPDATE ON currency_ledger
    FOR EACH ROW EXECUTE FUNCTION validate_match_ledger_authority();
CREATE TRIGGER progression_ledger_authority_checked
    BEFORE INSERT OR UPDATE ON progression_ledger
    FOR EACH ROW EXECUTE FUNCTION validate_match_ledger_authority();

ALTER TABLE matches ADD CONSTRAINT uq_matches_id_mode UNIQUE (id, mode);

CREATE TABLE course_records (
    account_id BIGINT NOT NULL,
    course_id BIGINT NOT NULL,
    mode TEXT NOT NULL,
    best_score SMALLINT NOT NULL,
    best_strokes SMALLINT NOT NULL,
    rounds_completed BIGINT NOT NULL,
    best_match_id UUID NOT NULL,
    best_player_result_key UUID NOT NULL,
    first_achieved_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (account_id, course_id, mode),
    CONSTRAINT fk_course_records_account FOREIGN KEY (account_id)
        REFERENCES accounts(id),
    CONSTRAINT fk_course_records_best_mode
        FOREIGN KEY (best_match_id, mode) REFERENCES matches(id, mode),
    CONSTRAINT fk_course_records_best_authority
        FOREIGN KEY (best_match_id, account_id, best_player_result_key)
        REFERENCES match_players(match_id, account_id, player_result_key),
    CONSTRAINT ck_course_records_course_id CHECK (course_id BETWEEN 1 AND 4294967295),
    CONSTRAINT ck_course_records_mode CHECK (mode = 'stroke_two'),
    CONSTRAINT ck_course_records_best_strokes CHECK (best_strokes > 0),
    CONSTRAINT ck_course_records_rounds CHECK (rounds_completed > 0),
    CONSTRAINT ck_course_records_time CHECK (first_achieved_at <= updated_at)
);

CREATE INDEX ix_course_records_course_best
    ON course_records (course_id, mode, best_score, best_strokes, first_achieved_at);

CREATE FUNCTION validate_course_record_authority() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    authoritative_match_id UUID;
    authoritative_account_id BIGINT;
    authoritative_player_key UUID;
    authoritative_mode TEXT;
    authoritative_formula TEXT;
    authoritative_course_id BIGINT;
    authoritative_status TEXT;
    authoritative_par SMALLINT;
    authoritative_completion TEXT;
    authoritative_strokes SMALLINT;
    authoritative_score SMALLINT;
    authoritative_place SMALLINT;
BEGIN
    SELECT m.id, mp.account_id, mp.player_result_key, m.mode, m.reward_formula,
           m.course_id, m.status, m.par, mp.completion, mp.strokes, mp.score, mp.place
    INTO authoritative_match_id, authoritative_account_id, authoritative_player_key,
         authoritative_mode, authoritative_formula, authoritative_course_id,
         authoritative_status, authoritative_par, authoritative_completion,
         authoritative_strokes, authoritative_score, authoritative_place
    FROM matches m
    JOIN match_players mp ON mp.match_id = m.id
    WHERE m.id = NEW.best_match_id
      AND mp.player_result_key = NEW.best_player_result_key;
    IF NOT FOUND
        OR authoritative_match_id IS NULL
        OR authoritative_match_id IS DISTINCT FROM NEW.best_match_id
        OR authoritative_account_id IS NULL
        OR authoritative_account_id IS DISTINCT FROM NEW.account_id
        OR authoritative_player_key IS NULL
        OR authoritative_player_key IS DISTINCT FROM NEW.best_player_result_key
        OR authoritative_mode IS DISTINCT FROM NEW.mode
        OR authoritative_mode IS DISTINCT FROM 'stroke_two'
        OR authoritative_formula IS DISTINCT FROM 'stroke-two-v1'
        OR authoritative_course_id IS DISTINCT FROM NEW.course_id
        OR authoritative_status IS NULL
        OR authoritative_status NOT IN ('results_pending', 'committed')
        OR authoritative_completion IS NULL
        OR authoritative_completion IS DISTINCT FROM 'holed'
        OR authoritative_strokes IS NULL
        OR authoritative_strokes IS DISTINCT FROM NEW.best_strokes
        OR authoritative_score IS NULL
        OR authoritative_score IS DISTINCT FROM NEW.best_score
        OR authoritative_place IS NULL
        OR authoritative_place NOT IN (1, 2)
        OR authoritative_par IS NULL
        OR authoritative_score IS DISTINCT FROM authoritative_strokes - authoritative_par
    THEN
        RAISE EXCEPTION 'course record authority is invalid';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER course_records_authority_checked
    BEFORE INSERT OR UPDATE ON course_records
    FOR EACH ROW EXECUTE FUNCTION validate_course_record_authority();

-- Captured authority is immutable, and a terminal settlement cannot be rewritten.
CREATE FUNCTION reject_match_player_history_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    aggregate_status TEXT;
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'match player history rows are immutable';
    END IF;
    SELECT status INTO aggregate_status FROM matches WHERE id = OLD.match_id;
    IF aggregate_status IN ('committed', 'aborted') AND NEW IS DISTINCT FROM OLD THEN
        RAISE EXCEPTION 'terminal match player history rows are immutable';
    END IF;
    IF NEW.match_id IS DISTINCT FROM OLD.match_id
        OR NEW.account_id IS DISTINCT FROM OLD.account_id
        OR NEW.participant_order IS DISTINCT FROM OLD.participant_order
        OR NEW.player_result_key IS DISTINCT FROM OLD.player_result_key
        OR (OLD.completion IS NOT NULL AND NEW IS DISTINCT FROM OLD)
    THEN
        RAISE EXCEPTION 'match player history rows are immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER match_players_history_immutable
    BEFORE UPDATE OR DELETE ON match_players
    FOR EACH ROW EXECUTE FUNCTION reject_match_player_history_mutation();

CREATE FUNCTION reject_invalid_match_history_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id IS DISTINCT FROM OLD.id
        OR NEW.result_commit_key IS DISTINCT FROM OLD.result_commit_key
        OR NEW.mode IS DISTINCT FROM OLD.mode
        OR NEW.course_id IS DISTINCT FROM OLD.course_id
        OR NEW.hole IS DISTINCT FROM OLD.hole
        OR NEW.par IS DISTINCT FROM OLD.par
        OR NEW.catalog_sha256 IS DISTINCT FROM OLD.catalog_sha256
        OR NEW.seed IS DISTINCT FROM OLD.seed
        OR NEW.weather IS DISTINCT FROM OLD.weather
        OR NEW.wind_speed_tenths IS DISTINCT FROM OLD.wind_speed_tenths
        OR NEW.wind_angle_degrees IS DISTINCT FROM OLD.wind_angle_degrees
        OR NEW.reward_formula IS DISTINCT FROM OLD.reward_formula
        OR NEW.created_at IS DISTINCT FROM OLD.created_at
    THEN
        RAISE EXCEPTION 'match identity and configuration are immutable';
    END IF;
    IF OLD.status IN ('committed', 'aborted') AND NEW IS DISTINCT FROM OLD THEN
        RAISE EXCEPTION 'terminal match history rows are immutable';
    END IF;
    IF NOT (
        (OLD.status = 'loading' AND NEW.status IN ('loading', 'in_game', 'aborted'))
        OR (OLD.status = 'in_game' AND NEW.status IN ('in_game', 'results_pending', 'aborted'))
        OR (OLD.status = 'results_pending'
            AND NEW.status IN ('results_pending', 'committed', 'aborted'))
        OR (OLD.status IN ('committed', 'aborted') AND NEW.status = OLD.status)
    ) THEN
        RAISE EXCEPTION 'match lifecycle transition is invalid';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER matches_history_checked
    BEFORE UPDATE ON matches
    FOR EACH ROW EXECUTE FUNCTION reject_invalid_match_history_mutation();
