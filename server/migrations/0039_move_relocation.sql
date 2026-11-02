-- Same-transaction relocation evidence; retained after later deletion/supersession.
-- No source/canonical foreign keys: historical evidence is never replay authority.
CREATE TABLE move_commit_receipts (
 op_id TEXT PRIMARY KEY NOT NULL, preview_id TEXT NOT NULL UNIQUE,
 actor_id TEXT NOT NULL, source_space_id TEXT NOT NULL, destination_space_id TEXT NOT NULL,
 request_fingerprint TEXT NOT NULL, source_fingerprint TEXT NOT NULL,
 manifest_json TEXT NOT NULL, expected_state_json TEXT NOT NULL,
 created_at TEXT NOT NULL, verified_at TEXT
);
CREATE TABLE move_metadata_escrow (
 op_id TEXT NOT NULL REFERENCES move_commit_receipts(op_id),
 document_id TEXT NOT NULL, family TEXT NOT NULL, source_id TEXT NOT NULL,
 disposition TEXT NOT NULL CHECK(disposition IN ('preserve','remove','map','create')),
 original_json TEXT NOT NULL, expected_json TEXT NOT NULL,
 PRIMARY KEY(op_id,document_id,family,source_id)
);
-- An old body URL resolves by stable asset ID to its CURRENT scoped association.
-- Aliases grant no access, never point to another alias and survive repeated moves.
CREATE TABLE move_asset_path_aliases (
 old_path TEXT PRIMARY KEY NOT NULL, asset_id TEXT NOT NULL,
 asset_kind TEXT NOT NULL CHECK(asset_kind IN ('attachment','drawing')),
 op_id TEXT NOT NULL REFERENCES move_commit_receipts(op_id), created_at TEXT NOT NULL
);
