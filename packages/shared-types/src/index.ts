// Shared type definitions between web app and server API

export type UserRole = "admin" | "member" | "viewer";

export type SpaceVisibility = "private" | "internal" | "public";

export type SpaceMemberRole = "owner" | "editor" | "viewer";

export type ThemeMode = "light" | "dark" | "system";

export interface User {
  id: string;
  email: string;
  display_name: string;
  avatar_url?: string;
  role: UserRole;
  created_at: string;
  updated_at: string;
}

export interface Space {
  id: string;
  name: string;
  slug: string;
  description?: string;
  icon?: string;
  accent_color?: string;
  visibility: SpaceVisibility;
  created_by: string;
  owner_username: string;
  owner_display_name: string;
  created_at: string;
  updated_at: string;
}

export interface Document {
  human_identifier?: string | null;
  maintenance_user_id?: string | null;
  maintenance_group_id?: string | null;
  id: string;
  space_id: string;
  folder_id?: string;
  parent_id?: string;
  title: string;
  icon?: string;
  cover_image?: string;
  locked: boolean;
  public: boolean;
  public_slug?: string;
  created_by: string;
  last_updated_by: string;
  created_at: string;
  /** Exact original Trash cycle witness returned only while the page is trashed. */
  deleted_at?: string;
  updated_at: string;
}

export interface Folder {
  id: string;
  space_id: string;
  parent_id?: string;
  name: string;
  icon?: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export interface Tag {
  id: string;
  space_id: string;
  name: string;
  color: string;
}

export interface ApiError {
  error: string;
  message: string;
  status: number;
}

export * from "./reviewedMoves";

/** Direct published-page viewer scope does not confer space membership. */
export interface PageViewerProposal { user_id: string; action: "grant" | "revoke" }
export interface PageViewerInspection {
  schema_version: 1; space_id: string; document_id: string; scope_notice: string;
  grants: { user_id: string; revision: string; role: "viewer"; scope: "page"; display_name: string | null; available: boolean }[];
}
export interface PageViewerReview {
  schema_version: 1; space_id: string; document_id: string; proposal: PageViewerProposal; source_digest: string;
  recipient: { id: string; display_name: string | null; email: string | null }; existing_grant: Record<string, unknown> | null;
}
export interface PageViewerConfirmed {
  schema_version: 1; space_id: string; document_id: string; user_id: string; action: "grant" | "revoke";
  source_digest: string; confirmed: true; grant: Record<string, unknown> | null;
}
export interface SharedPagePresentation {
  id: string; space_id: string; title: string; published_at: string; updated_at: string;
  human_identifier: string | null; icon: string | null;
}
export interface SharedPagesInspection { pages: SharedPagePresentation[]; next_cursor: string | null }

/** Presentation captured from the same published row/body as published_source_digest. */
export interface PublishedDocumentMetadata { title: string; icon: string | null; cover_image: string | null; }
