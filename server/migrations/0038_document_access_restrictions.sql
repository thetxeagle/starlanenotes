-- Dormant restriction foundation: no management, page-grant or guest API.
CREATE TABLE document_access_policies (
 document_id TEXT PRIMARY KEY NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
 space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
 mode TEXT NOT NULL CHECK(mode IN ('inherited','restricted')),
 revision TEXT NOT NULL,
 created_by TEXT NOT NULL REFERENCES users(id),
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 UNIQUE(document_id,space_id)
);
CREATE TABLE document_access_allowlist (
 id TEXT PRIMARY KEY NOT NULL,
 document_id TEXT NOT NULL,
 space_id TEXT NOT NULL,
 user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
 group_id TEXT REFERENCES groups(id) ON DELETE CASCADE,
 created_by TEXT NOT NULL REFERENCES users(id),
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 CHECK ((user_id IS NOT NULL AND group_id IS NULL) OR (user_id IS NULL AND group_id IS NOT NULL)),
 FOREIGN KEY(document_id,space_id) REFERENCES document_access_policies(document_id,space_id) ON DELETE CASCADE
);
CREATE INDEX document_access_allowlist_page ON document_access_allowlist(document_id,space_id);
CREATE UNIQUE INDEX document_access_allowlist_user ON document_access_allowlist(document_id,user_id);
CREATE UNIQUE INDEX document_access_allowlist_group ON document_access_allowlist(document_id,group_id);
