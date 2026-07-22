import type { SyncProviderKind } from "@/lib/storageProviders";

export type ProviderSyncMode = "simulated" | "manual";

export interface ProviderAuthSession {
  access_token: string;
  refresh_token: string;
  account_label: string;
  scopes: string[];
  issued_at: string;
  expires_at: string;
  provider_root_id?: string;
  provider_root_name?: string;
}

export interface ProviderSessionRecord {
  provider: SyncProviderKind;
  connected: boolean;
  auth: ProviderAuthSession | null;
  sync_mode: ProviderSyncMode;
  last_operation?: "connect" | "disconnect" | "pull" | "push";
  last_synced_at?: string;
}

export interface ProviderSessionSummary {
  provider: SyncProviderKind;
  connected: boolean;
  account_label?: string;
  scopes?: string[];
  sync_mode: ProviderSyncMode;
  issued_at?: string;
  expires_at?: string;
  refresh_token_ready?: boolean;
  provider_root_id?: string;
  provider_root_name?: string;
}

export interface ProviderRemoteSyncEnablementRecord {
  provider: SyncProviderKind;
  enabled: boolean;
  enablement_boundary: "explicit_manual_provider_session";
  enabled_at?: string;
  disabled_at?: string;
  reason?: string;
}

export interface ProviderDefaultSyncValidationEvidenceRecord {
  provider: SyncProviderKind;
  production_oauth_verified: boolean;
  device_background_wake_verified: boolean;
  production_oauth_verified_at?: string;
  device_background_wake_verified_at?: string;
  notes: string[];
}

export interface ProviderOAuthAuthorizationRequest {
  provider: SyncProviderKind;
  auth_url: string;
  token_url: string;
  redirect_uri: string;
  client_id: string;
  scopes: string[];
  state: string;
  code_challenge: string;
  code_challenge_method: "S256";
  access_type?: "offline";
  prompt?: "consent";
}

export interface ProviderOAuthTokenResponse {
  access_token: string;
  refresh_token?: string;
  expires_in: number;
  scope?: string;
  issued_at?: string;
  account_label?: string;
  provider_root_id?: string;
  provider_root_name?: string;
}

export interface ProviderOAuthRefreshRequest {
  provider: SyncProviderKind;
  token_url: string;
  client_id: string;
  refresh_token: string;
  grant_type: "refresh_token";
  scopes: string[];
}

export interface ProviderOAuthCodeExchangeRequest {
  provider: "google-drive";
  token_url: "https://oauth2.googleapis.com/token";
  client_id: string;
  redirect_uri: string;
  code: string;
  code_verifier: string;
  grant_type: "authorization_code";
}

export interface ProviderOAuthCodeExchangeTransport {
  exchangeGoogleDriveOAuthCode(
    request: ProviderOAuthCodeExchangeRequest,
  ): Promise<ProviderOAuthTokenResponse>;
}

export interface ProviderOAuthRefreshTransport {
  refreshGoogleDriveOAuthToken(
    request: ProviderOAuthRefreshRequest,
  ): Promise<ProviderOAuthTokenResponse>;
}

export interface GoogleDriveRootLookupRequest {
  provider: "google-drive";
  method: "GET";
  endpoint: "https://www.googleapis.com/drive/v3/files";
  query: {
    q: string;
    fields: "files(id,name,mimeType,modifiedTime)";
    spaces: "drive";
    pageSize: "10";
  };
  required_scopes: string[];
  root_name: string;
}

export interface GoogleDriveRootCreateRequest {
  provider: "google-drive";
  method: "POST";
  endpoint: "https://www.googleapis.com/drive/v3/files";
  body: {
    name: string;
    mimeType: "application/vnd.google-apps.folder";
  };
  required_scopes: string[];
  root_name: string;
}

export interface GoogleDriveRootSelection {
  id: string;
  name: string;
}

export interface GoogleDriveApiResponse {
  ok: boolean;
  status: number;
  statusText?: string;
  json?: () => Promise<unknown>;
  text?: () => Promise<string>;
}

export interface GoogleDriveApiTransport {
  fetch: (input: string, init: RequestInit) => Promise<GoogleDriveApiResponse>;
}

export interface GoogleDriveRootSelectionExecutionResult {
  status: "selected_existing" | "created" | "blocked";
  session: ProviderSessionRecord;
  blockers: string[];
  lookup_request: GoogleDriveRootLookupRequest;
  create_request?: GoogleDriveRootCreateRequest;
  selection?: GoogleDriveRootSelection;
  receipt?: Record<string, unknown>;
}

export interface GoogleDriveManualConnectionResult {
  status: "connected" | "blocked";
  session: ProviderSessionRecord;
  persisted: boolean;
  blockers: string[];
  root_result?: GoogleDriveRootSelectionExecutionResult;
}

export interface GoogleDriveManualCallbackConnectionResult {
  status: "connected" | "blocked";
  callback_result: GoogleDriveOAuthCallbackValidationResult;
  connection_result?: GoogleDriveManualConnectionResult;
  session?: ProviderSessionRecord;
  persisted: boolean;
  blockers: string[];
}

export interface GoogleDriveManualConnectionPlan {
  provider: "google-drive";
  status: "ready_to_complete" | "blocked";
  callback_result: GoogleDriveOAuthCallbackValidationResult;
  token_exchange_boundary: "injected_transport";
  root_resolution_boundary: "injected_api_transport";
  persistence_boundary: "provider_session_record";
  default_sync_enabled: false;
  root_name: string;
  blockers: string[];
  steps: string[];
}

export interface GoogleDriveManualConnectionReadiness {
  provider: "google-drive";
  connected: boolean;
  manual_session: boolean;
  access_token_ready: boolean;
  refresh_token_ready: boolean;
  required_scopes_ready: boolean;
  selected_root_ready: boolean;
  oauth_exchange_boundary_ready: true;
  root_execution_boundary_ready: true;
  default_sync_enabled: false;
  ready_for_explicit_api_transport: boolean;
  blockers: string[];
}

export interface GoogleDriveManualSetupPlan {
  provider: "google-drive";
  status: "ready_to_start" | "blocked";
  oauth_client_type: "public_pkce";
  oauth_redirect_environment:
    | "production_https"
    | "native_custom_scheme"
    | "local_development"
    | "unconfigured";
  production_oauth_candidate: boolean;
  client_id_configured: boolean;
  client_secret_configured: boolean;
  client_secret_allowed: false;
  redirect_uri_configured: boolean;
  pkce_required: true;
  required_scopes: string[];
  oauth_authorization_url?: string;
  token_exchange_boundary: "injected_transport";
  root_resolution_boundary: "injected_api_transport";
  persistence_boundary: "provider_session_record";
  default_sync_enabled: false;
  blockers: string[];
  steps: string[];
}

export interface GoogleDriveProductionOAuthEvidenceUpdate {
  evidence: ProviderDefaultSyncValidationEvidenceRecord;
  productionVerifiedNow: boolean;
}

export interface GoogleDriveOAuthBootstrap {
  state: string;
  code_verifier: string;
  code_challenge: string;
  code_challenge_method: "S256";
  created_at: string;
}

export interface GoogleDriveOAuthCallbackValidationResult {
  provider: "google-drive";
  status: "ready_for_exchange" | "blocked";
  callback_url: string;
  expected_state: string;
  returned_state?: string;
  code?: string;
  error?: string;
  error_description?: string;
  exchange_request?: ProviderOAuthCodeExchangeRequest;
  blockers: string[];
}

function safeLocalStorage(): Storage | null {
  if (typeof window === "undefined") return null;
  return window.localStorage;
}

function storageKey(provider: SyncProviderKind): string {
  return `starlane_provider_session_${provider}`;
}

function remoteSyncEnablementStorageKey(provider: SyncProviderKind): string {
  return `starlane_provider_remote_sync_enablement_${provider}`;
}

function defaultSyncValidationEvidenceStorageKey(provider: SyncProviderKind): string {
  return `starlane_provider_default_sync_validation_${provider}`;
}

function defaultProviderScopes(provider: SyncProviderKind): string[] {
  if (provider === "icloud-drive") return ["icloud.documents", "icloud.files", "offline_access"];
  if (provider === "google-drive") return ["drive.file", "drive.metadata.readonly", "offline_access"];
  return ["local.vault"];
}

function tokenPrefix(provider: SyncProviderKind): string {
  return provider.replace(/[^a-z0-9]+/gi, "-").replace(/^-+|-+$/g, "");
}

function makeToken(provider: SyncProviderKind, suffix: string): string {
  const rand =
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID().replace(/-/g, "")
      : Math.random().toString(36).slice(2, 12);
  return `${tokenPrefix(provider)}-${suffix}-${rand}`;
}

function base64UrlEncode(bytes: Uint8Array): string {
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function randomBytes(length: number): Uint8Array {
  const bytes = new Uint8Array(length);
  if (typeof crypto !== "undefined" && crypto.getRandomValues) {
    crypto.getRandomValues(bytes);
    return bytes;
  }
  for (let index = 0; index < length; index += 1) {
    bytes[index] = Math.floor(Math.random() * 256);
  }
  return bytes;
}

function escapeGoogleDriveQueryLiteral(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/'/g, "\\'");
}

function googleDriveRootLookupUrl(request: GoogleDriveRootLookupRequest): string {
  const params = new URLSearchParams(request.query);
  return `${request.endpoint}?${params.toString()}`;
}

async function readGoogleDriveApiReceipt(
  response: GoogleDriveApiResponse,
): Promise<Record<string, unknown>> {
  if (response.json) {
    try {
      const json = await response.json();
      return typeof json === "object" && json !== null
        ? (json as Record<string, unknown>)
        : { value: json };
    } catch {
      // Fall through to text/status receipt.
    }
  }
  if (response.text) {
    try {
      return {
        status: response.status,
        status_text: response.statusText,
        body: await response.text(),
      };
    } catch {
      // Fall through to status receipt.
    }
  }
  return { status: response.status, status_text: response.statusText };
}

function firstGoogleDriveRootSelection(receipt: Record<string, unknown>): GoogleDriveRootSelection | null {
  const files = Array.isArray(receipt.files) ? receipt.files : [];
  for (const file of files) {
    if (typeof file !== "object" || file === null) continue;
    const candidate = file as Record<string, unknown>;
    if (typeof candidate.id === "string" && typeof candidate.name === "string") {
      return {
        id: candidate.id,
        name: candidate.name,
      };
    }
  }
  return null;
}

function googleDriveRootSelectionFromReceipt(
  receipt: Record<string, unknown>,
  fallbackName: string,
): GoogleDriveRootSelection | null {
  return typeof receipt.id === "string"
    ? {
        id: receipt.id,
        name: typeof receipt.name === "string" ? receipt.name : fallbackName,
      }
    : null;
}

function validateGoogleDriveRootSession(
  session: ProviderSessionRecord,
  requiredScopes: string[],
  now = new Date(),
): string[] {
  const blockers: string[] = [];
  if (session.provider !== "google-drive") {
    blockers.push("Google Drive root selection requires a Google Drive provider session");
  }
  if (!session.connected) {
    blockers.push("Google Drive root selection requires a connected provider session");
  }
  if (session.sync_mode !== "manual") {
    blockers.push("Google Drive root selection requires a real provider auth session");
  }
  if (!session.auth) {
    blockers.push("Google Drive root selection requires provider auth tokens");
    return blockers;
  }

  const grantedScopes = new Set(session.auth.scopes);
  const missingScopes = requiredScopes.filter((scope) => !grantedScopes.has(scope));
  if (missingScopes.length > 0) {
    blockers.push(`Google Drive root selection missing scopes: ${missingScopes.join(", ")}`);
  }

  const expiresAt = Date.parse(session.auth.expires_at);
  if (!Number.isFinite(expiresAt) || expiresAt <= now.getTime()) {
    blockers.push("Google Drive root selection requires an unexpired access token");
  }

  return blockers;
}

export function createProviderAuthSession(provider: SyncProviderKind): ProviderAuthSession {
  const issuedAt = new Date().toISOString();
  const expiresAt = new Date(Date.now() + 7 * 24 * 60 * 60 * 1000).toISOString();

  return {
    access_token: makeToken(provider, "access"),
    refresh_token: makeToken(provider, "refresh"),
    account_label:
      provider === "icloud-drive" ? "iCloud account" : provider === "google-drive" ? "Google account" : "Local account",
    scopes: defaultProviderScopes(provider),
    issued_at: issuedAt,
    expires_at: expiresAt,
  };
}

export function buildGoogleDriveOAuthAuthorizationRequest({
  clientId,
  redirectUri,
  state,
  codeChallenge,
  scopes = defaultProviderScopes("google-drive"),
}: {
  clientId: string;
  redirectUri: string;
  state: string;
  codeChallenge: string;
  scopes?: string[];
}): ProviderOAuthAuthorizationRequest {
  const params = new URLSearchParams({
    client_id: clientId,
    redirect_uri: redirectUri,
    response_type: "code",
    scope: scopes.join(" "),
    state,
    code_challenge: codeChallenge,
    code_challenge_method: "S256",
    access_type: "offline",
    prompt: "consent",
  });

  return {
    provider: "google-drive",
    auth_url: `https://accounts.google.com/o/oauth2/v2/auth?${params.toString()}`,
    token_url: "https://oauth2.googleapis.com/token",
    redirect_uri: redirectUri,
    client_id: clientId,
    scopes,
    state,
    code_challenge: codeChallenge,
    code_challenge_method: "S256",
    access_type: "offline",
    prompt: "consent",
  };
}

export function createGoogleDriveManualProviderSession(
  tokenResponse: ProviderOAuthTokenResponse,
): ProviderSessionRecord {
  const issuedAt = tokenResponse.issued_at ?? new Date().toISOString();
  const issuedAtMs = Date.parse(issuedAt);
  const expiresAt = new Date(
    (Number.isFinite(issuedAtMs) ? issuedAtMs : Date.now())
      + Math.max(0, tokenResponse.expires_in) * 1000,
  ).toISOString();
  const scopes = tokenResponse.scope
    ? tokenResponse.scope.split(/\s+/).filter(Boolean)
    : defaultProviderScopes("google-drive");

  return {
    provider: "google-drive",
    connected: true,
    auth: {
      access_token: tokenResponse.access_token,
      refresh_token: tokenResponse.refresh_token ?? "",
      account_label: tokenResponse.account_label ?? "Google account",
      scopes,
      issued_at: issuedAt,
      expires_at: expiresAt,
      provider_root_id: tokenResponse.provider_root_id,
      provider_root_name: tokenResponse.provider_root_name,
    },
    sync_mode: "manual",
    last_operation: "connect",
  };
}

export function buildGoogleDriveOAuthCodeExchangeRequest({
  clientId,
  redirectUri,
  code,
  codeVerifier,
}: {
  clientId: string;
  redirectUri: string;
  code: string;
  codeVerifier: string;
}): ProviderOAuthCodeExchangeRequest {
  return {
    provider: "google-drive",
    token_url: "https://oauth2.googleapis.com/token",
    client_id: clientId,
    redirect_uri: redirectUri,
    code,
    code_verifier: codeVerifier,
    grant_type: "authorization_code",
  };
}

export function validateGoogleDriveOAuthCallback({
  callbackUrl,
  expectedState,
  clientId,
  redirectUri,
  codeVerifier,
}: {
  callbackUrl: string;
  expectedState: string;
  clientId: string;
  redirectUri: string;
  codeVerifier: string;
}): GoogleDriveOAuthCallbackValidationResult {
  const normalizedCallbackUrl = callbackUrl.trim();
  const normalizedExpectedState = expectedState.trim();
  const normalizedClientId = clientId.trim();
  const normalizedRedirectUri = redirectUri.trim();
  const normalizedCodeVerifier = codeVerifier.trim();
  const blockers: string[] = [];
  let parsedUrl: URL | null = null;

  try {
    parsedUrl = normalizedCallbackUrl.startsWith("?")
      ? new URL(`starlane://provider/google-drive/callback${normalizedCallbackUrl}`)
      : new URL(normalizedCallbackUrl);
  } catch {
    blockers.push("Google Drive OAuth callback URL is invalid");
  }

  const params = parsedUrl?.searchParams;
  const code = params?.get("code")?.trim() ?? "";
  const returnedState = params?.get("state")?.trim() ?? "";
  const error = params?.get("error")?.trim() ?? "";
  const errorDescription = params?.get("error_description")?.trim() ?? "";

  if (error) {
    blockers.push(
      errorDescription
        ? `Google Drive OAuth returned error: ${error} (${errorDescription})`
        : `Google Drive OAuth returned error: ${error}`,
    );
  }
  if (!code) {
    blockers.push("Google Drive OAuth callback is missing an authorization code");
  }
  if (!normalizedExpectedState) {
    blockers.push("Google Drive OAuth callback validation requires an expected state");
  }
  if (!returnedState) {
    blockers.push("Google Drive OAuth callback is missing state");
  } else if (normalizedExpectedState && returnedState !== normalizedExpectedState) {
    blockers.push("Google Drive OAuth callback state does not match the pending setup state");
  }
  if (!normalizedClientId) {
    blockers.push("Google Drive OAuth callback exchange requires a public client ID");
  }
  if (!normalizedRedirectUri) {
    blockers.push("Google Drive OAuth callback exchange requires a redirect URI");
  }
  if (!normalizedCodeVerifier) {
    blockers.push("Google Drive OAuth callback exchange requires a PKCE code verifier");
  }

  const exchangeRequest =
    blockers.length === 0
      ? buildGoogleDriveOAuthCodeExchangeRequest({
          clientId: normalizedClientId,
          redirectUri: normalizedRedirectUri,
          code,
          codeVerifier: normalizedCodeVerifier,
        })
      : undefined;

  return {
    provider: "google-drive",
    status: blockers.length === 0 ? "ready_for_exchange" : "blocked",
    callback_url: normalizedCallbackUrl,
    expected_state: normalizedExpectedState,
    returned_state: returnedState || undefined,
    code: code || undefined,
    error: error || undefined,
    error_description: errorDescription || undefined,
    exchange_request: exchangeRequest,
    blockers,
  };
}

export function buildGoogleDriveManualConnectionPlan({
  callbackUrl,
  expectedState,
  clientId,
  redirectUri,
  codeVerifier,
  rootName = "Starlane Notes",
}: {
  callbackUrl: string;
  expectedState: string;
  clientId: string;
  redirectUri: string;
  codeVerifier: string;
  rootName?: string;
}): GoogleDriveManualConnectionPlan {
  const callbackResult = validateGoogleDriveOAuthCallback({
    callbackUrl,
    expectedState,
    clientId,
    redirectUri,
    codeVerifier,
  });
  const blockers = [...callbackResult.blockers];

  return {
    provider: "google-drive",
    status: blockers.length === 0 ? "ready_to_complete" : "blocked",
    callback_result: callbackResult,
    token_exchange_boundary: "injected_transport",
    root_resolution_boundary: "injected_api_transport",
    persistence_boundary: "provider_session_record",
    default_sync_enabled: false,
    root_name: rootName,
    blockers,
    steps: [
      "Validate the OAuth callback state before token exchange.",
      "Exchange the authorization code through an injected token transport.",
      `Resolve or create the '${rootName}' Drive root through an injected API transport.`,
      "Persist the manual provider session only after token and root setup succeed.",
      "Keep default provider sync disabled until the provider is explicitly enabled.",
    ],
  };
}

export async function exchangeGoogleDriveOAuthCodeForManualProviderSession({
  request,
  transport,
}: {
  request: ProviderOAuthCodeExchangeRequest;
  transport: ProviderOAuthCodeExchangeTransport;
}): Promise<ProviderSessionRecord> {
  const tokenResponse = await transport.exchangeGoogleDriveOAuthCode(request);
  return createGoogleDriveManualProviderSession(tokenResponse);
}

export function createGoogleDriveOAuthFetchTransport(
  fetchImpl: (input: string, init: RequestInit) => Promise<GoogleDriveApiResponse> =
    globalThis.fetch,
): ProviderOAuthCodeExchangeTransport & ProviderOAuthRefreshTransport {
  return {
    async exchangeGoogleDriveOAuthCode(request) {
      const body = new URLSearchParams({
        client_id: request.client_id,
        redirect_uri: request.redirect_uri,
        code: request.code,
        code_verifier: request.code_verifier,
        grant_type: request.grant_type,
      });
      return executeGoogleDriveTokenRequest(fetchImpl, request.token_url, body);
    },
    async refreshGoogleDriveOAuthToken(request) {
      const body = new URLSearchParams({
        client_id: request.client_id,
        refresh_token: request.refresh_token,
        grant_type: request.grant_type,
        scope: request.scopes.join(" "),
      });
      return executeGoogleDriveTokenRequest(fetchImpl, request.token_url, body);
    },
  };
}

export function createGoogleDriveApiFetchTransport(
  fetchImpl: (input: string, init: RequestInit) => Promise<GoogleDriveApiResponse> =
    globalThis.fetch,
): GoogleDriveApiTransport {
  return {
    fetch: fetchImpl,
  };
}

async function executeGoogleDriveTokenRequest(
  fetchImpl: (input: string, init: RequestInit) => Promise<GoogleDriveApiResponse>,
  tokenUrl: string,
  body: URLSearchParams,
): Promise<ProviderOAuthTokenResponse> {
  const response = await fetchImpl(tokenUrl, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
    },
    body,
  });
  const payload = await readGoogleDriveApiReceipt(response);
  if (!response.ok) {
    throw new Error(
      `Google Drive OAuth token request failed with ${response.status}${
        response.statusText ? ` ${response.statusText}` : ""
      }`,
    );
  }
  return googleDriveTokenResponseFromReceipt(payload);
}

function googleDriveTokenResponseFromReceipt(
  payload: Record<string, unknown>,
): ProviderOAuthTokenResponse {
  if (typeof payload.access_token !== "string" || payload.access_token.trim().length === 0) {
    throw new Error("Google Drive OAuth token response missing access_token");
  }
  if (typeof payload.expires_in !== "number" || !Number.isFinite(payload.expires_in)) {
    throw new Error("Google Drive OAuth token response missing expires_in");
  }
  return {
    access_token: payload.access_token,
    refresh_token: typeof payload.refresh_token === "string" ? payload.refresh_token : undefined,
    expires_in: payload.expires_in,
    scope: typeof payload.scope === "string" ? payload.scope : undefined,
    issued_at: typeof payload.issued_at === "string" ? payload.issued_at : undefined,
    account_label: typeof payload.account_label === "string" ? payload.account_label : undefined,
    provider_root_id:
      typeof payload.provider_root_id === "string" ? payload.provider_root_id : undefined,
    provider_root_name:
      typeof payload.provider_root_name === "string" ? payload.provider_root_name : undefined,
  };
}

export function buildGoogleDriveOAuthRefreshRequest({
  clientId,
  session,
}: {
  clientId: string;
  session: ProviderSessionRecord;
}): ProviderOAuthRefreshRequest {
  if (session.provider !== "google-drive" || !session.auth?.refresh_token) {
    throw new Error("Google Drive refresh requires an existing provider refresh token");
  }

  return {
    provider: "google-drive",
    token_url: "https://oauth2.googleapis.com/token",
    client_id: clientId,
    refresh_token: session.auth.refresh_token,
    grant_type: "refresh_token",
    scopes: session.auth.scopes,
  };
}

export function refreshGoogleDriveManualProviderSession(
  session: ProviderSessionRecord,
  tokenResponse: ProviderOAuthTokenResponse,
): ProviderSessionRecord {
  if (session.provider !== "google-drive" || !session.auth) {
    throw new Error("Google Drive refresh requires an existing provider auth session");
  }

  const refreshed = createGoogleDriveManualProviderSession({
    ...tokenResponse,
    refresh_token: tokenResponse.refresh_token ?? session.auth.refresh_token,
    scope: tokenResponse.scope ?? session.auth.scopes.join(" "),
    account_label: tokenResponse.account_label ?? session.auth.account_label,
    provider_root_id: tokenResponse.provider_root_id ?? session.auth.provider_root_id,
    provider_root_name: tokenResponse.provider_root_name ?? session.auth.provider_root_name,
  });

  return {
    ...refreshed,
    last_synced_at: session.last_synced_at,
  };
}

export async function refreshGoogleDriveManualProviderSessionWithTransport({
  clientId,
  session,
  transport,
}: {
  clientId: string;
  session: ProviderSessionRecord;
  transport: ProviderOAuthRefreshTransport;
}): Promise<ProviderSessionRecord> {
  const request = buildGoogleDriveOAuthRefreshRequest({ clientId, session });
  const tokenResponse = await transport.refreshGoogleDriveOAuthToken(request);
  return refreshGoogleDriveManualProviderSession(session, tokenResponse);
}

export function buildGoogleDriveRootLookupRequest({
  rootName = "Starlane Notes",
}: {
  rootName?: string;
} = {}): GoogleDriveRootLookupRequest {
  const escapedRootName = escapeGoogleDriveQueryLiteral(rootName);

  return {
    provider: "google-drive",
    method: "GET",
    endpoint: "https://www.googleapis.com/drive/v3/files",
    query: {
      q: `mimeType = 'application/vnd.google-apps.folder' and name = '${escapedRootName}' and trashed = false`,
      fields: "files(id,name,mimeType,modifiedTime)",
      spaces: "drive",
      pageSize: "10",
    },
    required_scopes: ["drive.file", "drive.metadata.readonly"],
    root_name: rootName,
  };
}

export function buildGoogleDriveRootCreateRequest({
  rootName = "Starlane Notes",
}: {
  rootName?: string;
} = {}): GoogleDriveRootCreateRequest {
  return {
    provider: "google-drive",
    method: "POST",
    endpoint: "https://www.googleapis.com/drive/v3/files",
    body: {
      name: rootName,
      mimeType: "application/vnd.google-apps.folder",
    },
    required_scopes: ["drive.file"],
    root_name: rootName,
  };
}

export function applyGoogleDriveRootSelectionToSession(
  session: ProviderSessionRecord,
  selection: GoogleDriveRootSelection,
): ProviderSessionRecord {
  if (session.provider !== "google-drive" || !session.auth) {
    throw new Error("Google Drive root selection requires an existing provider auth session");
  }

  return {
    ...session,
    auth: {
      ...session.auth,
      provider_root_id: selection.id,
      provider_root_name: selection.name,
    },
  };
}

export async function resolveGoogleDriveRootForManualProviderSession({
  session,
  rootName = "Starlane Notes",
  transport,
  now = new Date(),
}: {
  session: ProviderSessionRecord;
  rootName?: string;
  transport: GoogleDriveApiTransport;
  now?: Date;
}): Promise<GoogleDriveRootSelectionExecutionResult> {
  const lookupRequest = buildGoogleDriveRootLookupRequest({ rootName });
  const createRequest = buildGoogleDriveRootCreateRequest({ rootName });
  const requiredScopes = Array.from(
    new Set([...lookupRequest.required_scopes, ...createRequest.required_scopes]),
  );
  const sessionBlockers = validateGoogleDriveRootSession(session, requiredScopes, now);
  if (sessionBlockers.length > 0 || !session.auth) {
    return {
      status: "blocked",
      session,
      blockers: sessionBlockers,
      lookup_request: lookupRequest,
      create_request: createRequest,
    };
  }

  try {
    const lookupResponse = await transport.fetch(googleDriveRootLookupUrl(lookupRequest), {
      method: lookupRequest.method,
      headers: {
        Authorization: `Bearer ${session.auth.access_token}`,
      },
    });
    const lookupReceipt = await readGoogleDriveApiReceipt(lookupResponse);
    if (!lookupResponse.ok) {
      return {
        status: "blocked",
        session,
        blockers: [
          `Google Drive root lookup failed with ${lookupResponse.status}${
            lookupResponse.statusText ? ` ${lookupResponse.statusText}` : ""
          }`,
        ],
        lookup_request: lookupRequest,
        create_request: createRequest,
        receipt: lookupReceipt,
      };
    }

    const existingSelection = firstGoogleDriveRootSelection(lookupReceipt);
    if (existingSelection) {
      return {
        status: "selected_existing",
        session: applyGoogleDriveRootSelectionToSession(session, existingSelection),
        blockers: [],
        lookup_request: lookupRequest,
        selection: existingSelection,
        receipt: lookupReceipt,
      };
    }

    const createResponse = await transport.fetch(createRequest.endpoint, {
      method: createRequest.method,
      headers: {
        Authorization: `Bearer ${session.auth.access_token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(createRequest.body),
    });
    const createReceipt = await readGoogleDriveApiReceipt(createResponse);
    const createdSelection = googleDriveRootSelectionFromReceipt(createReceipt, rootName);
    if (!createResponse.ok || !createdSelection) {
      return {
        status: "blocked",
        session,
        blockers: [
          !createResponse.ok
            ? `Google Drive root create failed with ${createResponse.status}${
                createResponse.statusText ? ` ${createResponse.statusText}` : ""
              }`
            : "Google Drive root create response did not include a folder id",
        ],
        lookup_request: lookupRequest,
        create_request: createRequest,
        receipt: createReceipt,
      };
    }

    return {
      status: "created",
      session: applyGoogleDriveRootSelectionToSession(session, createdSelection),
      blockers: [],
      lookup_request: lookupRequest,
      create_request: createRequest,
      selection: createdSelection,
      receipt: createReceipt,
    };
  } catch (error) {
    return {
      status: "blocked",
      session,
      blockers: [
        error instanceof Error ? error.message : "Google Drive root selection transport failed",
      ],
      lookup_request: lookupRequest,
      create_request: createRequest,
    };
  }
}

export async function completeGoogleDriveManualProviderConnection({
  request,
  tokenTransport,
  rootTransport,
  rootName = "Starlane Notes",
  persist = true,
  now = new Date(),
}: {
  request: ProviderOAuthCodeExchangeRequest;
  tokenTransport: ProviderOAuthCodeExchangeTransport;
  rootTransport?: GoogleDriveApiTransport;
  rootName?: string;
  persist?: boolean;
  now?: Date;
}): Promise<GoogleDriveManualConnectionResult> {
  const exchangedSession = await exchangeGoogleDriveOAuthCodeForManualProviderSession({
    request,
    transport: tokenTransport,
  });

  let finalSession = exchangedSession;
  let rootResult: GoogleDriveRootSelectionExecutionResult | undefined;
  if (rootTransport) {
    rootResult = await resolveGoogleDriveRootForManualProviderSession({
      session: exchangedSession,
      rootName,
      transport: rootTransport,
      now,
    });
    if (rootResult.status === "blocked") {
      return {
        status: "blocked",
        session: exchangedSession,
        persisted: false,
        blockers: rootResult.blockers,
        root_result: rootResult,
      };
    }
    finalSession = rootResult.session;
  }

  if (persist) {
    writeProviderSession("google-drive", finalSession);
  }

  return {
    status: "connected",
    session: finalSession,
    persisted: persist,
    blockers: [],
    root_result: rootResult,
  };
}

export async function completeGoogleDriveManualProviderConnectionFromCallback({
  callbackUrl,
  expectedState,
  clientId,
  redirectUri,
  codeVerifier,
  tokenTransport,
  rootTransport,
  rootName = "Starlane Notes",
  persist = true,
  now = new Date(),
}: {
  callbackUrl: string;
  expectedState: string;
  clientId: string;
  redirectUri: string;
  codeVerifier: string;
  tokenTransport: ProviderOAuthCodeExchangeTransport;
  rootTransport?: GoogleDriveApiTransport;
  rootName?: string;
  persist?: boolean;
  now?: Date;
}): Promise<GoogleDriveManualCallbackConnectionResult> {
  const callbackResult = validateGoogleDriveOAuthCallback({
    callbackUrl,
    expectedState,
    clientId,
    redirectUri,
    codeVerifier,
  });

  if (!callbackResult.exchange_request) {
    return {
      status: "blocked",
      callback_result: callbackResult,
      persisted: false,
      blockers: callbackResult.blockers,
    };
  }

  const connectionResult = await completeGoogleDriveManualProviderConnection({
    request: callbackResult.exchange_request,
    tokenTransport,
    rootTransport,
    rootName,
    persist,
    now,
  });

  return {
    status: connectionResult.status,
    callback_result: callbackResult,
    connection_result: connectionResult,
    session: connectionResult.session,
    persisted: connectionResult.persisted,
    blockers: connectionResult.blockers,
  };
}

export function describeGoogleDriveManualConnectionReadiness(
  session: ProviderSessionRecord = readProviderSession("google-drive"),
  now = new Date(),
): GoogleDriveManualConnectionReadiness {
  const blockers: string[] = [];
  const manualSession = session.provider === "google-drive" && session.sync_mode === "manual";
  const connected = session.provider === "google-drive" && session.connected;
  const accessTokenReady = Boolean(session.auth?.access_token);
  const refreshTokenReady = Boolean(session.auth?.refresh_token?.trim());
  const selectedRootReady = Boolean(session.auth?.provider_root_id);
  const requiredScopes = ["drive.file", "drive.metadata.readonly", "offline_access"];
  const grantedScopes = new Set(session.auth?.scopes ?? []);
  const missingScopes = requiredScopes.filter((scope) => !grantedScopes.has(scope));
  const expiresAt = Date.parse(session.auth?.expires_at ?? "");
  const unexpired = Number.isFinite(expiresAt) && expiresAt > now.getTime();

  if (!connected) blockers.push("Google Drive manual provider session is not connected");
  if (!manualSession) blockers.push("Google Drive manual provider session is not using real OAuth auth");
  if (!accessTokenReady) blockers.push("Google Drive manual provider session is missing an access token");
  if (!refreshTokenReady) blockers.push("Google Drive manual provider session is missing a refresh token");
  if (!unexpired) blockers.push("Google Drive manual provider access token is expired or invalid");
  if (missingScopes.length > 0) {
    blockers.push(`Google Drive manual provider session missing scopes: ${missingScopes.join(", ")}`);
  }
  if (!selectedRootReady) {
    blockers.push("Google Drive manual provider session has not selected a Drive root folder");
  }

  return {
    provider: "google-drive",
    connected,
    manual_session: manualSession,
    access_token_ready: accessTokenReady && unexpired,
    refresh_token_ready: refreshTokenReady,
    required_scopes_ready: missingScopes.length === 0,
    selected_root_ready: selectedRootReady,
    oauth_exchange_boundary_ready: true,
    root_execution_boundary_ready: true,
    default_sync_enabled: false,
    ready_for_explicit_api_transport: blockers.length === 0,
    blockers,
  };
}

export function buildGoogleDriveManualSetupPlan({
  clientId,
  clientSecret,
  redirectUri,
  state,
  codeChallenge,
  rootName = "Starlane Notes",
}: {
  clientId?: string;
  clientSecret?: string;
  redirectUri?: string;
  state?: string;
  codeChallenge?: string;
  rootName?: string;
} = {}): GoogleDriveManualSetupPlan {
  const normalizedClientId = clientId?.trim() ?? "";
  const normalizedClientSecret = clientSecret?.trim() ?? "";
  const normalizedRedirectUri = redirectUri?.trim() ?? "";
  const normalizedState = state?.trim() ?? "";
  const normalizedCodeChallenge = codeChallenge?.trim() ?? "";
  const blockers: string[] = [];
  const redirectEnvironment = classifyGoogleDriveOAuthRedirectEnvironment(normalizedRedirectUri);

  if (!normalizedClientId) blockers.push("Google Drive setup requires a public OAuth client ID");
  if (normalizedClientSecret) {
    blockers.push("Google Drive browser setup must not expose or send an OAuth client secret");
  }
  if (!normalizedRedirectUri) blockers.push("Google Drive setup requires an OAuth redirect URI");
  if (!normalizedState) blockers.push("Google Drive setup requires a CSRF state value");
  if (!normalizedCodeChallenge) blockers.push("Google Drive setup requires a PKCE code challenge");

  const authorizationRequest =
    blockers.length === 0
      ? buildGoogleDriveOAuthAuthorizationRequest({
          clientId: normalizedClientId,
          redirectUri: normalizedRedirectUri,
          state: normalizedState,
          codeChallenge: normalizedCodeChallenge,
        })
      : null;

  return {
    provider: "google-drive",
    status: blockers.length === 0 ? "ready_to_start" : "blocked",
    oauth_client_type: "public_pkce",
    oauth_redirect_environment: redirectEnvironment,
    production_oauth_candidate:
      blockers.length === 0 &&
      (redirectEnvironment === "production_https" ||
        redirectEnvironment === "native_custom_scheme"),
    client_id_configured: Boolean(normalizedClientId),
    client_secret_configured: Boolean(normalizedClientSecret),
    client_secret_allowed: false,
    redirect_uri_configured: Boolean(normalizedRedirectUri),
    pkce_required: true,
    required_scopes: defaultProviderScopes("google-drive"),
    oauth_authorization_url: authorizationRequest?.auth_url,
    token_exchange_boundary: "injected_transport",
    root_resolution_boundary: "injected_api_transport",
    persistence_boundary: "provider_session_record",
    default_sync_enabled: false,
    blockers,
    steps: [
      "Generate a CSRF state value and PKCE verifier/challenge in the app shell.",
      "Open the Google OAuth authorization URL with a public PKCE client, offline access, and Drive file scopes.",
      "Exchange the returned authorization code through an injected token transport without a client secret.",
      `Resolve or create the '${rootName}' Drive root through an injected API transport.`,
      "Persist the completed manual provider session only after token and root setup succeed.",
    ],
  };
}

export function classifyGoogleDriveOAuthRedirectEnvironment(
  redirectUri: string,
): GoogleDriveManualSetupPlan["oauth_redirect_environment"] {
  const normalizedRedirectUri = redirectUri.trim();
  if (!normalizedRedirectUri) return "unconfigured";

  try {
    const parsed = new URL(normalizedRedirectUri);
    if (parsed.protocol === "starlane:") return "native_custom_scheme";
    if (parsed.protocol !== "https:") return "local_development";

    const hostname = parsed.hostname.toLowerCase();
    if (
      hostname === "localhost" ||
      hostname === "127.0.0.1" ||
      hostname === "::1" ||
      hostname.endsWith(".localhost")
    ) {
      return "local_development";
    }

    return "production_https";
  } catch {
    return "local_development";
  }
}

export function formatGoogleDriveOAuthRedirectEnvironmentForEvidence(
  environment: GoogleDriveManualSetupPlan["oauth_redirect_environment"],
): string {
  switch (environment) {
    case "production_https":
      return "production HTTPS";
    case "native_custom_scheme":
      return "native callback";
    case "local_development":
      return "local/dev";
    case "unconfigured":
    default:
      return "missing";
  }
}

export function buildGoogleDriveProductionOAuthEvidenceUpdate({
  setupPlan,
  existingEvidence,
  verifiedAt,
}: {
  setupPlan: GoogleDriveManualSetupPlan;
  existingEvidence: ProviderDefaultSyncValidationEvidenceRecord;
  verifiedAt: string;
}): GoogleDriveProductionOAuthEvidenceUpdate {
  const retainedNotes = existingEvidence.notes.filter(
    (note) =>
      !note.includes("production OAuth completion") &&
      !note.includes("live OAuth completed through"),
  );

  if (setupPlan.production_oauth_candidate) {
    return {
      productionVerifiedNow: true,
      evidence: {
        ...existingEvidence,
        provider: "google-drive",
        production_oauth_verified: true,
        production_oauth_verified_at: verifiedAt,
        notes: [
          `Google Drive production OAuth completed through live Google API transports at ${verifiedAt}.`,
          ...retainedNotes,
        ],
      },
    };
  }

  return {
    productionVerifiedNow: false,
    evidence: {
      ...existingEvidence,
      provider: "google-drive",
      notes: [
        `Google Drive live OAuth completed through ${formatGoogleDriveOAuthRedirectEnvironmentForEvidence(
          setupPlan.oauth_redirect_environment,
        )} redirect at ${verifiedAt}; this does not count as production OAuth validation.`,
        ...retainedNotes,
      ],
    },
  };
}

export async function createGoogleDriveOAuthBootstrap({
  verifierBytes,
  stateBytes,
  now = new Date(),
}: {
  verifierBytes?: Uint8Array;
  stateBytes?: Uint8Array;
  now?: Date;
} = {}): Promise<GoogleDriveOAuthBootstrap> {
  const verifierSource = verifierBytes ?? randomBytes(32);
  const stateSource = stateBytes ?? randomBytes(16);
  const codeVerifier = base64UrlEncode(verifierSource);
  const state = base64UrlEncode(stateSource);
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(codeVerifier),
  );

  return {
    state,
    code_verifier: codeVerifier,
    code_challenge: base64UrlEncode(new Uint8Array(digest)),
    code_challenge_method: "S256",
    created_at: now.toISOString(),
  };
}

export function createDisconnectedProviderSession(
  provider: SyncProviderKind,
): ProviderSessionRecord {
  return {
    provider,
    connected: false,
    auth: null,
    sync_mode: "manual",
    last_operation: "disconnect",
  };
}

export function createConnectedProviderSession(
  provider: SyncProviderKind,
): ProviderSessionRecord {
  return {
    provider,
    connected: true,
    auth: createProviderAuthSession(provider),
    sync_mode: "simulated",
    last_operation: "connect",
  };
}

export function readProviderSession(provider: SyncProviderKind): ProviderSessionRecord {
  const storage = safeLocalStorage();
  if (!storage) return createDisconnectedProviderSession(provider);

  const raw = storage.getItem(storageKey(provider));
  if (!raw) return createDisconnectedProviderSession(provider);

  try {
    const parsed = JSON.parse(raw) as ProviderSessionRecord;
    if (parsed.provider !== provider) return createDisconnectedProviderSession(provider);
    return parsed;
  } catch {
    return createDisconnectedProviderSession(provider);
  }
}

export function writeProviderSession(
  provider: SyncProviderKind,
  session: ProviderSessionRecord,
): void {
  const storage = safeLocalStorage();
  if (!storage) return;
  storage.setItem(storageKey(provider), JSON.stringify(session));
}

export function readProviderRemoteSyncEnablement(
  provider: SyncProviderKind,
): ProviderRemoteSyncEnablementRecord {
  const storage = safeLocalStorage();
  const fallback: ProviderRemoteSyncEnablementRecord = {
    provider,
    enabled: false,
    enablement_boundary: "explicit_manual_provider_session",
  };
  if (!storage) return fallback;
  const raw = storage.getItem(remoteSyncEnablementStorageKey(provider));
  if (!raw) return fallback;
  try {
    const parsed = JSON.parse(raw) as Partial<ProviderRemoteSyncEnablementRecord>;
    return {
      provider,
      enabled: parsed.enabled === true,
      enablement_boundary: "explicit_manual_provider_session",
      enabled_at: typeof parsed.enabled_at === "string" ? parsed.enabled_at : undefined,
      disabled_at: typeof parsed.disabled_at === "string" ? parsed.disabled_at : undefined,
      reason: typeof parsed.reason === "string" ? parsed.reason : undefined,
    };
  } catch {
    return fallback;
  }
}

export function writeProviderRemoteSyncEnablement(
  provider: SyncProviderKind,
  enablement: ProviderRemoteSyncEnablementRecord,
): void {
  const storage = safeLocalStorage();
  if (!storage) return;
  storage.setItem(
    remoteSyncEnablementStorageKey(provider),
    JSON.stringify({
      ...enablement,
      provider,
      enablement_boundary: "explicit_manual_provider_session",
    }),
  );
}

export function readProviderDefaultSyncValidationEvidence(
  provider: SyncProviderKind,
): ProviderDefaultSyncValidationEvidenceRecord {
  const storage = safeLocalStorage();
  const fallback: ProviderDefaultSyncValidationEvidenceRecord = {
    provider,
    production_oauth_verified: false,
    device_background_wake_verified: false,
    notes: [],
  };
  if (!storage) return fallback;
  const raw = storage.getItem(defaultSyncValidationEvidenceStorageKey(provider));
  if (!raw) return fallback;
  try {
    const parsed = JSON.parse(raw) as Partial<ProviderDefaultSyncValidationEvidenceRecord>;
    return {
      provider,
      production_oauth_verified: parsed.production_oauth_verified === true,
      device_background_wake_verified: parsed.device_background_wake_verified === true,
      production_oauth_verified_at:
        typeof parsed.production_oauth_verified_at === "string"
          ? parsed.production_oauth_verified_at
          : undefined,
      device_background_wake_verified_at:
        typeof parsed.device_background_wake_verified_at === "string"
          ? parsed.device_background_wake_verified_at
          : undefined,
      notes: Array.isArray(parsed.notes)
        ? parsed.notes.filter((note): note is string => typeof note === "string")
        : [],
    };
  } catch {
    return fallback;
  }
}

export function writeProviderDefaultSyncValidationEvidence(
  provider: SyncProviderKind,
  evidence: ProviderDefaultSyncValidationEvidenceRecord,
): void {
  const storage = safeLocalStorage();
  if (!storage) return;
  storage.setItem(
    defaultSyncValidationEvidenceStorageKey(provider),
    JSON.stringify({ ...evidence, provider }),
  );
}

export function clearProviderSession(provider: SyncProviderKind): void {
  const storage = safeLocalStorage();
  if (!storage) return;
  storage.removeItem(storageKey(provider));
  storage.removeItem(defaultSyncValidationEvidenceStorageKey(provider));
}

export function summarizeProviderSession(
  session: ProviderSessionRecord,
): ProviderSessionSummary {
  return {
    provider: session.provider,
    connected: session.connected,
    account_label: session.auth?.account_label,
    scopes: session.auth?.scopes,
    sync_mode: session.sync_mode,
    issued_at: session.auth?.issued_at,
    expires_at: session.auth?.expires_at,
    refresh_token_ready: Boolean(session.auth?.refresh_token?.trim()),
    provider_root_id: session.auth?.provider_root_id,
    provider_root_name: session.auth?.provider_root_name,
  };
}

export function createUnsupportedRemoteSyncError(
  providerLabel: string,
  operation: "pull" | "push",
): Error {
  const error = new Error(
    `${providerLabel} does not support real remote ${operation} yet; this path is staged only.`,
  );
  (error as Error & { code?: string }).code = "unsupported_remote_sync";
  return error;
}

export function createProviderNotConnectedError(
  providerLabel: string,
  operation: "pull" | "push",
): Error {
  const error = new Error(`${providerLabel} must be connected before ${operation}.`);
  (error as Error & { code?: string }).code = "provider_not_connected";
  return error;
}
