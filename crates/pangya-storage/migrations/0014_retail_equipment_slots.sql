-- Durable retail equipment projections for the 0x0020 and 0x000b/0x000c families.
-- Values are denormalized catalog snapshots; ownership remains a composite FK to inventory_items.
CREATE TABLE player_equipment_slots (
    account_id BIGINT NOT NULL,
    slot_family TEXT NOT NULL,
    slot_index SMALLINT NOT NULL,
    inventory_item_id BIGINT,
    item_type_id BIGINT NOT NULL,
    character_id BIGINT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT pk_player_equipment_slots PRIMARY KEY (account_id, slot_family, slot_index),
    CONSTRAINT ck_player_equipment_slots_family CHECK (
        slot_family IN ('caddie', 'consumable', 'decoration', 'mascot', 'cut_in')
    ),
    CONSTRAINT ck_player_equipment_slots_index CHECK (
        (slot_family = 'caddie' AND slot_index = 0)
        OR (slot_family = 'consumable' AND slot_index BETWEEN 0 AND 9)
        OR (slot_family = 'decoration' AND slot_index BETWEEN 0 AND 5)
        OR (slot_family = 'mascot' AND slot_index = 0)
        OR (slot_family = 'cut_in' AND slot_index BETWEEN 0 AND 3)
    ),
    CONSTRAINT ck_player_equipment_slots_type CHECK (item_type_id BETWEEN 0 AND 4294967295),
    CONSTRAINT fk_player_equipment_slots_inventory
        FOREIGN KEY (account_id, inventory_item_id)
        REFERENCES inventory_items (account_id, id) ON DELETE CASCADE,
    CONSTRAINT fk_player_equipment_slots_character
        FOREIGN KEY (account_id, character_id)
        REFERENCES characters (account_id, id) ON DELETE CASCADE
);
CREATE INDEX ix_player_equipment_slots_inventory
    ON player_equipment_slots (account_id, inventory_item_id)
    WHERE inventory_item_id IS NOT NULL;

CREATE FUNCTION enforce_player_equipment_slot_authority() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE selected_type BIGINT; selected_class TEXT;
BEGIN
    IF NEW.inventory_item_id IS NULL THEN
        IF NEW.item_type_id <> 0 THEN
            RAISE EXCEPTION 'empty equipment slot has a catalog id';
        END IF;
    ELSE
        SELECT item_type_id, inventory_class INTO selected_type, selected_class
          FROM inventory_items
         WHERE account_id = NEW.account_id AND id = NEW.inventory_item_id;
        IF NOT FOUND OR selected_type <> NEW.item_type_id THEN
            RAISE EXCEPTION 'equipment slot inventory snapshot mismatch';
        END IF;
        IF NEW.slot_family = 'caddie' AND selected_class <> 'caddie' THEN
            RAISE EXCEPTION 'caddie equipment class mismatch';
        ELSIF NEW.slot_family = 'consumable' AND selected_class <> 'consumable' THEN
            RAISE EXCEPTION 'consumable equipment class mismatch';
        ELSIF NEW.slot_family IN ('decoration', 'cut_in')
              AND selected_class <> 'skin' THEN
            RAISE EXCEPTION 'decoration equipment class mismatch';
        ELSIF NEW.slot_family = 'mascot' AND selected_class <> 'mascot' THEN
            RAISE EXCEPTION 'mascot equipment class mismatch';
        END IF;
    END IF;
    IF NEW.slot_family = 'cut_in' AND NEW.character_id IS NULL THEN
        RAISE EXCEPTION 'cut-in equipment needs a character';
    ELSIF NEW.slot_family <> 'cut_in' AND NEW.character_id IS NOT NULL THEN
        RAISE EXCEPTION 'unexpected character on equipment slot';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER tr_player_equipment_slot_authority
    BEFORE INSERT OR UPDATE ON player_equipment_slots
    FOR EACH ROW EXECUTE FUNCTION enforce_player_equipment_slot_authority();
