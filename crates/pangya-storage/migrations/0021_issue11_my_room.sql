-- Durable visitor-visible My Room state for issue #11.
-- The entry bytes follow PacketDoc gameservice/server/012d.ksy: four opaque bytes,
-- Furniture.iff catalog id, nineteen opaque bytes.
CREATE TABLE my_room_furniture (
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    slot_index SMALLINT NOT NULL,
    item_type_id BIGINT NOT NULL,
    unknown_prefix BYTEA NOT NULL,
    unknown_suffix BYTEA NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT pk_my_room_furniture PRIMARY KEY (account_id, slot_index),
    CONSTRAINT ck_my_room_furniture_slot CHECK (slot_index >= 0 AND slot_index < 1024),
    CONSTRAINT ck_my_room_furniture_item_type CHECK (item_type_id BETWEEN 0 AND 4294967295),
    CONSTRAINT ck_my_room_furniture_prefix CHECK (octet_length(unknown_prefix) = 4),
    CONSTRAINT ck_my_room_furniture_suffix CHECK (octet_length(unknown_suffix) = 19)
);

CREATE INDEX ix_my_room_furniture_order
    ON my_room_furniture (account_id, slot_index);

CREATE TABLE mascot_messages (
    account_id BIGINT NOT NULL,
    inventory_item_id BIGINT NOT NULL,
    message BYTEA NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT pk_mascot_messages PRIMARY KEY (account_id, inventory_item_id),
    CONSTRAINT fk_mascot_messages_inventory
        FOREIGN KEY (account_id, inventory_item_id)
        REFERENCES inventory_items (account_id, id) ON DELETE CASCADE,
    CONSTRAINT ck_mascot_messages_length CHECK (octet_length(message) BETWEEN 1 AND 30)
);

CREATE FUNCTION enforce_mascot_message_owner() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE item_class TEXT;
BEGIN
    SELECT inventory_class INTO item_class
      FROM inventory_items
     WHERE account_id = NEW.account_id AND id = NEW.inventory_item_id;
    IF item_class IS DISTINCT FROM 'mascot' THEN
        RAISE EXCEPTION 'mascot message requires a mascot inventory row';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER tr_mascot_message_owner
    BEFORE INSERT OR UPDATE ON mascot_messages
    FOR EACH ROW EXECUTE FUNCTION enforce_mascot_message_owner();
