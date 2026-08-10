-- A GM capability is distinct from the HTTP admin role. It is read at game authentication
-- and can never be asserted by a client packet.
ALTER TABLE accounts
    ADD COLUMN game_master BOOLEAN NOT NULL DEFAULT FALSE;
