-- Forward-only MessageService durability: outbound delivery leases and presence TTL.
ALTER TABLE message_offline_messages
    ADD COLUMN delivery_lease_until TIMESTAMPTZ,
    ADD COLUMN delivery_lease_token BYTEA;
ALTER TABLE message_offline_messages
    ADD CONSTRAINT ck_message_offline_lease_token_width
        CHECK (delivery_lease_token IS NULL OR octet_length(delivery_lease_token) = 16),
    ADD CONSTRAINT ck_message_offline_delivery_state
        CHECK ((delivery_lease_until IS NULL) = (delivery_lease_token IS NULL));
ALTER TABLE message_presence
    ADD COLUMN expires_at TIMESTAMPTZ NOT NULL DEFAULT (now() + interval '90 seconds');
CREATE INDEX message_presence_expiry_idx ON message_presence(expires_at);
