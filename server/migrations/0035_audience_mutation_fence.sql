-- Audience topology writers acquire this caller-transaction fence BEFORE spaces.
-- Existing writers remain unprotected until explicitly adopted; no grant cleanup.
CREATE TABLE audience_mutation_fence (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1)
);
INSERT INTO audience_mutation_fence (id) VALUES (1);
