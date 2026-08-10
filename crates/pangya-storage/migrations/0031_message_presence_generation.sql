-- MessageService final presence correctness: generation-fence queued events and safely
-- discard legacy pending requests whose direction was unknowable before migration 0025.
-- A NULL requester cannot be safely promoted to friendship or assigned a direction.
DELETE FROM message_friends
WHERE pending AND requested_by_account_id IS NULL;

ALTER TABLE message_presence
    ADD COLUMN generation BIGINT NOT NULL DEFAULT 0;
ALTER TABLE message_presence_events
    ADD COLUMN sender_generation BIGINT NOT NULL DEFAULT 0;

-- Keep the monotonically increasing fence after the live projection is removed on Goodbye.
CREATE TABLE message_presence_generations (
    account_id BIGINT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    generation BIGINT NOT NULL DEFAULT 0
);
INSERT INTO message_presence_generations(account_id, generation)
SELECT account_id, generation FROM message_presence;

CREATE INDEX message_presence_events_sender_generation_idx
    ON message_presence_events(sender_account_id, sender_generation, id);
