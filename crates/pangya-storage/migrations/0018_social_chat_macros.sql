-- Issue #12: nine durable lobby chat macros, served by LoginService 0x0006.
ALTER TABLE profiles ADD COLUMN chat_macros BYTEA[] NOT NULL DEFAULT ARRAY[''::bytea,''::bytea,''::bytea,''::bytea,''::bytea,''::bytea,''::bytea,''::bytea,''::bytea];
ALTER TABLE profiles ADD CONSTRAINT ck_profiles_chat_macros_count CHECK (cardinality(chat_macros) = 9);
-- Per-slot width/NUL checks are enforced by the bounded protocol decoder and repository write.
