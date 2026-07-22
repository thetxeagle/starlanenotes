import { createContext, useContext } from "react";

export interface User {
  id: string;
  email: string;
  username?: string;
  display_name: string;
  avatar_url?: string;
  role: string;
  totp_enabled: boolean;
}

export interface AuthConfig {
  google_enabled: boolean;
  github_enabled: boolean;
  oidc_enabled: boolean;
  oidc_display_name: string;
  require_2fa: boolean;
  allow_public_signups: boolean;
  setup_completed: boolean;
  site_name: string;
  max_upload_size_mb: number;
}

export interface AuthContextType {
  user: User | null;
  config: AuthConfig | null;
  loading: boolean;
  login: (email: string, password: string, totpCode?: string) => Promise<void>;
  loginWithPasskey: (email: string) => Promise<void>;
  register: (email: string, displayName: string, password: string) => Promise<void>;
  updateCurrentUser: (input: Partial<User>) => void;
  updateProfile: (input: { display_name: string; avatar_url?: string; username?: string }) => Promise<void>;
  logout: () => void;
}

export const AuthContext = createContext<AuthContextType | null>(null);

export function useAuth(): AuthContextType {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return context;
}
