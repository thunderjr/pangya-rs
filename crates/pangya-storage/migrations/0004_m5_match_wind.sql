-- Persist deterministic wind selected for synthetic M5 matches.
-- Temporary defaults safely backfill matches created before this migration.
ALTER TABLE matches
    ADD COLUMN wind_speed_tenths SMALLINT NOT NULL DEFAULT 0,
    ADD COLUMN wind_angle_degrees SMALLINT NOT NULL DEFAULT 0,
    ADD CONSTRAINT ck_matches_wind_speed_tenths
        CHECK (wind_speed_tenths BETWEEN 0 AND 150),
    ADD CONSTRAINT ck_matches_wind_angle_degrees
        CHECK (wind_angle_degrees BETWEEN 0 AND 359);

-- New matches must always provide authoritative wind explicitly.
ALTER TABLE matches
    ALTER COLUMN wind_speed_tenths DROP DEFAULT,
    ALTER COLUMN wind_angle_degrees DROP DEFAULT;
