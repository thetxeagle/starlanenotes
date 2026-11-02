-- Immutable mapping from one reviewed local draft transfer operation to its
-- actual private server draft. No foreign keys: page or draft deletion must
-- not erase or authorize replay of historical transfer evidence.
CREATE TABLE draft_transfer_receipts (
    actor_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    draft_created_at TEXT NOT NULL,
    source_digest TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (to_char(statement_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')),
    PRIMARY KEY (actor_id, operation_id)
);
