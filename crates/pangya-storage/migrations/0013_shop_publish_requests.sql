-- An operator-driven queue for re-authoring the client's own shop tables.
--
-- `shop_offer_overrides` (0010) already makes the shop admin-controlled on the *server* side: it
-- decides what is charged and what is permitted, live, with no restart. What it cannot do is
-- change what the client DISPLAYS, because the client renders shop names, prices and listing from
-- IFF tables inside its own PAK series. Closing that half means re-authoring those tables and
-- shipping the archive — which is a filesystem job over proprietary inputs, not something the
-- HTTP surface that mutates player state should ever be able to execute.
--
-- So the console does not run the authoring. It enqueues a request here, carrying the exact
-- catalog document it rendered, and a worker outside this process claims it, authors, publishes,
-- and reports back. That keeps one point of control (this database) without putting code
-- execution behind an admin cookie.
--
-- See docs/SPEC_CLIENT_PATCH_DELIVERY.md PATCH-009.

CREATE TABLE shop_publish_requests (
    id BIGSERIAL PRIMARY KEY,
    requested_by BIGINT NOT NULL REFERENCES accounts(id),
    -- The `shop_overlay_revision` the document was rendered at. A publish that completes tells
    -- the console exactly which overlay the players' clients are now showing, which is the only
    -- way to answer "is the client behind the server?" without re-hashing the archive.
    overlay_revision BIGINT NOT NULL,
    -- The rendered `catalog.json` the worker must author, stored rather than recomputed: an
    -- operator publishes what they reviewed, and a concurrent overlay edit must not silently
    -- change what ships.
    --
    -- TEXT, not JSONB, and that is load-bearing. `jsonb` normalises: it reorders keys, drops
    -- insignificant whitespace and rewrites numbers, so `document::text` would return different
    -- bytes than went in. The digest below is over the exact bytes the operator approved and the
    -- worker re-hashes what it writes to disk, so a normalising column would fail every publish.
    -- The cast in the CHECK still refuses anything that is not valid JSON.
    document TEXT NOT NULL CHECK (document::jsonb IS NOT NULL),
    document_sha256 BYTEA NOT NULL CHECK (octet_length(document_sha256) = 32),
    offer_count INTEGER NOT NULL CHECK (offer_count >= 0),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'published', 'failed')),
    -- Operator-facing outcome text. On failure this is the worker's reason; a failed publish
    -- that says nothing is indistinguishable from one that never ran.
    detail TEXT,
    -- What actually reached the client tree, recorded by the worker so the console can show the
    -- published archive without trusting a second source.
    client_pak_name TEXT,
    client_pak_sha256 BYTEA CHECK (client_pak_sha256 IS NULL OR octet_length(client_pak_sha256) = 32),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    CONSTRAINT ck_shop_publish_terminal_finished CHECK (
        (status IN ('published', 'failed')) = (finished_at IS NOT NULL)
    ),
    CONSTRAINT ck_shop_publish_started CHECK (
        (status = 'pending') = (started_at IS NULL)
    )
);

-- At most one request may be outstanding. Two workers authoring the same client tree would race
-- on the staged archive, and two queued requests would mean the second silently discards the
-- first operator's intent.
CREATE UNIQUE INDEX uq_shop_publish_active
    ON shop_publish_requests ((TRUE))
    WHERE status IN ('pending', 'running');

CREATE INDEX ix_shop_publish_requested_at
    ON shop_publish_requests (requested_at DESC);

-- The console's "is the client behind the server?" question is answered from the newest
-- successful publish, so that lookup gets its own index rather than a scan over every attempt.
CREATE INDEX ix_shop_publish_published
    ON shop_publish_requests (finished_at DESC)
    WHERE status = 'published';
