-- Issue #35: lease offline-note delivery before acknowledging the outbound socket write.
ALTER TABLE offline_notes
    ADD COLUMN delivery_lease_until TIMESTAMPTZ,
    ADD COLUMN delivery_lease_token BYTEA;

ALTER TABLE offline_notes
    ADD CONSTRAINT ck_offline_notes_lease_token_width
        CHECK (delivery_lease_token IS NULL OR octet_length(delivery_lease_token) = 16),
    ADD CONSTRAINT ck_offline_notes_delivery_state
        CHECK (
            ((delivery_lease_until IS NULL) = (delivery_lease_token IS NULL)) AND
            ((delivered_at IS NULL) OR (delivery_lease_until IS NULL AND delivery_lease_token IS NULL))
        );
