-- Durable idempotency ledger for client 0x0020 equipment updates. The operation key is
-- derived from the authenticated frame; it is deliberately not a wire field.
CREATE TABLE retail_equipment_operations (
    operation_id UUID PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id),
    request_payload BYTEA NOT NULL,
    expected_version BIGINT NOT NULL,
    result_version BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_retail_equipment_operation_account UNIQUE (operation_id, account_id),
    CONSTRAINT ck_retail_equipment_operation_payload CHECK (octet_length(request_payload) > 0),
    CONSTRAINT ck_retail_equipment_operation_versions CHECK (
        expected_version BETWEEN 0 AND 4294967295
        AND result_version BETWEEN 0 AND 4294967295
    )
);
CREATE INDEX ix_retail_equipment_operations_account_created
    ON retail_equipment_operations (account_id, created_at, operation_id);
CREATE FUNCTION retail_equipment_operations_are_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'retail equipment operation rows are immutable'; END $$;
CREATE TRIGGER tr_retail_equipment_operations_immutable
    BEFORE UPDATE OR DELETE ON retail_equipment_operations
    FOR EACH ROW EXECUTE FUNCTION retail_equipment_operations_are_immutable();
