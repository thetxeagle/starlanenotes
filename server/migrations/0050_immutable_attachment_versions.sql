-- Additive immutable attachment registry and explicit retained deletion state.
-- Existing attachments.id remains the immutable version identifier.
-- No cascading foreign keys: original metadata/evidence survives association deletion.
CREATE TABLE attachment_families (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    current_version_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    state TEXT NOT NULL CHECK (state = 'active'),
    created_at TEXT NOT NULL
);
CREATE TABLE attachment_version_records (
    version_id TEXT PRIMARY KEY NOT NULL,
    logical_attachment_id TEXT NOT NULL,
    version_number INTEGER NOT NULL CHECK (version_number > 0),
    original_space_id TEXT NOT NULL,
    original_document_id TEXT NOT NULL,
    uploader_id TEXT NOT NULL,
    original_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    binary_sha256 TEXT NOT NULL CHECK (length(binary_sha256)=64 AND binary_sha256 NOT GLOB '*[^0-9a-f]*'),
    original_created_at TEXT NOT NULL,
    original_storage_path TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    upload_operation_id TEXT NOT NULL UNIQUE,
    upload_attempt_id TEXT NOT NULL UNIQUE,
    raw_receipt_sha256 TEXT NOT NULL,
    intent_sha256 TEXT NOT NULL,
    install_proof_sha256 TEXT NOT NULL,
    provenance TEXT NOT NULL CHECK (provenance='direct_upload_v1'),
    UNIQUE(logical_attachment_id, version_number)
);
CREATE INDEX idx_attachment_families_page ON attachment_families(space_id,document_id);
CREATE INDEX idx_attachment_versions_family ON attachment_version_records(logical_attachment_id,version_number);

-- Unnumbered additive SQLite proposal. Immutable family transitions; no aliases/deletion.
CREATE TABLE attachment_version_changes (
 operation_id TEXT PRIMARY KEY NOT NULL,
 family_id TEXT NOT NULL,
 expected_version_id TEXT NOT NULL,
 expected_revision INTEGER NOT NULL CHECK(expected_revision>0),
 new_version_id TEXT NOT NULL UNIQUE,
 applied_revision INTEGER NOT NULL CHECK(applied_revision=expected_revision+1),
 actor_id TEXT NOT NULL,
 origin TEXT NOT NULL,
 space_id TEXT NOT NULL,
 document_id TEXT NOT NULL,
 upload_receipt_sha256 TEXT NOT NULL,
 changed_at TEXT NOT NULL,
 receipt_json TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_attachment_changes_revision ON attachment_version_changes(family_id,applied_revision);

-- Logical deletion retains original SQL attribution and immutable version evidence.
-- No cascades/backfill: a legacy row remains legacy; historical uploader/time are not synthesized.
CREATE TABLE attachment_retirements (
    attachment_id TEXT PRIMARY KEY NOT NULL,
    state TEXT NOT NULL CHECK (state='retained'),
    actor_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('direct','replay')),
    operation_id TEXT NOT NULL,
    retired_at TEXT NOT NULL,
    association_json TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    UNIQUE(origin,operation_id)
);
CREATE INDEX idx_attachment_retirements_page ON attachment_retirements(space_id,document_id);
