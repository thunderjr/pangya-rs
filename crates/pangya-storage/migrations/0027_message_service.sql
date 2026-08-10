-- MessageService durable social state. Rows are directed so a block/alias is owned by
-- the player who set it; friend confirmation inserts both directions atomically.
CREATE TABLE message_friends (
    owner_account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    friend_account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    alias TEXT NOT NULL DEFAULT 'Friend',
    blocked BOOLEAN NOT NULL DEFAULT FALSE,
    pending BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (owner_account_id, friend_account_id),
    CHECK (owner_account_id <> friend_account_id),
    CHECK (char_length(alias) <= 10)
);
CREATE INDEX message_friends_friend_idx ON message_friends(friend_account_id);

CREATE TABLE message_offline_messages (
    id BIGSERIAL PRIMARY KEY,
    sender_account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    recipient_account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    body BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    delivered_at TIMESTAMPTZ,
    CHECK (octet_length(body) <= 512)
);
CREATE INDEX message_offline_pending_idx
    ON message_offline_messages(recipient_account_id, id)
    WHERE delivered_at IS NULL;
