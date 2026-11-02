-- Display is optional; normalized identity is server-derived and never a page UUID replacement.
ALTER TABLE documents ADD COLUMN human_identifier TEXT;
ALTER TABLE documents ADD COLUMN human_identifier_key TEXT CHECK (
 (human_identifier IS NULL AND human_identifier_key IS NULL) OR
 (human_identifier IS NOT NULL AND human_identifier_key IS NOT NULL AND human_identifier_key=UPPER(human_identifier))
);
CREATE UNIQUE INDEX documents_space_identifier ON documents(space_id,human_identifier_key);
-- Historical incoming evidence survives page/space removal; no disposable-source FK.
CREATE TABLE document_identifier_receipts (
 provider TEXT NOT NULL,incoming_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 document_id TEXT NOT NULL,space_id TEXT NOT NULL,payload_fingerprint TEXT NOT NULL,
 receipt_json TEXT NOT NULL,PRIMARY KEY(provider,incoming_id)
);
