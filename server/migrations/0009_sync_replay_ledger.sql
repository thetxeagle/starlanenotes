-- Sync replay idempotency ledger
-- Records queued local operation replay attempts before any mutating replay execution.

CREATE TABLE IF NOT EXISTS sync_replay_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    status TEXT NOT NULL,
    attempted_by TEXT NOT NULL REFERENCES users(id),
    attempted_at TEXT NOT NULL,
    completed_at TEXT,
    error TEXT,
    payload TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_replay_operations_status
    ON sync_replay_operations(status);

CREATE INDEX IF NOT EXISTS idx_sync_replay_operations_attempted_by
    ON sync_replay_operations(attempted_by);
