-- PacketDoc subtype-9 bytes are opaque; retain them without assigning skin semantics.
ALTER TABLE player_equipment_slots
    ADD COLUMN cut_in_opaque BYTEA;
ALTER TABLE player_equipment_slots
    ADD CONSTRAINT ck_player_equipment_slots_cut_in_opaque
    CHECK (cut_in_opaque IS NULL OR (slot_family = 'cut_in' AND octet_length(cut_in_opaque) = 16));
ALTER TABLE player_equipment_slots
    ADD CONSTRAINT ck_player_equipment_slots_cut_in_data
    CHECK (slot_family <> 'cut_in' OR cut_in_opaque IS NOT NULL);

-- characters.hair_color already exists in the account foundation migration; updates below use it atomically.
