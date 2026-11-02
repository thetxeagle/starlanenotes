-- Route-owned immutable confirmation evidence; survives draft consumption and page deletion.
-- No source foreign keys and no changes to the generic publication journal contract.
CREATE TABLE draft_publication_receipts (
    operation_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL,
    request_json TEXT NOT NULL,
    draft_json TEXT NOT NULL,
    result_page_json TEXT NOT NULL,
    response_json TEXT NOT NULL,
    storage_manifest_json TEXT NOT NULL
);
CREATE INDEX draft_publication_receipts_document ON draft_publication_receipts(document_id);
