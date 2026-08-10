-- Durable U.S. 852 tutorial masks and exactly-once completion reward fences.
CREATE TABLE tutorial_progress (
    account_id BIGINT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    rookie_mask INTEGER NOT NULL DEFAULT 0,
    beginner_mask INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_tutorial_rookie_mask CHECK (rookie_mask BETWEEN 0 AND 255),
    CONSTRAINT ck_tutorial_beginner_mask CHECK (beginner_mask BETWEEN 0 AND 16128)
);

CREATE TABLE tutorial_reward_claims (
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    completion_option SMALLINT NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, completion_option),
    CONSTRAINT ck_tutorial_reward_option CHECK (completion_option IN (1, 2))
);

CREATE TABLE tutorial_mission_rewards (
    account_id BIGINT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    mission INTEGER NOT NULL,
    item_type_id BIGINT NOT NULL,
    quantity INTEGER NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, mission),
    CONSTRAINT ck_tutorial_mission_positive CHECK (mission > 0),
    CONSTRAINT ck_tutorial_mission_item_type CHECK (item_type_id BETWEEN 0 AND 4294967295),
    CONSTRAINT ck_tutorial_mission_quantity CHECK (quantity > 0)
);
