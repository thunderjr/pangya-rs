-- Forward-only M2 foundation. Released migrations are never edited or rolled back.
CREATE TABLE accounts (
    id BIGSERIAL PRIMARY KEY,
    username_normalized TEXT NOT NULL,
    username_display TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_accounts_username_normalized UNIQUE (username_normalized),
    CONSTRAINT ck_accounts_username_normalized CHECK (
        username_normalized = lower(username_normalized)
        AND username_normalized ~ '^[a-z0-9_]{3,32}$'
    ),
    CONSTRAINT ck_accounts_status CHECK (status IN ('active', 'banned', 'disabled'))
);

CREATE TABLE credentials (
    account_id BIGINT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    scheme TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_credentials_scheme CHECK (scheme = 'argon2id-client-md5-v1'),
    CONSTRAINT ck_credentials_hash_nonempty CHECK (length(password_hash) > 0)
);

CREATE TABLE profiles (
    account_id BIGINT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    nickname_display TEXT,
    nickname_normalized TEXT,
    rank INTEGER NOT NULL DEFAULT 0,
    experience BIGINT NOT NULL DEFAULT 0,
    pang BIGINT NOT NULL DEFAULT 0,
    points BIGINT NOT NULL DEFAULT 0,
    setup_state TEXT NOT NULL DEFAULT 'needs_nickname',
    selected_character_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_profiles_nickname_normalized UNIQUE (nickname_normalized),
    CONSTRAINT ck_profiles_nickname_pair CHECK (
        (nickname_display IS NULL) = (nickname_normalized IS NULL)
    ),
    CONSTRAINT ck_profiles_nickname_normalized CHECK (
        nickname_normalized IS NULL OR (
            nickname_normalized = lower(nickname_normalized)
            AND nickname_normalized ~ '^[a-z0-9_-]{3,16}$'
        )
    ),
    CONSTRAINT ck_profiles_rank_nonnegative CHECK (rank >= 0),
    CONSTRAINT ck_profiles_experience_nonnegative CHECK (experience >= 0),
    CONSTRAINT ck_profiles_pang_nonnegative CHECK (pang >= 0),
    CONSTRAINT ck_profiles_points_nonnegative CHECK (points >= 0),
    CONSTRAINT ck_profiles_setup_state CHECK (
        setup_state IN ('needs_nickname', 'needs_starter', 'complete')
    )
);

CREATE TABLE characters (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    item_type_id BIGINT NOT NULL,
    starter_key TEXT NOT NULL,
    hair_color SMALLINT NOT NULL DEFAULT 0,
    mastery INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_characters_owner_id UNIQUE (account_id, id),
    CONSTRAINT uq_characters_starter_key UNIQUE (account_id, starter_key),
    CONSTRAINT ck_characters_item_type_range CHECK (item_type_id BETWEEN 0 AND 4294967295),
    CONSTRAINT ck_characters_starter_key CHECK (starter_key ~ '^[a-z0-9_.-]{1,64}$'),
    CONSTRAINT ck_characters_hair_color CHECK (hair_color >= 0),
    CONSTRAINT ck_characters_mastery CHECK (mastery >= 0)
);

ALTER TABLE profiles
    ADD CONSTRAINT fk_profiles_selected_character_owner
    FOREIGN KEY (account_id, selected_character_id)
    REFERENCES characters(account_id, id)
    DEFERRABLE INITIALLY IMMEDIATE;

CREATE TABLE inventory_items (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    item_type_id BIGINT NOT NULL,
    starter_key TEXT NOT NULL,
    quantity BIGINT NOT NULL,
    durability BIGINT,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_inventory_owner_id UNIQUE (account_id, id),
    CONSTRAINT uq_inventory_starter_key UNIQUE (account_id, starter_key),
    CONSTRAINT ck_inventory_item_type_range CHECK (item_type_id BETWEEN 0 AND 4294967295),
    CONSTRAINT ck_inventory_starter_key CHECK (starter_key ~ '^[a-z0-9_.-]{1,64}$'),
    CONSTRAINT ck_inventory_quantity_positive CHECK (quantity > 0),
    CONSTRAINT ck_inventory_durability_nonnegative CHECK (durability IS NULL OR durability >= 0)
);

CREATE TABLE equipment_sets (
    id BIGSERIAL PRIMARY KEY,
    account_id BIGINT NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE,
    character_id BIGINT NOT NULL,
    club_item_id BIGINT,
    ball_item_id BIGINT,
    version BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT fk_equipment_character_owner FOREIGN KEY (account_id, character_id)
        REFERENCES characters(account_id, id),
    CONSTRAINT fk_equipment_club_owner FOREIGN KEY (account_id, club_item_id)
        REFERENCES inventory_items(account_id, id),
    CONSTRAINT fk_equipment_ball_owner FOREIGN KEY (account_id, ball_item_id)
        REFERENCES inventory_items(account_id, id),
    CONSTRAINT ck_equipment_version_nonnegative CHECK (version >= 0)
);

CREATE TABLE handover_sessions (
    id UUID PRIMARY KEY,
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    token_digest BYTEA NOT NULL,
    target TEXT NOT NULL,
    source_address_prefix TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    CONSTRAINT uq_handover_token_digest UNIQUE (token_digest),
    CONSTRAINT ck_handover_digest_length CHECK (octet_length(token_digest) = 32),
    CONSTRAINT ck_handover_target CHECK (target IN ('game', 'message')),
    CONSTRAINT ck_handover_source_prefix CHECK (
        (family(source_address_prefix::inet) = 4
            AND masklen(source_address_prefix::inet) = 24
            AND source_address_prefix = network(source_address_prefix::inet)::text)
        OR
        (family(source_address_prefix::inet) = 6
            AND masklen(source_address_prefix::inet) = 56
            AND source_address_prefix = network(source_address_prefix::inet)::text)
    ),
    CONSTRAINT ck_handover_expiry CHECK (expires_at > issued_at),
    CONSTRAINT ck_handover_consumed_time CHECK (consumed_at IS NULL OR consumed_at >= issued_at),
    CONSTRAINT ck_handover_revoked_time CHECK (revoked_at IS NULL OR revoked_at >= issued_at)
);

CREATE INDEX ix_handover_account_outstanding
    ON handover_sessions (account_id, expires_at)
    WHERE consumed_at IS NULL AND revoked_at IS NULL;

CREATE INDEX ix_handover_source_prefix_issued
    ON handover_sessions (source_address_prefix, issued_at);
