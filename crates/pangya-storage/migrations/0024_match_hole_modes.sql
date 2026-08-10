-- Persist the room's hole progression mode with the whole-card shape.
ALTER TABLE matches ADD COLUMN hole_mode SMALLINT NOT NULL DEFAULT 0;
ALTER TABLE matches ADD CONSTRAINT ck_matches_hole_mode CHECK (hole_mode BETWEEN 0 AND 3);
