-- Additive audience provenance. No cascading foreign keys or inferred grants.
-- Existing records retain declared page-owned compatibility, including retained
-- immutable versions whose active association has already been removed.
CREATE TABLE attachment_audience_origins (
    asset_id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('legacy_page_v1','draft_upload_v2','draft_replay_v2')),
    original_space_id TEXT NOT NULL,
    original_document_id TEXT NOT NULL,
    draft_id TEXT,
    draft_created_at TEXT,
    actor_id TEXT,
    operation_id TEXT,
    scope_json TEXT,
    scope_sha256 TEXT,
    CHECK ((kind='legacy_page_v1' AND draft_id IS NULL AND draft_created_at IS NULL AND actor_id IS NULL AND operation_id IS NULL AND scope_json IS NULL AND scope_sha256 IS NULL) OR (kind IN ('draft_upload_v2','draft_replay_v2') AND draft_id IS NOT NULL AND draft_created_at IS NOT NULL AND actor_id IS NOT NULL AND operation_id IS NOT NULL AND scope_json IS NOT NULL AND scope_sha256 IS NOT NULL))
);
CREATE TABLE attachment_audience_bindings (
    asset_id TEXT NOT NULL,
    binding_kind TEXT NOT NULL CHECK (binding_kind IN ('draft','publication')),
    target_id TEXT NOT NULL,
    target_created_at TEXT NOT NULL,
    source_operation_id TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL,
    PRIMARY KEY (asset_id,binding_kind,target_id)
);
INSERT INTO attachment_audience_origins(asset_id,kind,original_space_id,original_document_id)
SELECT id,'legacy_page_v1',space_id,document_id FROM attachments;
INSERT INTO attachment_audience_origins(asset_id,kind,original_space_id,original_document_id)
SELECT version_id,'legacy_page_v1',original_space_id,original_document_id
FROM attachment_version_records v
WHERE NOT EXISTS (SELECT 1 FROM attachment_audience_origins o WHERE o.asset_id=v.version_id);

CREATE INDEX attachment_audience_origins_document ON attachment_audience_origins(original_document_id);
CREATE INDEX attachment_audience_bindings_operation ON attachment_audience_bindings(source_operation_id);
