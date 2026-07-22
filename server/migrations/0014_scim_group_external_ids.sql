-- Immutable identity correlation for SCIM-provisioned groups.
ALTER TABLE groups ADD COLUMN external_id TEXT;
ALTER TABLE groups ADD COLUMN scim_managed INTEGER NOT NULL DEFAULT 0;

CREATE UNIQUE INDEX IF NOT EXISTS idx_groups_external_id
    ON groups(external_id)
    WHERE external_id IS NOT NULL;
