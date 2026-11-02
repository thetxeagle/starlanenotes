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

-- Accounting evidence only. No FK cascade, serving, installation or erasure authority.
-- Snapshot active legacy/current rows in this migration transaction before page deletion
-- can erase their byte attribution. original_storage_path is the frozen accounting
-- observation at upgrade, not fabricated original upload/relocation provenance.
CREATE TABLE attachment_retained_byte_ledger (
    asset_id TEXT PRIMARY KEY NOT NULL,
    byte_length BIGINT NOT NULL CHECK (byte_length >= 0),
    filename TEXT NOT NULL,
    original_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    uploader_id TEXT NOT NULL,
    original_created_at TEXT NOT NULL,
    original_storage_path TEXT NOT NULL,
    evidence_kind TEXT NOT NULL CHECK (evidence_kind IN ('legacy_upgrade', 'direct', 'replay')),
    original_receipt_json TEXT,
    original_receipt_sha256 TEXT,
    CHECK (
        (evidence_kind = 'legacy_upgrade' AND original_receipt_json IS NULL AND original_receipt_sha256 IS NULL)
        OR
        (evidence_kind IN ('direct', 'replay') AND original_receipt_json IS NOT NULL
         AND original_receipt_sha256 IS NOT NULL AND length(original_receipt_sha256) = 64)
    )
);
INSERT INTO attachment_retained_byte_ledger
    (asset_id,byte_length,filename,original_name,mime_type,uploader_id,original_created_at,
     original_storage_path,evidence_kind,original_receipt_json,original_receipt_sha256)
SELECT id,size_bytes,filename,original_name,mime_type,created_by,created_at,storage_path,
       'legacy_upgrade',NULL,NULL FROM attachments;
