-- Separate NEXT proposal only; frozen 0052 retention plan is immutable.
-- A disposition row is accounting/provenance evidence, never an unlink/serving grant.
CREATE TABLE attachment_erasure_dispositions (
 deletion_operation_id TEXT NOT NULL,
 asset_id TEXT NOT NULL,
 actor_id TEXT NOT NULL,
 space_id TEXT NOT NULL,
 document_id TEXT NOT NULL,
 producer_kind TEXT NOT NULL CHECK (producer_kind='original_direct_v1'),
 original_attempt_id TEXT NOT NULL,
 plan_sha256 TEXT NOT NULL CHECK (length(plan_sha256)=64),
 original_receipt_sha256 TEXT NOT NULL CHECK (length(original_receipt_sha256)=64),
 admission_claim_sha256 TEXT NOT NULL CHECK (length(admission_claim_sha256)=64),
 native_intent_json TEXT NOT NULL CHECK (length(native_intent_json)<=16384),
 native_intent_sha256 TEXT NOT NULL CHECK (length(native_intent_sha256)=64),
 native_tombstone_json TEXT NOT NULL CHECK (native_tombstone_json=native_intent_json),
 native_tombstone_sha256 TEXT NOT NULL CHECK (native_tombstone_sha256=native_intent_sha256),
 owned_path_set_json TEXT NOT NULL CHECK (length(owned_path_set_json)<=65536),
 owned_path_set_sha256 TEXT NOT NULL CHECK (length(owned_path_set_sha256)=64),
 disposition TEXT NOT NULL CHECK (disposition='owned_paths_erased_v1'),
 completed_at TEXT NOT NULL,
 PRIMARY KEY (deletion_operation_id,asset_id),
 UNIQUE (asset_id)
);
