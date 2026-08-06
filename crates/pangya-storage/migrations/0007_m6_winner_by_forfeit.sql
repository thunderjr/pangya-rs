-- Truthful synthetic M6 winner-by-forfeit completion.
-- A participant does not acquire a fabricated golf score or course record when the opponent
-- forfeits before that participant has taken a stroke.

ALTER TABLE match_players DROP CONSTRAINT ck_match_players_completion;
ALTER TABLE match_players ADD CONSTRAINT ck_match_players_completion CHECK (
    completion IS NULL OR completion IN (
        'holed', 'stroke_cap', 'winner_by_forfeit', 'give_up', 'disconnect',
        'turn_timeout', 'game_timeout'
    )
);

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
            (completion = 'winner_by_forfeit' AND score IS NULL
                AND pang_reward = 10 AND experience_reward = 5 AND NOT quit)
            OR
            (completion IN ('give_up', 'disconnect', 'turn_timeout', 'game_timeout')
                AND score IS NULL AND pang_reward = 0 AND experience_reward = 0 AND quit)
        ))
);

CREATE OR REPLACE FUNCTION validate_match_player_settlement() RETURNS trigger
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
                        (NEW.completion = 'winner_by_forfeit'
                            AND NEW.strokes IS NOT NULL AND NEW.score IS NULL AND NOT NEW.quit
                            AND NEW.pang_reward = 10 AND NEW.experience_reward = 5)
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

-- Winner-by-forfeit is an aggregate fact. A deferred constraint sees both player rows after every
-- statement in the transaction and prevents a truthful winner from being detached from the exact
-- direct forfeit that caused it.
CREATE FUNCTION validate_stroke_forfeit_pair() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    parent_mode TEXT;
    settled_count BIGINT;
    winner_count BIGINT;
    direct_forfeit_count BIGINT;
    winner_first_count BIGINT;
    direct_forfeit_second_count BIGINT;
BEGIN
    SELECT mode INTO parent_mode FROM matches WHERE id = NEW.match_id;
    IF parent_mode IS DISTINCT FROM 'stroke_two' THEN
        RETURN NEW;
    END IF;

    SELECT
        count(*) FILTER (WHERE completion IS NOT NULL),
        count(*) FILTER (WHERE completion = 'winner_by_forfeit'),
        count(*) FILTER (WHERE completion IN ('give_up', 'disconnect', 'turn_timeout')),
        count(*) FILTER (WHERE completion = 'winner_by_forfeit' AND place = 1),
        count(*) FILTER (
            WHERE completion IN ('give_up', 'disconnect', 'turn_timeout') AND place = 2
        )
    INTO settled_count, winner_count, direct_forfeit_count,
         winner_first_count, direct_forfeit_second_count
    FROM match_players
    WHERE match_id = NEW.match_id;

    IF settled_count NOT IN (0, 2)
        OR (
            (winner_count, direct_forfeit_count) <> (0, 0)
            AND (
                winner_count, direct_forfeit_count,
                winner_first_count, direct_forfeit_second_count
            ) <> (1, 1, 1, 1)
        )
    THEN
        RAISE EXCEPTION 'stroke winner-by-forfeit pairing is invalid';
    END IF;
    RETURN NEW;
END
$$;

CREATE CONSTRAINT TRIGGER match_players_stroke_forfeit_pair_checked
    AFTER INSERT OR UPDATE ON match_players
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION validate_stroke_forfeit_pair();
