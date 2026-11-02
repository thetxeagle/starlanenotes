-- Dormant direct-user, page-only viewer foundation. No grant management route.
-- Restrict deletion/relocation until consumed cascade/source witnesses are adopted.
CREATE UNIQUE INDEX documents_direct_grant_binding ON documents(id,space_id);
CREATE TABLE document_viewer_grants (
 id TEXT PRIMARY KEY NOT NULL CHECK(length(trim(id)) > 0),
 document_id TEXT NOT NULL,
 space_id TEXT NOT NULL,
 user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
 role TEXT NOT NULL CHECK(role = 'viewer'),
 scope TEXT NOT NULL CHECK(scope = 'page'),
 revision TEXT NOT NULL CHECK(length(trim(revision)) > 0),
 created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL,
 UNIQUE(document_id,user_id),
 FOREIGN KEY(document_id,space_id) REFERENCES documents(id,space_id) ON DELETE RESTRICT,
 FOREIGN KEY(space_id) REFERENCES spaces(id) ON DELETE RESTRICT
);
CREATE INDEX document_viewer_grants_recipient ON document_viewer_grants(user_id,document_id);
