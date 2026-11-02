-- Historical actor/request identity only. No FKs: deletion cannot authorize rebirth.
CREATE TABLE draft_birth_receipts (
 actor_id TEXT NOT NULL,
 operation_id TEXT NOT NULL,
 space_id TEXT NOT NULL,
 document_id TEXT NOT NULL,
 request_fingerprint TEXT NOT NULL,
 draft_id TEXT NOT NULL,
 result_digest TEXT NOT NULL,
 result_json TEXT NOT NULL,
 PRIMARY KEY (actor_id, operation_id)
);
