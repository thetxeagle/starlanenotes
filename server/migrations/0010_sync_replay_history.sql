-- Sync replay manifest history
-- Records authenticated replay runs and recovery checkpoints for operator recovery UX.

CREATE TABLE IF NOT EXISTS sync_replay_manifests (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    status TEXT NOT NULL,
    attempted_by TEXT NOT NULL REFERENCES users(id),
    operation_count INTEGER NOT NULL,
    attempted_operation_count INTEGER NOT NULL,
    applied_operation_count INTEGER NOT NULL,
    skipped_operation_count INTEGER NOT NULL,
    failed_operation_count INTEGER NOT NULL,
    unsupported_operation_count INTEGER NOT NULL,
    recovery_checkpoint_count INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sync_replay_manifests_attempted_by
    ON sync_replay_manifests(attempted_by);

CREATE INDEX IF NOT EXISTS idx_sync_replay_manifests_created_at
    ON sync_replay_manifests(created_at);

CREATE TABLE IF NOT EXISTS sync_replay_checkpoints (
    id TEXT PRIMARY KEY NOT NULL,
    manifest_id TEXT NOT NULL REFERENCES sync_replay_manifests(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    attempted_by TEXT NOT NULL REFERENCES users(id),
    space_id TEXT,
    source_server_url TEXT,
    downloaded_by_user_id TEXT,
    checkpoint_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sync_replay_checkpoints_manifest_id
    ON sync_replay_checkpoints(manifest_id);

CREATE INDEX IF NOT EXISTS idx_sync_replay_checkpoints_space
    ON sync_replay_checkpoints(space_id, provider);
