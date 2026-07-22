import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  applyGoogleDriveRootSelectionToSession,
  buildGoogleDriveManualConnectionPlan,
  buildGoogleDriveManualSetupPlan,
  buildGoogleDriveOAuthCodeExchangeRequest,
  buildGoogleDriveOAuthRefreshRequest,
  buildGoogleDriveOAuthAuthorizationRequest,
  buildGoogleDriveProductionOAuthEvidenceUpdate,
  buildGoogleDriveRootCreateRequest,
  buildGoogleDriveRootLookupRequest,
  classifyGoogleDriveOAuthRedirectEnvironment,
  clearProviderSession,
  completeGoogleDriveManualProviderConnection,
  completeGoogleDriveManualProviderConnectionFromCallback,
  createGoogleDriveApiFetchTransport,
  createGoogleDriveOAuthFetchTransport,
  createDisconnectedProviderSession,
  createGoogleDriveManualProviderSession,
  createGoogleDriveOAuthBootstrap,
  describeGoogleDriveManualConnectionReadiness,
  exchangeGoogleDriveOAuthCodeForManualProviderSession,
  readProviderDefaultSyncValidationEvidence,
  readProviderSession,
  refreshGoogleDriveManualProviderSession,
  refreshGoogleDriveManualProviderSessionWithTransport,
  resolveGoogleDriveRootForManualProviderSession,
  summarizeProviderSession,
  validateGoogleDriveOAuthCallback,
  writeProviderDefaultSyncValidationEvidence,
} from "./providerSession";

describe("providerSession", () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    (globalThis as { window?: unknown }).window = {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => {
          storage.set(key, value);
        },
        removeItem: (key: string) => {
          storage.delete(key);
        },
      },
    };
  });

  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
  });

  it("builds a Google Drive OAuth authorization request with offline PKCE parameters", () => {
    const request = buildGoogleDriveOAuthAuthorizationRequest({
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      state: "state-123",
      codeChallenge: "challenge-abc",
    });
    const authUrl = new URL(request.auth_url);

    expect(authUrl.origin + authUrl.pathname).toBe(
      "https://accounts.google.com/o/oauth2/v2/auth",
    );
    expect(authUrl.searchParams.get("client_id")).toBe("google-client-id");
    expect(authUrl.searchParams.get("redirect_uri")).toBe(
      "starlane://provider/google-drive/callback",
    );
    expect(authUrl.searchParams.get("response_type")).toBe("code");
    expect(authUrl.searchParams.get("state")).toBe("state-123");
    expect(authUrl.searchParams.get("code_challenge")).toBe("challenge-abc");
    expect(authUrl.searchParams.get("code_challenge_method")).toBe("S256");
    expect(authUrl.searchParams.get("access_type")).toBe("offline");
    expect(authUrl.searchParams.get("prompt")).toBe("consent");
    expect(authUrl.searchParams.get("scope")).toBe(
      "drive.file drive.metadata.readonly offline_access",
    );
    expect(request).toMatchObject({
      provider: "google-drive",
      token_url: "https://oauth2.googleapis.com/token",
      scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
    });
  });

  it("persists and clears provider default-sync validation evidence", () => {
    expect(readProviderDefaultSyncValidationEvidence("google-drive")).toEqual({
      provider: "google-drive",
      production_oauth_verified: false,
      device_background_wake_verified: false,
      notes: [],
    });

    writeProviderDefaultSyncValidationEvidence("google-drive", {
      provider: "google-drive",
      production_oauth_verified: true,
      device_background_wake_verified: false,
      production_oauth_verified_at: "2026-07-09T12:00:00.000Z",
      notes: ["Live OAuth completed"],
    });

    expect(readProviderDefaultSyncValidationEvidence("google-drive")).toEqual({
      provider: "google-drive",
      production_oauth_verified: true,
      device_background_wake_verified: false,
      production_oauth_verified_at: "2026-07-09T12:00:00.000Z",
      device_background_wake_verified_at: undefined,
      notes: ["Live OAuth completed"],
    });

    clearProviderSession("google-drive");

    expect(readProviderDefaultSyncValidationEvidence("google-drive")).toEqual({
      provider: "google-drive",
      production_oauth_verified: false,
      device_background_wake_verified: false,
      notes: [],
    });
  });

  it("creates a manual Google Drive provider session from OAuth token response data", () => {
    const session = createGoogleDriveManualProviderSession({
      access_token: "access-token",
      refresh_token: "refresh-token",
      expires_in: 3600,
      scope: "drive.file drive.metadata.readonly offline_access",
      issued_at: "2026-07-08T12:00:00.000Z",
      account_label: "eagle@example.com",
      provider_root_id: "drive-root-1",
      provider_root_name: "Starlane Notes",
    });

    expect(session).toEqual({
      provider: "google-drive",
      connected: true,
      auth: {
        access_token: "access-token",
        refresh_token: "refresh-token",
        account_label: "eagle@example.com",
        scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
        issued_at: "2026-07-08T12:00:00.000Z",
        expires_at: "2026-07-08T13:00:00.000Z",
        provider_root_id: "drive-root-1",
        provider_root_name: "Starlane Notes",
      },
      sync_mode: "manual",
      last_operation: "connect",
    });
  });

  it("summarizes Google Drive refresh token readiness without exposing the token", () => {
    const session = createGoogleDriveManualProviderSession({
      access_token: "access-token",
      refresh_token: "refresh-token",
      expires_in: 3600,
      scope: "drive.file drive.metadata.readonly offline_access",
      issued_at: "2026-07-08T12:00:00.000Z",
      account_label: "eagle@example.com",
      provider_root_id: "drive-root-1",
      provider_root_name: "Starlane Notes",
    });

    expect(summarizeProviderSession(session)).toMatchObject({
      refresh_token_ready: true,
      provider_root_id: "drive-root-1",
    });
    expect(JSON.stringify(summarizeProviderSession(session))).not.toContain("refresh-token");
  });

  it("builds a Google Drive OAuth code exchange request", () => {
    expect(
      buildGoogleDriveOAuthCodeExchangeRequest({
        clientId: "google-client-id",
        redirectUri: "starlane://provider/google-drive/callback",
        code: "oauth-code",
        codeVerifier: "verifier-123",
      }),
    ).toEqual({
      provider: "google-drive",
      token_url: "https://oauth2.googleapis.com/token",
      client_id: "google-client-id",
      redirect_uri: "starlane://provider/google-drive/callback",
      code: "oauth-code",
      code_verifier: "verifier-123",
      grant_type: "authorization_code",
    });
  });

  it("exchanges a Google Drive OAuth code through an injected transport", async () => {
    const request = buildGoogleDriveOAuthCodeExchangeRequest({
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      code: "oauth-code",
      codeVerifier: "verifier-123",
    });
    const seenRequests: unknown[] = [];

    const session = await exchangeGoogleDriveOAuthCodeForManualProviderSession({
      request,
      transport: {
        async exchangeGoogleDriveOAuthCode(nextRequest) {
          seenRequests.push(nextRequest);
          return {
            access_token: "access-token",
            refresh_token: "refresh-token",
            expires_in: 3600,
            scope: "drive.file drive.metadata.readonly offline_access",
            issued_at: "2026-07-08T12:00:00.000Z",
            account_label: "eagle@example.com",
          };
        },
      },
    });

    expect(seenRequests).toEqual([request]);
    expect(session).toMatchObject({
      provider: "google-drive",
      connected: true,
      sync_mode: "manual",
      auth: {
        access_token: "access-token",
        refresh_token: "refresh-token",
        account_label: "eagle@example.com",
        scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
        issued_at: "2026-07-08T12:00:00.000Z",
        expires_at: "2026-07-08T13:00:00.000Z",
      },
    });
  });

  it("exchanges a Google Drive OAuth code through the explicit fetch transport", async () => {
    const request = buildGoogleDriveOAuthCodeExchangeRequest({
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      code: "oauth-code",
      codeVerifier: "verifier-123",
    });
    const calls: Array<{ input: string; init: RequestInit }> = [];
    const transport = createGoogleDriveOAuthFetchTransport(async (input, init) => {
      calls.push({ input, init });
      return {
        ok: true,
        status: 200,
        json: async () => ({
          access_token: "access-token",
          refresh_token: "refresh-token",
          expires_in: 3600,
          scope: "drive.file drive.metadata.readonly offline_access",
          account_label: "eagle@example.com",
        }),
      };
    });

    const session = await exchangeGoogleDriveOAuthCodeForManualProviderSession({
      request,
      transport,
    });

    expect(calls).toHaveLength(1);
    expect(calls[0].input).toBe("https://oauth2.googleapis.com/token");
    expect(calls[0].init.method).toBe("POST");
    expect(calls[0].init.headers).toEqual({
      "Content-Type": "application/x-www-form-urlencoded",
    });
    const body = calls[0].init.body as URLSearchParams;
    expect(Array.from(body.keys())).toEqual([
      "client_id",
      "redirect_uri",
      "code",
      "code_verifier",
      "grant_type",
    ]);
    expect(body.get("client_id")).toBe("google-client-id");
    expect(body.get("code")).toBe("oauth-code");
    expect(body.get("code_verifier")).toBe("verifier-123");
    expect(body.get("grant_type")).toBe("authorization_code");
    expect(body.has("client_secret")).toBe(false);
    expect(session).toMatchObject({
      provider: "google-drive",
      connected: true,
      sync_mode: "manual",
      auth: {
        access_token: "access-token",
        refresh_token: "refresh-token",
        account_label: "eagle@example.com",
      },
    });
  });

  it("refreshes a Google Drive manual session through the explicit fetch transport", async () => {
    const session = createGoogleDriveManualProviderSession({
      access_token: "old-access-token",
      refresh_token: "refresh-token",
      expires_in: 60,
      scope: "drive.file drive.metadata.readonly offline_access",
      issued_at: "2026-07-08T12:00:00.000Z",
      account_label: "eagle@example.com",
      provider_root_id: "drive-root-1",
      provider_root_name: "Starlane Notes",
    });
    const calls: Array<{ input: string; init: RequestInit }> = [];
    const transport = createGoogleDriveOAuthFetchTransport(async (input, init) => {
      calls.push({ input, init });
      return {
        ok: true,
        status: 200,
        json: async () => ({
          access_token: "new-access-token",
          expires_in: 3600,
          scope: "drive.file drive.metadata.readonly offline_access",
        }),
      };
    });

    const refreshed = await refreshGoogleDriveManualProviderSessionWithTransport({
      clientId: "google-client-id",
      session,
      transport,
    });

    const body = calls[0].init.body as URLSearchParams;
    expect(body.get("grant_type")).toBe("refresh_token");
    expect(body.get("refresh_token")).toBe("refresh-token");
    expect(body.get("scope")).toBe("drive.file drive.metadata.readonly offline_access");
    expect(body.has("client_secret")).toBe(false);
    expect(refreshed.auth).toMatchObject({
      access_token: "new-access-token",
      refresh_token: "refresh-token",
      provider_root_id: "drive-root-1",
      provider_root_name: "Starlane Notes",
    });
  });

  it("reports Google Drive OAuth fetch transport token errors", async () => {
    const request = buildGoogleDriveOAuthCodeExchangeRequest({
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      code: "oauth-code",
      codeVerifier: "verifier-123",
    });
    const transport = createGoogleDriveOAuthFetchTransport(async () => ({
      ok: false,
      status: 400,
      statusText: "Bad Request",
      json: async () => ({ error: "invalid_grant" }),
    }));

    await expect(
      exchangeGoogleDriveOAuthCodeForManualProviderSession({ request, transport }),
    ).rejects.toThrow("Google Drive OAuth token request failed with 400 Bad Request");
  });

  it("wraps fetch as a Google Drive API transport", async () => {
    const calls: Array<{ input: string; init: RequestInit }> = [];
    const transport = createGoogleDriveApiFetchTransport(async (input, init) => {
      calls.push({ input, init });
      return {
        ok: true,
        status: 200,
        json: async () => ({ files: [] }),
      };
    });

    const response = await transport.fetch("https://www.googleapis.com/drive/v3/files", {
      method: "GET",
    });

    expect(response.ok).toBe(true);
    expect(calls).toEqual([
      {
        input: "https://www.googleapis.com/drive/v3/files",
        init: { method: "GET" },
      },
    ]);
  });

  it("builds a Google Drive OAuth refresh request from an existing manual session", () => {
    const session = createGoogleDriveManualProviderSession({
      access_token: "old-access-token",
      refresh_token: "refresh-token",
      expires_in: 3600,
      scope: "drive.file drive.metadata.readonly offline_access",
      issued_at: "2026-07-08T12:00:00.000Z",
    });

    expect(
      buildGoogleDriveOAuthRefreshRequest({
        clientId: "google-client-id",
        session,
      }),
    ).toEqual({
      provider: "google-drive",
      token_url: "https://oauth2.googleapis.com/token",
      client_id: "google-client-id",
      refresh_token: "refresh-token",
      grant_type: "refresh_token",
      scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
    });
  });

  it("refreshes a Google Drive manual session while preserving refresh token and root metadata", () => {
    const original = {
      ...createGoogleDriveManualProviderSession({
        access_token: "old-access-token",
        refresh_token: "refresh-token",
        expires_in: 60,
        scope: "drive.file drive.metadata.readonly offline_access",
        issued_at: "2026-07-08T12:00:00.000Z",
        account_label: "eagle@example.com",
        provider_root_id: "drive-root-1",
        provider_root_name: "Starlane Notes",
      }),
      last_synced_at: "2026-07-08T12:02:00.000Z",
    };

    expect(
      refreshGoogleDriveManualProviderSession(original, {
        access_token: "new-access-token",
        expires_in: 1800,
        issued_at: "2026-07-08T12:10:00.000Z",
      }),
    ).toEqual({
      provider: "google-drive",
      connected: true,
      auth: {
        access_token: "new-access-token",
        refresh_token: "refresh-token",
        account_label: "eagle@example.com",
        scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
        issued_at: "2026-07-08T12:10:00.000Z",
        expires_at: "2026-07-08T12:40:00.000Z",
        provider_root_id: "drive-root-1",
        provider_root_name: "Starlane Notes",
      },
      sync_mode: "manual",
      last_operation: "connect",
      last_synced_at: "2026-07-08T12:02:00.000Z",
    });
  });

  it("rejects Google Drive refresh planning without a refresh token", () => {
    const session = createGoogleDriveManualProviderSession({
      access_token: "access-token",
      expires_in: 3600,
      issued_at: "2026-07-08T12:00:00.000Z",
    });

    expect(() =>
      buildGoogleDriveOAuthRefreshRequest({
        clientId: "google-client-id",
        session,
      }),
    ).toThrow("Google Drive refresh requires an existing provider refresh token");
  });

  it("builds Google Drive root lookup and create request contracts", () => {
    expect(buildGoogleDriveRootLookupRequest({ rootName: "Eagle's Notes" })).toEqual({
      provider: "google-drive",
      method: "GET",
      endpoint: "https://www.googleapis.com/drive/v3/files",
      query: {
        q: "mimeType = 'application/vnd.google-apps.folder' and name = 'Eagle\\'s Notes' and trashed = false",
        fields: "files(id,name,mimeType,modifiedTime)",
        spaces: "drive",
        pageSize: "10",
      },
      required_scopes: ["drive.file", "drive.metadata.readonly"],
      root_name: "Eagle's Notes",
    });

    expect(buildGoogleDriveRootCreateRequest()).toEqual({
      provider: "google-drive",
      method: "POST",
      endpoint: "https://www.googleapis.com/drive/v3/files",
      body: {
        name: "Starlane Notes",
        mimeType: "application/vnd.google-apps.folder",
      },
      required_scopes: ["drive.file"],
      root_name: "Starlane Notes",
    });
  });

  it("applies a selected Google Drive root to an existing manual session", () => {
    const session = {
      ...createGoogleDriveManualProviderSession({
        access_token: "access-token",
        refresh_token: "refresh-token",
        expires_in: 3600,
        scope: "drive.file drive.metadata.readonly offline_access",
        issued_at: "2026-07-08T12:00:00.000Z",
      }),
      last_synced_at: "2026-07-08T12:05:00.000Z",
    };

    expect(
      applyGoogleDriveRootSelectionToSession(session, {
        id: "drive-folder-id",
        name: "Starlane Notes",
      }),
    ).toEqual({
      ...session,
      auth: {
        ...session.auth,
        provider_root_id: "drive-folder-id",
        provider_root_name: "Starlane Notes",
      },
    });
  });

  it("rejects Google Drive root selection without an auth session", () => {
    expect(() =>
      applyGoogleDriveRootSelectionToSession(
        createDisconnectedProviderSession("google-drive"),
        {
          id: "drive-folder-id",
          name: "Starlane Notes",
        },
      ),
    ).toThrow("Google Drive root selection requires an existing provider auth session");
  });

  it("resolves an existing Google Drive root through an injected API transport", async () => {
    const session = createGoogleDriveManualProviderSession({
      access_token: "access-token",
      refresh_token: "refresh-token",
      expires_in: 3600,
      scope: "drive.file drive.metadata.readonly offline_access",
      issued_at: "2026-07-08T12:00:00.000Z",
    });
    const calls: Array<{ input: string; init: RequestInit }> = [];

    const result = await resolveGoogleDriveRootForManualProviderSession({
      session,
      rootName: "Starlane Notes",
      now: new Date("2026-07-08T12:01:00.000Z"),
      transport: {
        fetch: async (input, init) => {
          calls.push({ input, init });
          return {
            ok: true,
            status: 200,
            json: async () => ({
              files: [
                {
                  id: "drive-root-existing",
                  name: "Starlane Notes",
                  mimeType: "application/vnd.google-apps.folder",
                },
              ],
            }),
          };
        },
      },
    });

    expect(calls).toHaveLength(1);
    expect(calls[0]?.input).toContain("https://www.googleapis.com/drive/v3/files?");
    expect(calls[0]?.input).toContain("mimeType+%3D+%27application%2Fvnd.google-apps.folder%27");
    expect(calls[0]?.init).toMatchObject({
      method: "GET",
      headers: {
        Authorization: "Bearer access-token",
      },
    });
    expect(result).toMatchObject({
      status: "selected_existing",
      blockers: [],
      selection: {
        id: "drive-root-existing",
        name: "Starlane Notes",
      },
      session: {
        auth: {
          provider_root_id: "drive-root-existing",
          provider_root_name: "Starlane Notes",
        },
      },
    });
  });

  it("creates a Google Drive root when lookup returns no folder", async () => {
    const session = createGoogleDriveManualProviderSession({
      access_token: "access-token",
      refresh_token: "refresh-token",
      expires_in: 3600,
      scope: "drive.file drive.metadata.readonly offline_access",
      issued_at: "2026-07-08T12:00:00.000Z",
    });
    const calls: Array<{ input: string; init: RequestInit }> = [];

    const result = await resolveGoogleDriveRootForManualProviderSession({
      session,
      now: new Date("2026-07-08T12:01:00.000Z"),
      transport: {
        fetch: async (input, init) => {
          calls.push({ input, init });
          if (init.method === "GET") {
            return {
              ok: true,
              status: 200,
              json: async () => ({ files: [] }),
            };
          }
          return {
            ok: true,
            status: 200,
            json: async () => ({ id: "drive-root-created", name: "Starlane Notes" }),
          };
        },
      },
    });

    expect(calls).toHaveLength(2);
    expect(calls[1]?.input).toBe("https://www.googleapis.com/drive/v3/files");
    expect(calls[1]?.init).toMatchObject({
      method: "POST",
      headers: {
        Authorization: "Bearer access-token",
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        name: "Starlane Notes",
        mimeType: "application/vnd.google-apps.folder",
      }),
    });
    expect(result).toMatchObject({
      status: "created",
      blockers: [],
      selection: {
        id: "drive-root-created",
        name: "Starlane Notes",
      },
      session: {
        auth: {
          provider_root_id: "drive-root-created",
          provider_root_name: "Starlane Notes",
        },
      },
    });
  });

  it("blocks Google Drive root execution when the manual session lacks required scopes", async () => {
    const session = createGoogleDriveManualProviderSession({
      access_token: "access-token",
      refresh_token: "refresh-token",
      expires_in: 3600,
      scope: "drive.file",
      issued_at: "2026-07-08T12:00:00.000Z",
    });

    const result = await resolveGoogleDriveRootForManualProviderSession({
      session,
      now: new Date("2026-07-08T12:01:00.000Z"),
      transport: {
        fetch: async () => {
          throw new Error("should not fetch without required scopes");
        },
      },
    });

    expect(result).toMatchObject({
      status: "blocked",
      blockers: ["Google Drive root selection missing scopes: drive.metadata.readonly"],
    });
  });

  it("completes and persists a manual Google Drive provider connection", async () => {
    const storage = new Map<string, string>();
    (globalThis as { window?: unknown }).window = {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => {
          storage.set(key, value);
        },
        removeItem: (key: string) => {
          storage.delete(key);
        },
      },
    };
    const request = buildGoogleDriveOAuthCodeExchangeRequest({
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      code: "oauth-code",
      codeVerifier: "verifier-123",
    });

    try {
      const result = await completeGoogleDriveManualProviderConnection({
        request,
        now: new Date("2026-07-08T12:01:00.000Z"),
        tokenTransport: {
          async exchangeGoogleDriveOAuthCode() {
            return {
              access_token: "access-token",
              refresh_token: "refresh-token",
              expires_in: 3600,
              scope: "drive.file drive.metadata.readonly offline_access",
              issued_at: "2026-07-08T12:00:00.000Z",
              account_label: "eagle@example.com",
            };
          },
        },
        rootTransport: {
          fetch: async (input, init) => {
            if (init.method === "GET") {
              return {
                ok: true,
                status: 200,
                json: async () => ({ files: [] }),
              };
            }
            expect(input).toBe("https://www.googleapis.com/drive/v3/files");
            return {
              ok: true,
              status: 200,
              json: async () => ({ id: "drive-root-created", name: "Starlane Notes" }),
            };
          },
        },
      });

      expect(result).toMatchObject({
        status: "connected",
        persisted: true,
        blockers: [],
        root_result: {
          status: "created",
          selection: {
            id: "drive-root-created",
            name: "Starlane Notes",
          },
        },
      });
      expect(readProviderSession("google-drive")).toMatchObject({
        provider: "google-drive",
        connected: true,
        sync_mode: "manual",
        auth: {
          access_token: "access-token",
          provider_root_id: "drive-root-created",
          provider_root_name: "Starlane Notes",
        },
      });
    } finally {
      delete (globalThis as { window?: unknown }).window;
    }
  });

  it("does not persist a manual Google Drive provider connection when root resolution is blocked", async () => {
    const storage = new Map<string, string>();
    (globalThis as { window?: unknown }).window = {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => {
          storage.set(key, value);
        },
        removeItem: (key: string) => {
          storage.delete(key);
        },
      },
    };
    const request = buildGoogleDriveOAuthCodeExchangeRequest({
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      code: "oauth-code",
      codeVerifier: "verifier-123",
    });

    try {
      const result = await completeGoogleDriveManualProviderConnection({
        request,
        now: new Date("2026-07-08T12:01:00.000Z"),
        tokenTransport: {
          async exchangeGoogleDriveOAuthCode() {
            return {
              access_token: "access-token",
              refresh_token: "refresh-token",
              expires_in: 3600,
              scope: "drive.file",
              issued_at: "2026-07-08T12:00:00.000Z",
            };
          },
        },
        rootTransport: {
          fetch: async () => {
            throw new Error("should not fetch without required scopes");
          },
        },
      });

      expect(result).toMatchObject({
        status: "blocked",
        persisted: false,
        blockers: ["Google Drive root selection missing scopes: drive.metadata.readonly"],
      });
      expect(readProviderSession("google-drive")).toMatchObject({
        provider: "google-drive",
        connected: false,
      });
    } finally {
      delete (globalThis as { window?: unknown }).window;
    }
  });

  it("completes a manual Google Drive provider connection from a validated OAuth callback", async () => {
    const exchangedCodes: string[] = [];
    const result = await completeGoogleDriveManualProviderConnectionFromCallback({
      callbackUrl: "starlane://provider/google-drive/callback?code=oauth-code&state=state-123",
      expectedState: "state-123",
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      codeVerifier: "verifier-123",
      persist: false,
      tokenTransport: {
        async exchangeGoogleDriveOAuthCode(request) {
          exchangedCodes.push(request.code);
          return {
            access_token: "access-token",
            refresh_token: "refresh-token",
            expires_in: 3600,
            scope: "drive.file drive.metadata.readonly offline_access",
            issued_at: "2026-07-08T12:00:00.000Z",
          };
        },
      },
    });

    expect(exchangedCodes).toEqual(["oauth-code"]);
    expect(result).toMatchObject({
      status: "connected",
      persisted: false,
      blockers: [],
      callback_result: {
        status: "ready_for_exchange",
        code: "oauth-code",
      },
      session: {
        provider: "google-drive",
        connected: true,
        sync_mode: "manual",
        auth: {
          access_token: "access-token",
          refresh_token: "refresh-token",
        },
      },
    });
  });

  it("blocks a manual Google Drive provider connection from a callback with invalid state", async () => {
    let exchangeCalled = false;
    const result = await completeGoogleDriveManualProviderConnectionFromCallback({
      callbackUrl: "starlane://provider/google-drive/callback?code=oauth-code&state=wrong-state",
      expectedState: "state-123",
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      codeVerifier: "verifier-123",
      tokenTransport: {
        async exchangeGoogleDriveOAuthCode() {
          exchangeCalled = true;
          throw new Error("exchange should not run");
        },
      },
    });

    expect(exchangeCalled).toBe(false);
    expect(result).toMatchObject({
      status: "blocked",
      persisted: false,
      blockers: ["Google Drive OAuth callback state does not match the pending setup state"],
      callback_result: {
        status: "blocked",
      },
    });
    expect(result.connection_result).toBeUndefined();
    expect(result.session).toBeUndefined();
  });

  it("describes Google Drive manual connection readiness for explicit API transport", () => {
    const session = createGoogleDriveManualProviderSession({
      access_token: "access-token",
      refresh_token: "refresh-token",
      expires_in: 3600,
      scope: "drive.file drive.metadata.readonly offline_access",
      issued_at: "2026-07-08T12:00:00.000Z",
      provider_root_id: "drive-root-id",
      provider_root_name: "Starlane Notes",
    });

    expect(
      describeGoogleDriveManualConnectionReadiness(
        session,
        new Date("2026-07-08T12:01:00.000Z"),
      ),
    ).toEqual({
      provider: "google-drive",
      connected: true,
      manual_session: true,
      access_token_ready: true,
      refresh_token_ready: true,
      required_scopes_ready: true,
      selected_root_ready: true,
      oauth_exchange_boundary_ready: true,
      root_execution_boundary_ready: true,
      default_sync_enabled: false,
      ready_for_explicit_api_transport: true,
      blockers: [],
    });
  });

  it("blocks Google Drive manual readiness without a refresh token", () => {
    const session = createGoogleDriveManualProviderSession({
      access_token: "access-token",
      expires_in: 3600,
      scope: "drive.file drive.metadata.readonly offline_access",
      issued_at: "2026-07-08T12:00:00.000Z",
      provider_root_id: "drive-root-id",
      provider_root_name: "Starlane Notes",
    });

    expect(
      describeGoogleDriveManualConnectionReadiness(
        session,
        new Date("2026-07-08T12:01:00.000Z"),
      ),
    ).toMatchObject({
      access_token_ready: true,
      refresh_token_ready: false,
      required_scopes_ready: true,
      selected_root_ready: true,
      ready_for_explicit_api_transport: false,
      blockers: ["Google Drive manual provider session is missing a refresh token"],
    });
  });

  it("reports Google Drive manual connection blockers for simulated sessions", () => {
    expect(
      describeGoogleDriveManualConnectionReadiness(
        {
          provider: "google-drive",
          connected: true,
          auth: {
            access_token: "google-drive-access-simulated",
            refresh_token: "google-drive-refresh-simulated",
            account_label: "Google account",
            scopes: ["drive.file"],
            issued_at: "2026-07-08T12:00:00.000Z",
            expires_at: "2026-07-08T13:00:00.000Z",
          },
          sync_mode: "simulated",
        },
        new Date("2026-07-08T12:01:00.000Z"),
      ),
    ).toMatchObject({
      default_sync_enabled: false,
      ready_for_explicit_api_transport: false,
      blockers: [
        "Google Drive manual provider session is not using real OAuth auth",
        "Google Drive manual provider session missing scopes: drive.metadata.readonly, offline_access",
        "Google Drive manual provider session has not selected a Drive root folder",
      ],
    });
  });

  it("builds a blocked Google Drive manual setup plan when OAuth inputs are missing", () => {
    expect(buildGoogleDriveManualSetupPlan()).toEqual({
      provider: "google-drive",
      status: "blocked",
      oauth_client_type: "public_pkce",
      oauth_redirect_environment: "unconfigured",
      production_oauth_candidate: false,
      client_id_configured: false,
      client_secret_configured: false,
      client_secret_allowed: false,
      redirect_uri_configured: false,
      pkce_required: true,
      required_scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
      token_exchange_boundary: "injected_transport",
      root_resolution_boundary: "injected_api_transport",
      persistence_boundary: "provider_session_record",
      default_sync_enabled: false,
      blockers: [
        "Google Drive setup requires a public OAuth client ID",
        "Google Drive setup requires an OAuth redirect URI",
        "Google Drive setup requires a CSRF state value",
        "Google Drive setup requires a PKCE code challenge",
      ],
      steps: [
        "Generate a CSRF state value and PKCE verifier/challenge in the app shell.",
        "Open the Google OAuth authorization URL with a public PKCE client, offline access, and Drive file scopes.",
        "Exchange the returned authorization code through an injected token transport without a client secret.",
        "Resolve or create the 'Starlane Notes' Drive root through an injected API transport.",
        "Persist the completed manual provider session only after token and root setup succeed.",
      ],
    });
  });

  it("blocks Google Drive manual setup when a browser-facing client secret is configured", () => {
    const plan = buildGoogleDriveManualSetupPlan({
      clientId: "google-client-id",
      clientSecret: "do-not-ship-this",
      redirectUri: "starlane://provider/google-drive/callback",
      state: "state-123",
      codeChallenge: "challenge-abc",
    });

    expect(plan).toMatchObject({
      provider: "google-drive",
      status: "blocked",
      oauth_client_type: "public_pkce",
      oauth_redirect_environment: "native_custom_scheme",
      production_oauth_candidate: false,
      client_id_configured: true,
      client_secret_configured: true,
      client_secret_allowed: false,
      redirect_uri_configured: true,
      pkce_required: true,
      oauth_authorization_url: undefined,
      blockers: [
        "Google Drive browser setup must not expose or send an OAuth client secret",
      ],
    });
  });

  it("builds a ready Google Drive manual setup plan with an OAuth URL", () => {
    const plan = buildGoogleDriveManualSetupPlan({
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      state: "state-123",
      codeChallenge: "challenge-abc",
    });

    expect(plan).toMatchObject({
      provider: "google-drive",
      status: "ready_to_start",
      oauth_client_type: "public_pkce",
      oauth_redirect_environment: "native_custom_scheme",
      production_oauth_candidate: true,
      client_id_configured: true,
      client_secret_configured: false,
      client_secret_allowed: false,
      redirect_uri_configured: true,
      pkce_required: true,
      token_exchange_boundary: "injected_transport",
      root_resolution_boundary: "injected_api_transport",
      persistence_boundary: "provider_session_record",
      default_sync_enabled: false,
      blockers: [],
    });
    expect(plan.oauth_authorization_url).toContain(
      "https://accounts.google.com/o/oauth2/v2/auth?",
    );
    expect(new URL(plan.oauth_authorization_url ?? "").searchParams.get("client_id")).toBe(
      "google-client-id",
    );
  });

  it("classifies Google Drive OAuth redirect environments for production readiness", () => {
    expect(classifyGoogleDriveOAuthRedirectEnvironment("")).toBe("unconfigured");
    expect(
      classifyGoogleDriveOAuthRedirectEnvironment(
        "starlane://provider/google-drive/callback",
      ),
    ).toBe("native_custom_scheme");
    expect(
      classifyGoogleDriveOAuthRedirectEnvironment(
        "https://notes.example.com/provider/google-drive/callback",
      ),
    ).toBe("production_https");
    expect(
      classifyGoogleDriveOAuthRedirectEnvironment(
        "http://localhost:5199/provider/google-drive/callback",
      ),
    ).toBe("local_development");
    expect(
      classifyGoogleDriveOAuthRedirectEnvironment(
        "https://localhost/provider/google-drive/callback",
      ),
    ).toBe("local_development");
  });

  it("marks HTTPS non-local Google Drive setup as a production OAuth candidate", () => {
    expect(
      buildGoogleDriveManualSetupPlan({
        clientId: "google-client-id",
        redirectUri: "https://notes.example.com/provider/google-drive/callback",
        state: "state-123",
        codeChallenge: "challenge-abc",
      }),
    ).toMatchObject({
      status: "ready_to_start",
      oauth_redirect_environment: "production_https",
      production_oauth_candidate: true,
      blockers: [],
    });
  });

  it("keeps localhost Google Drive setup out of production OAuth candidate status", () => {
    expect(
      buildGoogleDriveManualSetupPlan({
        clientId: "google-client-id",
        redirectUri: "http://localhost:5199/provider/google-drive/callback",
        state: "state-123",
        codeChallenge: "challenge-abc",
      }),
    ).toMatchObject({
      status: "ready_to_start",
      oauth_redirect_environment: "local_development",
      production_oauth_candidate: false,
      blockers: [],
    });
  });

  it("records production OAuth evidence only for production-candidate setup", () => {
    const existingEvidence = readProviderDefaultSyncValidationEvidence("google-drive");
    const update = buildGoogleDriveProductionOAuthEvidenceUpdate({
      setupPlan: buildGoogleDriveManualSetupPlan({
        clientId: "google-client-id",
        redirectUri: "https://notes.example.com/provider/google-drive/callback",
        state: "state-123",
        codeChallenge: "challenge-abc",
      }),
      existingEvidence,
      verifiedAt: "2026-07-10T12:00:00.000Z",
    });

    expect(update).toMatchObject({
      productionVerifiedNow: true,
      evidence: {
        provider: "google-drive",
        production_oauth_verified: true,
        production_oauth_verified_at: "2026-07-10T12:00:00.000Z",
        notes: [
          "Google Drive production OAuth completed through live Google API transports at 2026-07-10T12:00:00.000Z.",
        ],
      },
    });
  });

  it("does not record production OAuth evidence for local development setup", () => {
    const update = buildGoogleDriveProductionOAuthEvidenceUpdate({
      setupPlan: buildGoogleDriveManualSetupPlan({
        clientId: "google-client-id",
        redirectUri: "http://localhost:5199/provider/google-drive/callback",
        state: "state-123",
        codeChallenge: "challenge-abc",
      }),
      existingEvidence: {
        provider: "google-drive",
        production_oauth_verified: false,
        device_background_wake_verified: false,
        notes: ["Existing note"],
      },
      verifiedAt: "2026-07-10T12:00:00.000Z",
    });

    expect(update).toMatchObject({
      productionVerifiedNow: false,
      evidence: {
        provider: "google-drive",
        production_oauth_verified: false,
        device_background_wake_verified: false,
        notes: [
          "Google Drive live OAuth completed through local/dev redirect at 2026-07-10T12:00:00.000Z; this does not count as production OAuth validation.",
          "Existing note",
        ],
      },
    });
    expect(update.evidence.production_oauth_verified_at).toBeUndefined();
  });

  it("validates a Google Drive OAuth callback and derives an exchange request", () => {
    const result = validateGoogleDriveOAuthCallback({
      callbackUrl: "starlane://provider/google-drive/callback?code=auth-code&state=state-123",
      expectedState: "state-123",
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      codeVerifier: "verifier-abc",
    });

    expect(result).toMatchObject({
      provider: "google-drive",
      status: "ready_for_exchange",
      expected_state: "state-123",
      returned_state: "state-123",
      code: "auth-code",
      blockers: [],
      exchange_request: {
        provider: "google-drive",
        token_url: "https://oauth2.googleapis.com/token",
        client_id: "google-client-id",
        redirect_uri: "starlane://provider/google-drive/callback",
        code: "auth-code",
        code_verifier: "verifier-abc",
        grant_type: "authorization_code",
      },
    });
  });

  it("builds a ready Google Drive manual connection plan from a validated callback", () => {
    const plan = buildGoogleDriveManualConnectionPlan({
      callbackUrl: "starlane://provider/google-drive/callback?code=auth-code&state=state-123",
      expectedState: "state-123",
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      codeVerifier: "verifier-abc",
    });

    expect(plan).toMatchObject({
      provider: "google-drive",
      status: "ready_to_complete",
      token_exchange_boundary: "injected_transport",
      root_resolution_boundary: "injected_api_transport",
      persistence_boundary: "provider_session_record",
      default_sync_enabled: false,
      root_name: "Starlane Notes",
      blockers: [],
      callback_result: {
        status: "ready_for_exchange",
        code: "auth-code",
      },
      steps: [
        "Validate the OAuth callback state before token exchange.",
        "Exchange the authorization code through an injected token transport.",
        "Resolve or create the 'Starlane Notes' Drive root through an injected API transport.",
        "Persist the manual provider session only after token and root setup succeed.",
        "Keep default provider sync disabled until the provider is explicitly enabled.",
      ],
    });
  });

  it("blocks a Google Drive manual connection plan when callback validation fails", () => {
    const plan = buildGoogleDriveManualConnectionPlan({
      callbackUrl: "starlane://provider/google-drive/callback?code=auth-code&state=wrong-state",
      expectedState: "state-123",
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      codeVerifier: "verifier-abc",
    });

    expect(plan.status).toBe("blocked");
    expect(plan.callback_result.exchange_request).toBeUndefined();
    expect(plan.blockers).toContain(
      "Google Drive OAuth callback state does not match the pending setup state",
    );
  });

  it("blocks a Google Drive OAuth callback with mismatched state", () => {
    const result = validateGoogleDriveOAuthCallback({
      callbackUrl: "starlane://provider/google-drive/callback?code=auth-code&state=other-state",
      expectedState: "state-123",
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      codeVerifier: "verifier-abc",
    });

    expect(result.status).toBe("blocked");
    expect(result.exchange_request).toBeUndefined();
    expect(result.blockers).toContain(
      "Google Drive OAuth callback state does not match the pending setup state",
    );
  });

  it("blocks a Google Drive OAuth callback that returned an OAuth error", () => {
    const result = validateGoogleDriveOAuthCallback({
      callbackUrl:
        "starlane://provider/google-drive/callback?error=access_denied&error_description=User%20cancelled&state=state-123",
      expectedState: "state-123",
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      codeVerifier: "verifier-abc",
    });

    expect(result.status).toBe("blocked");
    expect(result.exchange_request).toBeUndefined();
    expect(result.error).toBe("access_denied");
    expect(result.blockers).toContain(
      "Google Drive OAuth returned error: access_denied (User cancelled)",
    );
    expect(result.blockers).toContain(
      "Google Drive OAuth callback is missing an authorization code",
    );
  });

  it("blocks a Google Drive OAuth callback without an authorization code", () => {
    const result = validateGoogleDriveOAuthCallback({
      callbackUrl: "?state=state-123",
      expectedState: "state-123",
      clientId: "google-client-id",
      redirectUri: "starlane://provider/google-drive/callback",
      codeVerifier: "verifier-abc",
    });

    expect(result.status).toBe("blocked");
    expect(result.exchange_request).toBeUndefined();
    expect(result.returned_state).toBe("state-123");
    expect(result.blockers).toContain(
      "Google Drive OAuth callback is missing an authorization code",
    );
  });

  it("generates a Google Drive OAuth PKCE bootstrap", async () => {
    const bootstrap = await createGoogleDriveOAuthBootstrap({
      verifierBytes: new Uint8Array(32).fill(1),
      stateBytes: new Uint8Array(16).fill(2),
      now: new Date("2026-07-08T12:00:00.000Z"),
    });

    expect(bootstrap).toMatchObject({
      state: "AgICAgICAgICAgICAgICAg",
      code_verifier: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE",
      code_challenge_method: "S256",
      created_at: "2026-07-08T12:00:00.000Z",
    });
    expect(bootstrap.code_challenge).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(bootstrap.code_challenge).not.toContain("=");
  });
});
