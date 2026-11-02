CREATE TABLE attachment_capacity_lock (
    id BIGINT PRIMARY KEY CHECK (id = 1),
    revision BIGINT NOT NULL CHECK (revision > 0)
);
INSERT INTO attachment_capacity_lock (id, revision) VALUES (1, 1);
CREATE TABLE attachment_capacity_reservations (
    operation_id TEXT PRIMARY KEY,
    request_sha256 TEXT NOT NULL,
    admission_claim_sha256 TEXT NOT NULL,
    maximum_bytes BIGINT NOT NULL CHECK (maximum_bytes >= 0),
    state TEXT NOT NULL CHECK (state IN ('reserved', 'committed', 'disposed')),
    committed_asset_id TEXT,
    committed_bytes BIGINT,
    disposition_json TEXT,
    disposition_sha256 TEXT,
    CHECK (
        (state = 'reserved' AND committed_asset_id IS NULL AND committed_bytes IS NULL
         AND disposition_json IS NULL AND disposition_sha256 IS NULL)
        OR
        (state = 'committed' AND committed_asset_id IS NOT NULL AND committed_bytes IS NOT NULL
         AND committed_bytes >= 0 AND committed_bytes <= maximum_bytes
         AND disposition_json IS NULL AND disposition_sha256 IS NULL)
        OR
        (state = 'disposed' AND committed_asset_id IS NULL AND committed_bytes IS NULL
         AND disposition_json IS NOT NULL AND disposition_sha256 IS NOT NULL)
    )
);
