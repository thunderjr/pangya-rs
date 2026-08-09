-- Widens `inventory_items.inventory_class` to the families the client's own shop already sells.
--
-- The catalog parsed six of the client's fifty-seven tables, so three of its six shop tabs —
-- Caddie, Mascot, Decoration — had no server catalog behind them at all and every purchase from
-- them was refused with `not_in_catalog`. The tables were always present in the client; the
-- server simply never named them.
--
-- These eight share the same 0x90-byte record base as the original families, so they need no new
-- parsing. What they need is somewhere to land when bought, which is this CHECK.
--
-- `AddonPart.iff` is deliberately absent: its rows carry type-id tags 0x04 and 0x08, the same
-- space Character and CharacterPart occupy, and admitting it would make a type id ambiguous
-- across families for the sake of three shop rows.

ALTER TABLE inventory_items
    DROP CONSTRAINT ck_inventory_class;

ALTER TABLE inventory_items
    ADD CONSTRAINT ck_inventory_class CHECK (
        inventory_class IN (
            'legacy', 'club_set', 'ball', 'consumable', 'character_part',
            'caddie', 'caddie_item', 'mascot', 'card',
            'furniture', 'skin', 'hair_style', 'set_item'
        )
    );

-- The shape rule has to move with the vocabulary or every new class is refused by a constraint
-- that predates it. All eight are `ItemStacking::Unique` in the catalog — you own one caddie, one
-- mascot, one of a given furniture piece — so they take the same `quantity = 1` rule the club
-- set, ball and character part already have. Only consumables stack, and that is unchanged.
ALTER TABLE inventory_items
    DROP CONSTRAINT ck_inventory_m7_shape;

ALTER TABLE inventory_items
    ADD CONSTRAINT ck_inventory_m7_shape CHECK (
        inventory_class = 'legacy'
        OR (inventory_class = 'consumable' AND durability IS NULL)
        OR (inventory_class IN (
                'club_set', 'ball', 'character_part',
                'caddie', 'caddie_item', 'mascot', 'card',
                'furniture', 'skin', 'hair_style', 'set_item'
            )
            AND quantity = 1
            AND (durability IS NULL OR durability <= 4294967295))
    );
