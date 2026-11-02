-- Append restoration generations; original journal receipts and migrations remain immutable.
CREATE TABLE storage_restore_generations (
    generation_id TEXT PRIMARY KEY,
    backup_id TEXT NOT NULL,
    manifest_sha256 TEXT NOT NULL,
    database_sha256 TEXT NOT NULL,
    ledger_sha256 TEXT NOT NULL,
    root_device TEXT NOT NULL,
    root_inode TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'committed', 'needs_review')),
    created_at TEXT NOT NULL
);
CREATE TABLE storage_restore_current (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    generation_id TEXT NOT NULL REFERENCES storage_restore_generations(generation_id)
);
