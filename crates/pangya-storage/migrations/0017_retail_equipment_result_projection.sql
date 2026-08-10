-- Store the exact durable projection returned by each retail equipment operation. Replays must
-- return this immutable result rather than whatever later mutation happens to be live.
ALTER TABLE retail_equipment_operations
    ADD COLUMN result_projection BYTEA;
