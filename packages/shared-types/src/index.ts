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
