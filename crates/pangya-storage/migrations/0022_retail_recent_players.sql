-- Bounded durable retail recent-player history. One row per owner/encountered account;
-- newest rows are retained by the repository transaction.
CREATE TABLE retail_recent_players (
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    recent_account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    nickname TEXT NOT NULL CHECK (octet_length(nickname) BETWEEN 1 AND 21),
    seen_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (account_id, recent_account_id)
);
CREATE INDEX retail_recent_players_order ON retail_recent_players (account_id, seen_at DESC);
