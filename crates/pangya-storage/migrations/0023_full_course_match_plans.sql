-- Full-course match plans reuse the durable card-shape column. Existing rows with hole=1
-- remain valid one-hole cards; new rows may carry every supported course length.
ALTER TABLE matches DROP CONSTRAINT ck_matches_hole;
ALTER TABLE matches ADD CONSTRAINT ck_matches_hole CHECK (hole BETWEEN 1 AND 18);
