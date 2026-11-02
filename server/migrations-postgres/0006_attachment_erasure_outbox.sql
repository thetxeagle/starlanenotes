-- Additive lifecycle proposal after frozen capacity51/PG5. No FK/cascade or erasure grant.
CREATE TABLE attachment_erasure_outbox (
 operation_id TEXT PRIMARY KEY NOT NULL,
 actor_id TEXT NOT NULL,
 space_id TEXT NOT NULL,
 document_id TEXT NOT NULL,
 source_fingerprint TEXT NOT NULL CHECK(length(source_fingerprint)=64),
 plan_json TEXT NOT NULL,
 plan_sha256 TEXT NOT NULL CHECK(length(plan_sha256)=64),
 state TEXT NOT NULL CHECK(state='retained_review'),
 created_at TEXT NOT NULL
);
CREATE INDEX attachment_erasure_outbox_page ON attachment_erasure_outbox(space_id,document_id);
