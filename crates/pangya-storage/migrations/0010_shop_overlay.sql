-- Forward-only DB-backed shop overlay.
--
-- The catalog is parsed once at startup from the client's own IFF tables and is immutable, so
-- there is no runtime way to change what the server sells. `data.price_override_pang` can
-- reprice rows the client already sells, but deliberately cannot make an unsold row sellable.
--
-- This table is the missing axis: per item, whether the server offers it and for how much,
-- resolved at purchase time on top of the immutable catalog. It changes what the server
-- CHARGES and PERMITS. It does not change what the client DISPLAYS — the client renders shop
-- names, prices and listing from its own tables, and only re-authoring the IFF changes that.
CREATE TABLE shop_offer_overrides (
    item_type_id BIGINT PRIMARY KEY,
    -- NULL means "inherit whatever the client's own shop flag says".
    enabled BOOLEAN,
    -- NULL means "inherit the client's own price".
    pang BIGINT,
    note TEXT,
    updated_by BIGINT REFERENCES accounts(id),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_shop_override_type_range CHECK (item_type_id BETWEEN 0 AND 4294967295),
    -- Zero is how the client's own tables spell "not for sale"; an override that means
    -- "free" would be indistinguishable from one that means "unavailable".
    CONSTRAINT ck_shop_override_pang CHECK (pang IS NULL OR pang > 0),
    CONSTRAINT ck_shop_override_note CHECK (note IS NULL OR length(note) <= 200),
    -- A row that inherits both fields says nothing; deleting it is the way to clear.
    CONSTRAINT ck_shop_override_meaningful CHECK (enabled IS NOT NULL OR pang IS NOT NULL)
);

CREATE INDEX ix_shop_overrides_updated ON shop_offer_overrides (updated_at DESC);

-- A single counter the running server can compare against what it last loaded, so a reload is
-- driven by "something changed" rather than by polling every row.
CREATE TABLE shop_overlay_revision (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE,
    revision BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_shop_overlay_singleton CHECK (singleton),
    CONSTRAINT ck_shop_overlay_revision CHECK (revision >= 0)
);

INSERT INTO shop_overlay_revision (singleton) VALUES (TRUE);

CREATE FUNCTION bump_shop_overlay_revision() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE shop_overlay_revision
       SET revision = revision + 1, updated_at = now()
     WHERE singleton;
    RETURN NULL;
END $$;

-- A statement-level trigger: one bump per write, not one per row.
CREATE TRIGGER tr_shop_overlay_revision
    AFTER INSERT OR UPDATE OR DELETE ON shop_offer_overrides
    FOR EACH STATEMENT EXECUTE FUNCTION bump_shop_overlay_revision();
