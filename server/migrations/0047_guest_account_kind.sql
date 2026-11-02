-- Existing users retain identities, associations and resources. Guest authority is
-- resolved from this current classification, never from a JWT or role label.
ALTER TABLE users ADD COLUMN account_kind TEXT NOT NULL DEFAULT 'member'
 CHECK(account_kind IN ('member','guest'));
