import { useCallback, useEffect, useState, type ReactNode } from "react";
import { api, getSessionClientDetails } from "@/lib/api";
import { dataClient } from "@/lib/dataClient";
import { getPasskeyCredential } from "@/lib/passkeys";
import { completeNativeAuthHandoffInBrowser, finishNativeAuthHandoff, rememberBrowserNativeAuthHandoff } from "@/lib/nativeAuthHandoff";
import { isPersonalSpaceLabelCandidate } from "@/lib/spaceLabels";
import { AuthContext, type AuthConfig, type User } from "@/hooks/useAuth";
import type { AppLaunchMode } from "@/lib/launchMode";

const LOCAL_MODE_CONFIG: AuthConfig = {
  google_enabled: false,
  github_enabled: false,
  oidc_enabled: false,
  oidc_display_name: "",
  require_2fa: false,
  allow_public_signups: false,
  setup_completed: true,
  site_name: "Starlane Notes",
  max_upload_size_mb: 30,
};

const LOCAL_MODE_USER: User = {
  id: "local-user",
  email: "local@starlane.notes",
  display_name: "Local User",
  role: "owner",
  totp_enabled: false,
};

interface AuthProviderProps {
  children: ReactNode;
  mode?: AppLaunchMode;
}

export interface AuthCallbackTokens {
  accessToken: string;
  refreshToken: string;
}

export interface ParsedAuthCallbackFragment {
  hasAuthParams: boolean;
  tokens: AuthCallbackTokens | null;
  sanitizedHash: string;
}

export function parseAuthCallbackFragment(hash: string): ParsedAuthCallbackFragment {
  const rawFragment = hash.replace(/^#/, "");
  if (!rawFragment) return { hasAuthParams: false, tokens: null, sanitizedHash: "" };

  const params = new URLSearchParams(rawFragment);
  const accessToken = params.get("access_token")?.trim() ?? "";
  const refreshToken = params.get("refresh_token")?.trim() ?? "";
  const hasAuthParams = params.has("access_token") || params.has("refresh_token");

  params.delete("access_token");
  params.delete("refresh_token");
  const remaining = params.toString();

  return {
    hasAuthParams,
    tokens: accessToken && refreshToken ? { accessToken, refreshToken } : null,
    sanitizedHash: remaining ? `#${remaining}` : "",
  };
}

export function buildAuthCallbackCleanUrl(
  location: Pick<Location, "pathname" | "search">,
  sanitizedHash: string,
): string {
  return `${location.pathname}${location.search}${sanitizedHash}`;
}

export function mergeAuthUserUpdate(current: User | null, input: Partial<User>): User | null {
  return current ? { ...current, ...input } : current;
}

interface DownloadablePersonalSpace {
  id: string;
  name: string;
  slug: string;
  created_by?: string;
}

/** Downloads the personal-space index after sign-in; page bodies cache when opened. */
export async function downloadPersonalSpaceIndex(user: Pick<User, "id">): Promise<void> {
  const spaces = await api.fetch<DownloadablePersonalSpace[]>("/spaces");
  const personalSpace = spaces.find(
    (space) =>
      space.created_by === user.id &&
      isPersonalSpaceLabelCandidate({ id: space.id, name: space.name, slug: space.slug }),
  );
  if (!personalSpace) return;

  await Promise.all([
    api.fetch(`/spaces/${personalSpace.id}/folders`),
    api.fetch(`/spaces/${personalSpace.id}/documents`),
  ]);
}

export function AuthProvider({ children, mode = "server" }: AuthProviderProps) {
  const [user, setUser] = useState<User | null>(mode === "local" ? LOCAL_MODE_USER : null);
  const [config, setConfig] = useState<AuthConfig | null>(
    mode === "local" ? LOCAL_MODE_CONFIG : null,
  );
  const [loading, setLoading] = useState(mode !== "local");

  useEffect(() => {
    if (mode === "server") rememberBrowserNativeAuthHandoff();
  }, [mode]);

  // Load auth config (public endpoint)
  useEffect(() => {
    if (mode !== "server") return;
    api
      .fetch<AuthConfig>("/auth/config")
      .then(setConfig)
      .catch(() => {});
  }, [mode]);

  useEffect(() => {
    if (mode !== "server" || !user) return;
    void completeNativeAuthHandoffInBrowser()?.catch(() => undefined);
  }, [mode, user]);

  useEffect(() => {
    const complete = (event: Event) => {
      const url = (event as CustomEvent<string>).detail;
      void finishNativeAuthHandoff(url).then((data) => {
        const result = data as { access_token: string; refresh_token: string; user: User };
        api.setToken(result.access_token); api.setRefreshToken(result.refresh_token); setUser(result.user);
      }).catch(() => undefined);
    };
    document.addEventListener("starlane:native-auth-handoff", complete);
    return () => document.removeEventListener("starlane:native-auth-handoff", complete);
  }, []);

  // Check for existing session
  useEffect(() => {
    if (mode !== "server") {
      setLoading(false);
      return;
    }

    if (!api.isAuthenticated()) {
      setLoading(false);
      return;
    }

    api
      .fetch<User>("/auth/me")
      .then((nextUser) => {
        setUser(nextUser);
        void downloadPersonalSpaceIndex(nextUser).catch(() => undefined);
      })
      .catch(() => api.logout())
      .finally(() => setLoading(false));
  }, [mode]);

  // Handle OAuth callback tokens in URL fragment (not query string — more secure)
  useEffect(() => {
    if (mode !== "server") return;
    const callback = parseAuthCallbackFragment(window.location.hash);
    if (!callback.hasAuthParams) return;

    window.history.replaceState(
      {},
      "",
      buildAuthCallbackCleanUrl(window.location, callback.sanitizedHash),
    );

    if (callback.tokens) {
      api.setToken(callback.tokens.accessToken);
      api.setRefreshToken(callback.tokens.refreshToken);
      // Clean URL
      // Fetch user
      api
        .fetch<User>("/auth/me")
        .then((nextUser) => {
          setUser(nextUser);
          void downloadPersonalSpaceIndex(nextUser).catch(() => undefined);
        })
        .catch(() => {
          api.logout();
          setUser(null);
        })
        .finally(() => setLoading(false));
    } else {
      api.logout();
      setUser(null);
      setLoading(false);
    }
  }, [mode]);

  const login = useCallback(
    async (email: string, password: string, totpCode?: string) => {
      if (mode !== "server") {
        throw new Error("Login is unavailable in local mode");
      }

      const data = await api.fetch<{
        access_token: string;
        refresh_token: string;
        user: User;
      }>("/auth/login", {
        method: "POST",
        body: { email, password, totp_code: totpCode, ...getSessionClientDetails() },
      });

      api.setToken(data.access_token);
      api.setRefreshToken(data.refresh_token);
      setUser(data.user);
      void downloadPersonalSpaceIndex(data.user).catch(() => undefined);
    },
    [mode],
  );

  const loginWithPasskey = useCallback(
    async (email: string) => {
      if (mode !== "server") {
        throw new Error("Passkey sign-in is unavailable in local mode");
      }

      const challenge = await dataClient.startPasskeyLogin(email);
      const credential = await getPasskeyCredential(challenge.public_key);
      const data = await dataClient.finishPasskeyLogin(
        challenge.challenge_id,
        credential,
        getSessionClientDetails(),
      );

      api.setToken(data.access_token);
      api.setRefreshToken(data.refresh_token);
      setUser({
        ...data.user,
        username: data.user.username ?? undefined,
        avatar_url: data.user.avatar_url ?? undefined,
      });
      void downloadPersonalSpaceIndex(data.user).catch(() => undefined);
    },
    [mode],
  );

  const register = useCallback(
    async (email: string, displayName: string, password: string) => {
      if (mode !== "server") {
        throw new Error("Registration is unavailable in local mode");
      }

      const data = await api.fetch<{
        access_token: string;
        refresh_token: string;
        user: User;
      }>("/auth/register", {
        method: "POST",
        body: { email, display_name: displayName, password, ...getSessionClientDetails() },
      });

      api.setToken(data.access_token);
      api.setRefreshToken(data.refresh_token);
      setUser(data.user);
      void downloadPersonalSpaceIndex(data.user).catch(() => undefined);
    },
    [mode],
  );

  const updateProfile = useCallback(
    async (input: { display_name: string; avatar_url?: string; username?: string }) => {
      if (mode !== "server") {
        throw new Error("Profile editing is unavailable in local mode");
      }

      const nextUser = await api.fetch<User>("/auth/me", {
        method: "PUT",
        body: input,
      });
      setUser(nextUser);
    },
    [mode],
  );

  const updateCurrentUser = useCallback((input: Partial<User>) => {
    setUser((current) => mergeAuthUserUpdate(current, input));
  }, []);

  const logout = useCallback(() => {
    if (mode !== "server") return;
    void api.logoutOnServer();
    setUser(null);
  }, [mode]);

  return (
    <AuthContext.Provider
      value={{
        user,
        config,
        loading,
        login,
        loginWithPasskey,
        register,
        updateCurrentUser,
        updateProfile,
        logout,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}
