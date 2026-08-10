-- Preserve the direction of a pending MessageService friend request. Both directed rows
-- are retained for list rendering, but only the target's row may be used to confirm.
ALTER TABLE message_friends
    ADD COLUMN requested_by_account_id BIGINT REFERENCES accounts(id) ON DELETE CASCADE;

CREATE INDEX message_friends_pending_request_idx
    ON message_friends(owner_account_id, friend_account_id, requested_by_account_id)
    WHERE pending;
