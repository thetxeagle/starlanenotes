-- SCIM identity-provider correlation. A deployment has one configured SCIM bearer token,
-- so external IDs are unique across the local user directory while remaining optional.

ALTER TABLE users ADD COLUMN external_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_external_id
    ON users(external_id)
    WHERE external_id IS NOT NULL;
