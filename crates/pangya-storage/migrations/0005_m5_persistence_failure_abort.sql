-- Extend the forward-only M5 abort reason vocabulary for runtime persistence ambiguity.
ALTER TABLE matches DROP CONSTRAINT ck_matches_abort_reason;
ALTER TABLE matches ADD CONSTRAINT ck_matches_abort_reason CHECK (
    abort_reason IS NULL OR abort_reason IN (
        'disconnect', 'loading_timeout', 'shutdown', 'startup_recovery', 'persistence_failure'
    )
);

ALTER TABLE match_audit_events DROP CONSTRAINT ck_match_audit_reason;
ALTER TABLE match_audit_events ADD CONSTRAINT ck_match_audit_reason CHECK (
    (event = 'aborted' AND reason IN (
        'disconnect', 'loading_timeout', 'shutdown', 'startup_recovery', 'persistence_failure'
    )) OR (event <> 'aborted' AND reason IS NULL)
);
