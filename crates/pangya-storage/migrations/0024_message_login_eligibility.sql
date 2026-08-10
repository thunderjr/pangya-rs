-- One-time LoginService -> MessageService eligibility. No bearer is sent on 0x0012:
-- the shared database binds the authenticated identity to the exact peer address and nickname.
CREATE TABLE message_login_eligibility (
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    nickname TEXT NOT NULL,
    peer_ip INET NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (account_id, nickname, peer_ip),
    CONSTRAINT ck_message_eligibility_nickname CHECK (char_length(nickname) BETWEEN 1 AND 22),
    CONSTRAINT ck_message_eligibility_expiry CHECK (expires_at > issued_at)
);
CREATE INDEX message_login_eligibility_expiry_idx
    ON message_login_eligibility(expires_at);
