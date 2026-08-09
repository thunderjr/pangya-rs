-- Worn character parts: the first slot family beyond character/club set/ball to become durable.
--
-- The U.S. 852 wire has carried 24 part slots inside the 513-byte character block since M3, and
-- the server has answered them with zeros the whole time. `SPEC_DURABLE_PLAYER_STATE.md` names
-- this as milestone E2 and states the reason plainly: the wire models the slot, but nothing
-- persists it, so a player's outfit survives exactly as long as their session.
--
-- Keyed per character, not per account. Each owned character wears its own outfit in this
-- client, and the equipment update arrives inside that character's block — flattening it to the
-- account would silently share one outfit across every character a player owns.

CREATE TABLE character_part_slots (
    account_id BIGINT NOT NULL,
    character_id BIGINT NOT NULL,
    -- 0..23, matching CHARACTER_PARTS in crates/pangya-protocol. The client addresses these
    -- positionally, so the index is the identity of the slot and not a display order.
    slot_index SMALLINT NOT NULL,
    -- The owned row backing this slot, when the client named one. Nullable because the character
    -- block carries a type id and a uid per slot and a retail client sends a bare type id with a
    -- zero uid for parts it treats as intrinsic to the character.
    inventory_item_id BIGINT,
    item_type_id BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT pk_character_part_slots PRIMARY KEY (account_id, character_id, slot_index),
    CONSTRAINT ck_character_part_slots_index CHECK (slot_index BETWEEN 0 AND 23),
    CONSTRAINT ck_character_part_slots_type_range
        CHECK (item_type_id BETWEEN 0 AND 4294967295),
    -- A worn part must be a real part, never the "empty" encoding: an empty slot is an absent
    -- row, so that "worn nothing" and "worn something we failed to record" cannot look alike.
    CONSTRAINT ck_character_part_slots_type_nonzero CHECK (item_type_id > 0),

    -- Ownership rides the composite key rather than a bare character reference, so a row can
    -- never attach one account's character to another account. Same trick as
    -- `uq_inventory_owner_id` in 0008.
    CONSTRAINT fk_character_part_slots_character
        FOREIGN KEY (account_id, character_id)
        REFERENCES characters (account_id, id) ON DELETE CASCADE,
    -- And when the slot names an owned row, that row must belong to the same account.
    CONSTRAINT fk_character_part_slots_inventory
        FOREIGN KEY (account_id, inventory_item_id)
        REFERENCES inventory_items (account_id, id) ON DELETE SET NULL
);

-- The read path is "every slot for this account's character", which the primary key already
-- serves. This index exists for the delete-by-item path an inventory removal needs.
CREATE INDEX ix_character_part_slots_inventory
    ON character_part_slots (account_id, inventory_item_id)
    WHERE inventory_item_id IS NOT NULL;
