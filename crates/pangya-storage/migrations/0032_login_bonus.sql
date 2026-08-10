-- Durable per-account/server-day login bonus claims. The unique key is the idempotency fence.
CREATE TABLE login_bonus_claims (
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    server_day BIGINT NOT NULL,
    calendar_day INTEGER NOT NULL,
    item_type_id BIGINT NOT NULL,
    quantity INTEGER NOT NULL,
    inventory_item_id BIGINT NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, server_day),
    CONSTRAINT ck_login_bonus_server_day CHECK (server_day >= 0),
    CONSTRAINT ck_login_bonus_calendar_day CHECK (calendar_day > 0),
    CONSTRAINT ck_login_bonus_item_type CHECK (item_type_id BETWEEN 0 AND 4294967295),
    CONSTRAINT ck_login_bonus_quantity CHECK (quantity > 0),
    CONSTRAINT fk_login_bonus_inventory FOREIGN KEY (account_id, inventory_item_id)
        REFERENCES inventory_items(account_id, id)
);
