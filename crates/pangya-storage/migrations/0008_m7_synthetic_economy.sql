-- Forward-only M7 local synthetic economy. Existing M2-M6 rows remain `legacy`.
ALTER TABLE inventory_items
    ADD COLUMN inventory_class TEXT NOT NULL DEFAULT 'legacy',
    ADD CONSTRAINT ck_inventory_class CHECK (
        inventory_class IN ('legacy', 'club_set', 'ball', 'consumable', 'character_part')
    ),
    ADD CONSTRAINT ck_inventory_m7_shape CHECK (
        inventory_class = 'legacy'
        OR (inventory_class = 'consumable' AND durability IS NULL)
        OR (inventory_class IN ('club_set', 'ball', 'character_part') AND quantity = 1
            AND (durability IS NULL OR durability <= 4294967295))
    );

CREATE UNIQUE INDEX uq_inventory_consumable_owner_type
    ON inventory_items (account_id, item_type_id)
    WHERE inventory_class = 'consumable';
CREATE INDEX ix_inventory_owner_type ON inventory_items (account_id, item_type_id, id);

CREATE TABLE economy_operations (
    operation_id UUID PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id),
    command TEXT NOT NULL,
    catalog_sha256 BYTEA NOT NULL,
    request_type_id BIGINT,
    request_quantity BIGINT,
    request_inventory_id BIGINT,
    request_expected_version BIGINT,
    request_character_id BIGINT,
    request_character_type_id BIGINT,
    request_club_item_id BIGINT,
    request_club_type_id BIGINT,
    request_ball_item_id BIGINT,
    request_ball_type_id BIGINT,
    result_inventory_id BIGINT,
    result_type_id BIGINT,
    result_quantity BIGINT,
    result_durability BIGINT,
    result_pang_balance BIGINT,
    result_pang_cost BIGINT,
    result_character_id BIGINT,
    result_club_item_id BIGINT,
    result_ball_item_id BIGINT,
    result_equipment_version BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_economy_operation_account UNIQUE (operation_id, account_id),
    CONSTRAINT ck_economy_command CHECK (command IN ('purchase', 'equip', 'consume', 'repair')),
    CONSTRAINT ck_economy_catalog_hash CHECK (octet_length(catalog_sha256) = 32),
    CONSTRAINT ck_economy_type_ranges CHECK (
        (request_type_id IS NULL OR request_type_id BETWEEN 0 AND 4294967295)
        AND (request_character_type_id IS NULL OR request_character_type_id BETWEEN 0 AND 4294967295)
        AND (request_club_type_id IS NULL OR request_club_type_id BETWEEN 0 AND 4294967295)
        AND (request_ball_type_id IS NULL OR request_ball_type_id BETWEEN 0 AND 4294967295)
        AND (result_type_id IS NULL OR result_type_id BETWEEN 0 AND 4294967295)
    ),
    CONSTRAINT ck_economy_nonnegative_results CHECK (
        (result_quantity IS NULL OR result_quantity BETWEEN 0 AND 4294967295)
        AND (result_durability IS NULL OR result_durability BETWEEN 0 AND 4294967295)
        AND (result_pang_balance IS NULL OR result_pang_balance >= 0)
        AND (result_pang_cost IS NULL OR result_pang_cost >= 0)
        AND (request_expected_version IS NULL
            OR request_expected_version BETWEEN 0 AND 4294967295)
        AND (result_equipment_version IS NULL
            OR result_equipment_version BETWEEN 0 AND 4294967295)
        AND (request_inventory_id IS NULL OR request_inventory_id > 0)
        AND (request_character_id IS NULL OR request_character_id > 0)
        AND (request_club_item_id IS NULL OR request_club_item_id > 0)
        AND (request_ball_item_id IS NULL OR request_ball_item_id > 0)
        AND (result_inventory_id IS NULL OR result_inventory_id > 0)
        AND (result_character_id IS NULL OR result_character_id > 0)
        AND (result_club_item_id IS NULL OR result_club_item_id > 0)
        AND (result_ball_item_id IS NULL OR result_ball_item_id > 0)
    ),
    CONSTRAINT ck_economy_operation_shape CHECK (
        (command = 'purchase'
            AND request_type_id IS NOT NULL AND request_quantity > 0
            AND request_inventory_id IS NULL AND request_expected_version IS NULL
            AND request_character_id IS NULL AND request_character_type_id IS NULL
            AND request_club_item_id IS NULL AND request_club_type_id IS NULL
            AND request_ball_item_id IS NULL AND request_ball_type_id IS NULL
            AND result_inventory_id IS NOT NULL AND result_type_id = request_type_id
            AND result_quantity > 0 AND result_pang_balance IS NOT NULL
            AND result_pang_cost IS NOT NULL AND result_character_id IS NULL
            AND result_club_item_id IS NULL AND result_ball_item_id IS NULL
            AND result_equipment_version IS NULL)
        OR (command = 'equip'
            AND request_type_id IS NULL AND request_quantity IS NULL AND request_inventory_id IS NULL
            AND request_expected_version IS NOT NULL AND request_character_id IS NOT NULL
            AND request_character_type_id IS NOT NULL
            AND (request_club_item_id IS NULL) = (request_club_type_id IS NULL)
            AND (request_ball_item_id IS NULL) = (request_ball_type_id IS NULL)
            AND result_inventory_id IS NULL AND result_type_id IS NULL AND result_quantity IS NULL
            AND result_durability IS NULL AND result_pang_balance IS NULL AND result_pang_cost IS NULL
            AND result_character_id = request_character_id
            AND result_club_item_id IS NOT DISTINCT FROM request_club_item_id
            AND result_ball_item_id IS NOT DISTINCT FROM request_ball_item_id
            AND result_equipment_version IS NOT NULL)
        OR (command = 'consume'
            AND request_type_id IS NOT NULL AND request_quantity IS NULL AND request_inventory_id IS NOT NULL
            AND request_expected_version IS NULL AND request_character_id IS NULL
            AND request_character_type_id IS NULL AND request_club_item_id IS NULL
            AND request_club_type_id IS NULL AND request_ball_item_id IS NULL
            AND request_ball_type_id IS NULL AND result_inventory_id = request_inventory_id
            AND result_type_id = request_type_id AND result_quantity IS NOT NULL
            AND result_durability IS NULL AND result_pang_balance IS NULL
            AND result_pang_cost IS NULL AND result_character_id IS NULL
            AND result_club_item_id IS NULL AND result_ball_item_id IS NULL
            AND result_equipment_version IS NULL)
        OR (command = 'repair'
            AND request_type_id IS NOT NULL AND request_quantity IS NULL AND request_inventory_id IS NOT NULL
            AND request_expected_version IS NULL AND request_character_id IS NULL
            AND request_character_type_id IS NULL AND request_club_item_id IS NULL
            AND request_club_type_id IS NULL AND request_ball_item_id IS NULL
            AND request_ball_type_id IS NULL AND result_inventory_id = request_inventory_id
            AND result_type_id = request_type_id AND result_quantity = 1
            AND result_durability IS NOT NULL AND result_pang_balance IS NOT NULL
            AND result_pang_cost > 0 AND result_character_id IS NULL
            AND result_club_item_id IS NULL AND result_ball_item_id IS NULL
            AND result_equipment_version IS NULL)
    )
);
CREATE INDEX ix_economy_operations_account_created
    ON economy_operations (account_id, created_at, operation_id);

CREATE TABLE shop_currency_ledger (
    id BIGSERIAL PRIMARY KEY,
    operation_id UUID NOT NULL,
    account_id BIGINT NOT NULL,
    delta BIGINT NOT NULL,
    reason TEXT NOT NULL,
    balance_after BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT fk_shop_currency_operation FOREIGN KEY (operation_id, account_id)
        REFERENCES economy_operations(operation_id, account_id),
    CONSTRAINT uq_shop_currency_operation UNIQUE (operation_id),
    CONSTRAINT ck_shop_currency_reason CHECK (reason IN ('purchase', 'repair')),
    CONSTRAINT ck_shop_currency_negative CHECK (delta < 0),
    CONSTRAINT ck_shop_currency_balance CHECK (balance_after >= 0)
);
CREATE INDEX ix_shop_currency_account_created
    ON shop_currency_ledger (account_id, created_at, id);

CREATE TABLE item_ledger (
    id BIGSERIAL PRIMARY KEY,
    operation_id UUID NOT NULL,
    account_id BIGINT NOT NULL,
    inventory_id BIGINT NOT NULL,
    item_type_id BIGINT NOT NULL,
    quantity_delta BIGINT NOT NULL,
    quantity_after BIGINT NOT NULL,
    durability_delta BIGINT,
    durability_after BIGINT,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT fk_item_ledger_operation FOREIGN KEY (operation_id, account_id)
        REFERENCES economy_operations(operation_id, account_id),
    CONSTRAINT uq_item_ledger_operation UNIQUE (operation_id),
    CONSTRAINT ck_item_ledger_type CHECK (item_type_id BETWEEN 0 AND 4294967295),
    CONSTRAINT ck_item_ledger_after CHECK (
        quantity_after >= 0 AND (durability_after IS NULL OR durability_after >= 0)
    ),
    CONSTRAINT ck_item_ledger_reason CHECK (reason IN ('purchase', 'consume', 'repair')),
    CONSTRAINT ck_item_ledger_shape CHECK (
        (reason = 'purchase' AND quantity_delta > 0 AND quantity_after > 0
            AND (durability_delta IS NULL OR durability_delta > 0))
        OR (reason = 'consume' AND quantity_delta = -1 AND durability_delta IS NULL
            AND durability_after IS NULL)
        OR (reason = 'repair' AND quantity_delta = 0 AND quantity_after = 1
            AND durability_delta > 0 AND durability_after > 0)
    )
);
CREATE INDEX ix_item_ledger_inventory_created
    ON item_ledger (account_id, inventory_id, created_at, id);
CREATE INDEX ix_item_ledger_type_created
    ON item_ledger (account_id, item_type_id, created_at, id);

CREATE TABLE equipment_ledger (
    id BIGSERIAL PRIMARY KEY,
    operation_id UUID NOT NULL,
    account_id BIGINT NOT NULL,
    character_id BIGINT NOT NULL,
    club_item_id BIGINT,
    ball_item_id BIGINT,
    version_after BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT fk_equipment_ledger_operation FOREIGN KEY (operation_id, account_id)
        REFERENCES economy_operations(operation_id, account_id),
    CONSTRAINT uq_equipment_ledger_operation UNIQUE (operation_id),
    CONSTRAINT fk_equipment_ledger_character FOREIGN KEY (account_id, character_id)
        REFERENCES characters(account_id, id),
    CONSTRAINT fk_equipment_ledger_club FOREIGN KEY (account_id, club_item_id)
        REFERENCES inventory_items(account_id, id),
    CONSTRAINT fk_equipment_ledger_ball FOREIGN KEY (account_id, ball_item_id)
        REFERENCES inventory_items(account_id, id),
    CONSTRAINT ck_equipment_ledger_version CHECK (version_after >= 0)
);
CREATE INDEX ix_equipment_ledger_account_created
    ON equipment_ledger (account_id, created_at, id);

CREATE FUNCTION enforce_economy_ledger_authority() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE operation economy_operations%ROWTYPE; payload JSONB;
BEGIN
    SELECT * INTO operation FROM economy_operations
      WHERE operation_id = NEW.operation_id AND account_id = NEW.account_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'missing economy operation authority'; END IF;
    payload := to_jsonb(NEW);
    IF TG_TABLE_NAME = 'shop_currency_ledger' AND (
        operation.command <> payload->>'reason'
        OR (payload->>'delta')::BIGINT <> -operation.result_pang_cost
        OR (payload->>'balance_after')::BIGINT <> operation.result_pang_balance
    ) THEN
        RAISE EXCEPTION 'currency ledger authority mismatch';
    ELSIF TG_TABLE_NAME = 'item_ledger' AND (
        operation.command <> payload->>'reason'
        OR (payload->>'inventory_id')::BIGINT <> operation.result_inventory_id
        OR (payload->>'item_type_id')::BIGINT <> operation.result_type_id
        OR (payload->>'quantity_after')::BIGINT <> operation.result_quantity
        OR (operation.command = 'purchase'
            AND (payload->>'quantity_delta')::BIGINT <> operation.request_quantity)
        OR (operation.command = 'consume' AND (payload->>'quantity_delta')::BIGINT <> -1)
        OR (operation.command = 'repair' AND (
            (payload->>'quantity_delta')::BIGINT <> 0
            OR (payload->>'durability_after')::BIGINT <> operation.result_durability
        ))
    ) THEN
        RAISE EXCEPTION 'item ledger authority mismatch';
    ELSIF TG_TABLE_NAME = 'equipment_ledger' AND (
        operation.command <> 'equip'
        OR (payload->>'character_id')::BIGINT <> operation.result_character_id
        OR (payload->>'club_item_id')::BIGINT IS DISTINCT FROM operation.result_club_item_id
        OR (payload->>'ball_item_id')::BIGINT IS DISTINCT FROM operation.result_ball_item_id
        OR (payload->>'version_after')::BIGINT <> operation.result_equipment_version
    ) THEN
        RAISE EXCEPTION 'equipment ledger authority mismatch';
    END IF;
    RETURN NEW;
END $$;

CREATE TRIGGER tr_shop_currency_authority BEFORE INSERT ON shop_currency_ledger
    FOR EACH ROW EXECUTE FUNCTION enforce_economy_ledger_authority();
CREATE TRIGGER tr_item_ledger_authority BEFORE INSERT ON item_ledger
    FOR EACH ROW EXECUTE FUNCTION enforce_economy_ledger_authority();
CREATE TRIGGER tr_equipment_ledger_authority BEFORE INSERT ON equipment_ledger
    FOR EACH ROW EXECUTE FUNCTION enforce_economy_ledger_authority();

CREATE FUNCTION economy_rows_are_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'economy audit rows are immutable'; END $$;
CREATE TRIGGER tr_economy_operations_immutable BEFORE UPDATE OR DELETE ON economy_operations
    FOR EACH ROW EXECUTE FUNCTION economy_rows_are_immutable();
CREATE TRIGGER tr_shop_currency_immutable BEFORE UPDATE OR DELETE ON shop_currency_ledger
    FOR EACH ROW EXECUTE FUNCTION economy_rows_are_immutable();
CREATE TRIGGER tr_item_ledger_immutable BEFORE UPDATE OR DELETE ON item_ledger
    FOR EACH ROW EXECUTE FUNCTION economy_rows_are_immutable();
CREATE TRIGGER tr_equipment_ledger_immutable BEFORE UPDATE OR DELETE ON equipment_ledger
    FOR EACH ROW EXECUTE FUNCTION economy_rows_are_immutable();

CREATE FUNCTION enforce_m7_equipment_classes() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE selected_class TEXT;
BEGIN
    IF NEW.club_item_id IS NOT NULL THEN
        SELECT inventory_class INTO selected_class FROM inventory_items
          WHERE account_id = NEW.account_id AND id = NEW.club_item_id;
        IF selected_class NOT IN ('legacy', 'club_set') THEN
            RAISE EXCEPTION 'club equipment class mismatch';
        END IF;
    END IF;
    IF NEW.ball_item_id IS NOT NULL THEN
        SELECT inventory_class INTO selected_class FROM inventory_items
          WHERE account_id = NEW.account_id AND id = NEW.ball_item_id;
        IF selected_class NOT IN ('legacy', 'ball') THEN
            RAISE EXCEPTION 'ball equipment class mismatch';
        END IF;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER tr_equipment_classes BEFORE INSERT OR UPDATE ON equipment_sets
    FOR EACH ROW EXECUTE FUNCTION enforce_m7_equipment_classes();
