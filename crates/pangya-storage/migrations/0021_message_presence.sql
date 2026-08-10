-- Durable MessageService presence and fan-out notifications. Presence is intentionally
-- separate from accounts: it is a lease-like projection removed on Goodbye/disconnect.
CREATE TABLE message_presence (
    account_id BIGINT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    status SMALLINT NOT NULL,
    room_number SMALLINT NOT NULL,
    room_type INTEGER NOT NULL,
    server_id BIGINT NOT NULL,
    channel_id SMALLINT NOT NULL,
    channel_name BYTEA NOT NULL DEFAULT ''
);
CREATE TABLE message_presence_events (
    id BIGSERIAL PRIMARY KEY,
    recipient_account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    sender_account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    status SMALLINT NOT NULL,
    room_number SMALLINT NOT NULL,
    room_type INTEGER NOT NULL,
    server_id BIGINT NOT NULL,
    channel_id SMALLINT NOT NULL,
    channel_name BYTEA NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX message_presence_events_recipient_idx ON message_presence_events(recipient_account_id, id);
