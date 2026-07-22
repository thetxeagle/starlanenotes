export interface SpaceLabelRecord {
  id: string;
  name: string;
  slug: string;
  owner_username?: string;
  owner_display_name?: string;
}

function hasStableUsername(value: string): boolean {
  return /^[a-zA-Z0-9_-]{1,64}$/.test(value);
}

export function getSpaceOwnerIdentity(space: SpaceLabelRecord): string | null {
  const ownerUsername = space.owner_username?.trim();
  const ownerDisplayName = space.owner_display_name?.trim();

  if (ownerUsername && hasStableUsername(ownerUsername)) {
    return `@${ownerUsername}`;
  }

  return ownerDisplayName || ownerUsername || null;
}

export function isPersonalSpaceLabelCandidate(space: SpaceLabelRecord): boolean {
  const normalizedName = space.name.trim().toLowerCase();
  const normalizedSlug = space.slug.trim().toLowerCase();

  return (
    space.id === "local-space" ||
    normalizedName === "personal space" ||
    normalizedName.endsWith("'s space") ||
    normalizedSlug === "personal-space" ||
    normalizedSlug.startsWith("personal-")
  );
}

export function getPersonalSpaceOwnerLabel(space: SpaceLabelRecord): string | null {
  if (!isPersonalSpaceLabelCandidate(space)) return null;

  const ownerIdentity = getSpaceOwnerIdentity(space);
  return ownerIdentity ? `Personal space · ${ownerIdentity}` : "Personal space";
}
