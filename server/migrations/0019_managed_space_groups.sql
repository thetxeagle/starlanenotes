-- Give every space a first-class, application-managed collaboration group.
--
-- Existing groups remain reusable across spaces. Managed groups cannot be
-- detached from their owning space or deleted independently.

ALTER TABLE groups ADD COLUMN managed_space_id TEXT REFERENCES spaces(id) ON DELETE CASCADE;

CREATE UNIQUE INDEX IF NOT EXISTS idx_groups_managed_space_id
    ON groups(managed_space_id)
    WHERE managed_space_id IS NOT NULL;
