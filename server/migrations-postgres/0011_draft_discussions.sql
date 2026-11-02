-- Private selected-draft discussion. These rows deliberately do not reference
-- document_drafts: publication consumes that row while retaining discussion.
CREATE TABLE draft_discussions (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    draft_id TEXT NOT NULL UNIQUE,
    draft_created_at TEXT NOT NULL,
    draft_owner_id TEXT NOT NULL,
    current_source_digest TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active', 'closed')),
    publication_operation_id TEXT,
    closed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((state = 'active' AND publication_operation_id IS NULL AND closed_at IS NULL)
        OR (state = 'closed' AND publication_operation_id IS NOT NULL AND closed_at IS NOT NULL))
);

CREATE INDEX idx_draft_discussions_document ON draft_discussions(space_id, document_id);

CREATE TABLE draft_discussion_participants (
    discussion_id TEXT NOT NULL REFERENCES draft_discussions(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    participation TEXT NOT NULL CHECK (participation IN ('owner', 'viewer', 'editor')),
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    PRIMARY KEY (discussion_id, user_id)
);

CREATE TABLE draft_discussion_comments (
    id TEXT PRIMARY KEY NOT NULL,
    discussion_id TEXT NOT NULL REFERENCES draft_discussions(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES draft_discussion_comments(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    content TEXT NOT NULL CHECK (char_length(content) BETWEEN 1 AND 10000),
    source_digest TEXT NOT NULL,
    selection_start BIGINT,
    selection_end BIGINT,
    selection_text TEXT,
    resolved BIGINT NOT NULL DEFAULT 0 CHECK (resolved IN (0, 1)),
    resolved_by TEXT,
    resolved_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((selection_start IS NULL AND selection_end IS NULL AND selection_text IS NULL)
        OR (selection_start IS NOT NULL AND selection_end IS NOT NULL AND selection_text IS NOT NULL
            AND selection_start >= 0 AND selection_end > selection_start))
);

CREATE INDEX idx_draft_discussion_comments_context
    ON draft_discussion_comments(discussion_id, created_at);

CREATE TABLE draft_discussion_subscriptions (
    discussion_id TEXT NOT NULL REFERENCES draft_discussions(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (discussion_id, user_id)
);
