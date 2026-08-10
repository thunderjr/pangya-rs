-- Full-card stroke settlement stores per-hole par in matches.par while the
-- authoritative score formula applies it across every hole in matches.hole.
CREATE OR REPLACE FUNCTION validate_course_record_authority() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    authoritative_match_id UUID;
    authoritative_account_id BIGINT;
    authoritative_player_key UUID;
    authoritative_mode TEXT;
    authoritative_formula TEXT;
    authoritative_course_id BIGINT;
    authoritative_holes SMALLINT;
    authoritative_status TEXT;
    authoritative_par SMALLINT;
    authoritative_completion TEXT;
    authoritative_strokes SMALLINT;
    authoritative_score SMALLINT;
    authoritative_place SMALLINT;
BEGIN
    SELECT m.id, mp.account_id, mp.player_result_key, m.mode, m.reward_formula,
           m.course_id, m.hole, m.status, m.par, mp.completion, mp.strokes, mp.score, mp.place
    INTO authoritative_match_id, authoritative_account_id, authoritative_player_key,
         authoritative_mode, authoritative_formula, authoritative_course_id,
         authoritative_holes, authoritative_status, authoritative_par,
         authoritative_completion, authoritative_strokes, authoritative_score,
         authoritative_place
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
        OR authoritative_holes IS NULL
        OR authoritative_holes <= 0
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
        OR authoritative_score IS DISTINCT FROM
            authoritative_strokes - authoritative_par * authoritative_holes
    THEN
        RAISE EXCEPTION 'course record authority is invalid';
    END IF;
    RETURN NEW;
END
$$;
