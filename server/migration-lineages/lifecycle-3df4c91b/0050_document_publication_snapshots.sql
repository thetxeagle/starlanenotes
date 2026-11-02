-- DB-owned immutable payloads; no cleanup-cascading document/space/user FK.
CREATE TABLE document_publication_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL,
    captured_space_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('publication','baseline')),
    capture_operation_id TEXT NOT NULL,
    publication_operation_id TEXT,
    binding TEXT NOT NULL CHECK (binding IN ('capture_v2','legacy_content_v1')),
    payload_json TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    raw_markdown BLOB NOT NULL,
    raw_sha256 TEXT NOT NULL,
    raw_byte_length BIGINT NOT NULL CHECK (raw_byte_length >= 0),
    published_at TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    captured_by TEXT NOT NULL,
    CHECK ((kind='publication' AND publication_operation_id=capture_operation_id) OR
           (kind='baseline' AND publication_operation_id IS NULL))
);
CREATE UNIQUE INDEX document_publication_snapshots_operation
    ON document_publication_snapshots(publication_operation_id);
CREATE UNIQUE INDEX document_publication_snapshots_baseline
    ON document_publication_snapshots(document_id) WHERE kind='baseline';
CREATE INDEX document_publication_snapshots_document
    ON document_publication_snapshots(document_id,captured_at,id);
