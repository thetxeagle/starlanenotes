export type SpaceRoleId = "space_admin" | "editor" | "commenter" | "viewer";

export type SpacePermissionId =
  | "view"
  | "comment"
  | "edit_content"
  | "manage_members"
  | "manage_space";

export interface SpaceRoleDefinition {
  id: string;
  name: string;
  description: string;
  permissions: SpacePermissionId[];
  built_in?: boolean;
}

export const SPACE_PERMISSION_LABELS: Record<SpacePermissionId, string> = {
  view: "View space",
  comment: "Comment",
  edit_content: "Edit content",
  manage_members: "Manage members",
  manage_space: "Manage space",
};

export const DEFAULT_SPACE_ROLE_DEFINITIONS: SpaceRoleDefinition[] = [
  {
    id: "space_admin",
    name: "Space admin",
    description: "Manage space settings, access, and content.",
    permissions: ["view", "comment", "edit_content", "manage_members", "manage_space"],
  },
  {
    id: "editor",
    name: "Editor",
    description: "Create, edit, organize, and comment on content.",
    permissions: ["view", "comment", "edit_content"],
  },
  {
    id: "commenter",
    name: "Commenter",
    description: "View content and participate in comments.",
    permissions: ["view", "comment"],
  },
  {
    id: "viewer",
    name: "Viewer",
    description: "View space content without making changes.",
    permissions: ["view"],
  },
];

export function normalizeSpaceRole(role: string): SpaceRoleId {
  return role === "owner" ? "space_admin" : (role as SpaceRoleId);
}

export function getSpaceRoleLabel(
  role: string,
  definitions: SpaceRoleDefinition[] = DEFAULT_SPACE_ROLE_DEFINITIONS,
): string {
  if (role === "owner") return "Owner";
  return definitions.find((definition) => definition.id === role)?.name ?? role;
}
