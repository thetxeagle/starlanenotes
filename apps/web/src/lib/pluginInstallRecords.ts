import {
  summarizePluginPermissionReview,
  type ClientPluginPermission,
  type PluginManifest,
  type PluginRegistryIndexEntry,
  type ServerPluginPermission,
} from "@/lib/pluginManifest";

const PLUGIN_INSTALL_RECORDS_KEY = "starlane.pluginInstallRecords.v1";
const PLUGIN_INSTALL_AUDIT_EVENTS_KEY = "starlane.pluginInstallAuditEvents.v1";
const PLUGIN_RUNTIME_AUDIT_EVENTS_KEY = "starlane.pluginRuntimeAuditEvents.v1";
const MAX_PLUGIN_INSTALL_AUDIT_EVENTS = 50;
const MAX_PLUGIN_RUNTIME_AUDIT_EVENTS = 50;

export interface PluginInstallRecord {
  id: string;
  manifest: PluginManifest;
  installedAt: string;
  enabled: boolean;
  enabledSpaceIds: string[];
  source: "pasted-manifest";
  sourceDetails: PluginInstallSourceDetails;
}

export type PluginInstallSourceType =
  | "pasted-manifest"
  | "registry-index"
  | "zip-upload"
  | "git-repository";

export interface PluginInstallSourceDetails {
  type: PluginInstallSourceType;
  label: string;
  capturedAt: string;
  manifestUrl?: string;
  sourceUrl?: string;
  homepage?: string;
  verified?: boolean;
}

export type PluginInstallAuditAction =
  | "install_record_saved"
  | "install_record_enabled"
  | "install_record_disabled"
  | "space_enabled"
  | "space_disabled"
  | "install_record_removed";

export interface PluginInstallAuditEvent {
  id: string;
  pluginId: string;
  pluginName: string;
  action: PluginInstallAuditAction;
  spaceId?: string;
  spaceName?: string;
  createdAt: string;
}

export type PluginRuntimeAuditAction =
  | "runtime_enable_blocked"
  | "runtime_space_enable_blocked";

export interface PluginRuntimeAuditEvent {
  id: string;
  pluginId: string;
  pluginName: string;
  action: PluginRuntimeAuditAction;
  runtimeState: PluginRuntimeStatus["runtime_state"];
  reason: string;
  spaceId?: string;
  spaceName?: string;
  createdAt: string;
}

export interface PluginRuntimeStatus {
  enabled: boolean;
  enabled_space_count: number;
  permission_review_level: "low" | "medium" | "high";
  runtime_state: "disabled" | "blocked_runtime_unavailable";
  reason: string;
}

/** The only capability request shape a future plugin host may dispatch. */
export type PluginRuntimeCapability =
  | { kind: "client"; permission: ClientPluginPermission }
  | { kind: "server"; permission: Exclude<ServerPluginPermission, "http_fetch"> }
  | { kind: "http_fetch"; url: string };

export type PluginRuntimeAccessReason =
  | "plugin_disabled"
  | "space_not_enabled"
  | "permission_not_declared"
  | "http_fetch_url_invalid"
  | "http_fetch_domain_not_allowed"
  | "runtime_unavailable";

export type PluginRuntimeAccessDecision = {
  allowed: true;
  runtime_state: "blocked_runtime_unavailable";
} | {
  allowed: false;
  reason: PluginRuntimeAccessReason;
  /** A future host must only run code after this guard changes to allow it. */
  runtime_state: PluginRuntimeStatus["runtime_state"];
};

export interface PluginRuntimeAuditFilter {
  pluginId?: string;
  action?: PluginRuntimeAuditAction;
  spaceId?: string;
}

export function readPluginInstallRecords(): PluginInstallRecord[] {
  const storage = getBrowserLocalStorage();
  if (!storage) return [];
  try {
    const parsed = JSON.parse(storage.getItem(PLUGIN_INSTALL_RECORDS_KEY) ?? "[]") as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map(normalizePluginInstallRecord)
      .filter((record): record is PluginInstallRecord => record !== null)
      .sort((a, b) => a.manifest.name.localeCompare(b.manifest.name));
  } catch {
    return [];
  }
}

export function writePluginInstallRecords(records: PluginInstallRecord[]): void {
  const storage = getBrowserLocalStorage();
  if (!storage) return;
  storage.setItem(PLUGIN_INSTALL_RECORDS_KEY, JSON.stringify(records));
}

export function readPluginInstallAuditEvents(): PluginInstallAuditEvent[] {
  const storage = getBrowserLocalStorage();
  if (!storage) return [];
  try {
    const parsed = JSON.parse(storage.getItem(PLUGIN_INSTALL_AUDIT_EVENTS_KEY) ?? "[]") as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map(normalizePluginInstallAuditEvent)
      .filter((event): event is PluginInstallAuditEvent => event !== null)
      .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
      .slice(0, MAX_PLUGIN_INSTALL_AUDIT_EVENTS);
  } catch {
    return [];
  }
}

export function appendPluginInstallAuditEvent(
  events: PluginInstallAuditEvent[],
  input: Omit<PluginInstallAuditEvent, "id" | "createdAt">,
): PluginInstallAuditEvent[] {
  return [
    {
      id: makePluginInstallAuditEventId(),
      createdAt: new Date().toISOString(),
      ...input,
    },
    ...events,
  ]
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
    .slice(0, MAX_PLUGIN_INSTALL_AUDIT_EVENTS);
}

export function writePluginInstallAuditEvents(events: PluginInstallAuditEvent[]): void {
  const storage = getBrowserLocalStorage();
  if (!storage) return;
  storage.setItem(
    PLUGIN_INSTALL_AUDIT_EVENTS_KEY,
    JSON.stringify(events.slice(0, MAX_PLUGIN_INSTALL_AUDIT_EVENTS)),
  );
}

export function readPluginRuntimeAuditEvents(): PluginRuntimeAuditEvent[] {
  const storage = getBrowserLocalStorage();
  if (!storage) return [];
  try {
    const parsed = JSON.parse(storage.getItem(PLUGIN_RUNTIME_AUDIT_EVENTS_KEY) ?? "[]") as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map(normalizePluginRuntimeAuditEvent)
      .filter((event): event is PluginRuntimeAuditEvent => event !== null)
      .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
      .slice(0, MAX_PLUGIN_RUNTIME_AUDIT_EVENTS);
  } catch {
    return [];
  }
}

export function appendPluginRuntimeAuditEvent(
  events: PluginRuntimeAuditEvent[],
  input: Omit<PluginRuntimeAuditEvent, "id" | "createdAt">,
): PluginRuntimeAuditEvent[] {
  return [
    {
      id: makePluginInstallAuditEventId(),
      createdAt: new Date().toISOString(),
      ...input,
    },
    ...events,
  ]
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
    .slice(0, MAX_PLUGIN_RUNTIME_AUDIT_EVENTS);
}

export function writePluginRuntimeAuditEvents(events: PluginRuntimeAuditEvent[]): void {
  const storage = getBrowserLocalStorage();
  if (!storage) return;
  storage.setItem(
    PLUGIN_RUNTIME_AUDIT_EVENTS_KEY,
    JSON.stringify(events.slice(0, MAX_PLUGIN_RUNTIME_AUDIT_EVENTS)),
  );
}

export function filterPluginRuntimeAuditEvents(
  events: PluginRuntimeAuditEvent[],
  filter: PluginRuntimeAuditFilter = {},
): PluginRuntimeAuditEvent[] {
  return events.filter((event) => {
    if (filter.pluginId && event.pluginId !== filter.pluginId) return false;
    if (filter.action && event.action !== filter.action) return false;
    if (filter.spaceId && event.spaceId !== filter.spaceId) return false;
    return true;
  });
}

export function serializePluginRuntimeAuditEvents(
  events: PluginRuntimeAuditEvent[],
  format: "json" | "csv" = "json",
): string {
  if (format === "json") {
    return `${JSON.stringify(
      {
        schema_version: 1,
        exported_at: new Date().toISOString(),
        event_count: events.length,
        events,
      },
      null,
      2,
    )}\n`;
  }

  const rows = [
    [
      "id",
      "plugin_id",
      "plugin_name",
      "action",
      "runtime_state",
      "reason",
      "space_id",
      "space_name",
      "created_at",
    ],
    ...events.map((event) => [
      event.id,
      event.pluginId,
      event.pluginName,
      event.action,
      event.runtimeState,
      event.reason,
      event.spaceId ?? "",
      event.spaceName ?? "",
      event.createdAt,
    ]),
  ];
  return `${rows.map((row) => row.map(escapeCsvValue).join(",")).join("\n")}\n`;
}

export function upsertPluginInstallRecord(
  records: PluginInstallRecord[],
  manifest: PluginManifest,
  sourceDetails?: Partial<PluginInstallSourceDetails>,
): PluginInstallRecord[] {
  const existing = records.find((record) => record.id === manifest.id);
  const nextRecord: PluginInstallRecord = {
    id: manifest.id,
    manifest,
    installedAt: existing?.installedAt ?? new Date().toISOString(),
    enabled: existing?.enabled ?? false,
    enabledSpaceIds: existing?.enabledSpaceIds ?? [],
    source: "pasted-manifest",
    sourceDetails: normalizePluginInstallSourceDetails(
      {
        type: "pasted-manifest",
        label: "Pasted plugin.json",
        homepage: manifest.homepage,
        ...sourceDetails,
        capturedAt: sourceDetails?.capturedAt ?? new Date().toISOString(),
      },
      existing?.installedAt,
      manifest,
    ),
  };
  return [nextRecord, ...records.filter((record) => record.id !== manifest.id)].sort((a, b) =>
    a.manifest.name.localeCompare(b.manifest.name),
  );
}

export function getPluginInstallSourceDetailsForManifest(
  manifest: PluginManifest,
  registryEntries: PluginRegistryIndexEntry[] = [],
): Partial<PluginInstallSourceDetails> {
  const registryEntry = registryEntries.find(
    (entry) => entry.id === manifest.id && entry.version === manifest.version,
  );
  if (!registryEntry) {
    return {
      type: "pasted-manifest",
      label: "Pasted plugin.json",
      homepage: manifest.homepage,
    };
  }

  return {
    type: "registry-index",
    label: "Matched registry entry",
    manifestUrl: registryEntry.manifest_url,
    sourceUrl: registryEntry.source_url,
    homepage: registryEntry.homepage ?? manifest.homepage,
    verified: registryEntry.verified,
  };
}

export function formatPluginInstallSourceDetails(details: PluginInstallSourceDetails): string {
  switch (details.type) {
    case "registry-index":
      return details.verified === true ? `${details.label} / registry verified` : details.label;
    case "zip-upload":
      return `${details.label} / ZIP source`;
    case "git-repository":
      return `${details.label} / Git source`;
    case "pasted-manifest":
    default:
      return details.label;
  }
}

export function setPluginInstallRecordEnabled(
  records: PluginInstallRecord[],
  id: string,
  enabled: boolean,
): PluginInstallRecord[] {
  return records.map((record) => (record.id === id ? { ...record, enabled } : record));
}

export function setPluginInstallRecordSpaceEnabled(
  records: PluginInstallRecord[],
  id: string,
  spaceId: string,
  enabled: boolean,
): PluginInstallRecord[] {
  return records.map((record) => {
    if (record.id !== id) return record;
    const current = new Set(record.enabledSpaceIds);
    if (enabled) {
      current.add(spaceId);
    } else {
      current.delete(spaceId);
    }
    return { ...record, enabledSpaceIds: [...current].sort() };
  });
}

export function removePluginInstallRecord(
  records: PluginInstallRecord[],
  id: string,
): PluginInstallRecord[] {
  return records.filter((record) => record.id !== id);
}

export function getPluginRuntimeStatus(record: PluginInstallRecord): PluginRuntimeStatus {
  const review = summarizePluginPermissionReview(record.manifest);
  if (!record.enabled) {
    return {
      enabled: false,
      enabled_space_count: record.enabledSpaceIds.length,
      permission_review_level: review.level,
      runtime_state: "disabled",
      reason: "Plugin install record is saved but disabled.",
    };
  }

  return {
    enabled: true,
    enabled_space_count: record.enabledSpaceIds.length,
    permission_review_level: review.level,
    runtime_state: "blocked_runtime_unavailable",
    reason: "Plugin runtime execution is not available yet.",
  };
}

/**
 * Canonical deny-by-default sandbox gate. It deliberately returns no permissive
 * result while no client/server sandbox exists, but validates every future
 * dispatch against install state, per-space scope, manifest permissions, and
 * the HTTP hostname allowlist first.
 */
export function guardPluginRuntimeAccess(
  record: PluginInstallRecord,
  capability: PluginRuntimeCapability,
  spaceId?: string,
  options: { runtimeAvailable?: boolean } = {},
): PluginRuntimeAccessDecision {
  if (!record.enabled) return { allowed: false, reason: "plugin_disabled", runtime_state: "disabled" };
  if (spaceId && !record.enabledSpaceIds.includes(spaceId)) {
    return { allowed: false, reason: "space_not_enabled", runtime_state: "blocked_runtime_unavailable" };
  }
  if (capability.kind === "client" && !record.manifest.permissions.client.includes(capability.permission)) {
    return { allowed: false, reason: "permission_not_declared", runtime_state: "blocked_runtime_unavailable" };
  }
  if (capability.kind === "server" && !record.manifest.permissions.server.includes(capability.permission)) {
    return { allowed: false, reason: "permission_not_declared", runtime_state: "blocked_runtime_unavailable" };
  }
  if (capability.kind === "http_fetch") {
    if (!record.manifest.permissions.server.includes("http_fetch")) {
      return { allowed: false, reason: "permission_not_declared", runtime_state: "blocked_runtime_unavailable" };
    }
    let hostname: string;
    try {
      const parsed = new URL(capability.url);
      if (parsed.protocol !== "https:" || !parsed.hostname) throw new Error("invalid URL");
      hostname = parsed.hostname.toLowerCase();
    } catch {
      return { allowed: false, reason: "http_fetch_url_invalid", runtime_state: "blocked_runtime_unavailable" };
    }
    if (!(record.manifest.http_fetch ?? []).some((domain) => domain.toLowerCase() === hostname)) {
      return { allowed: false, reason: "http_fetch_domain_not_allowed", runtime_state: "blocked_runtime_unavailable" };
    }
  }
  if (options.runtimeAvailable) {
    return { allowed: true, runtime_state: "blocked_runtime_unavailable" };
  }
  return { allowed: false, reason: "runtime_unavailable", runtime_state: "blocked_runtime_unavailable" };
}

export function formatPluginInstallAuditAction(action: PluginInstallAuditAction): string {
  switch (action) {
    case "install_record_saved":
      return "Saved install record";
    case "install_record_enabled":
      return "Enabled plugin";
    case "install_record_disabled":
      return "Disabled plugin";
    case "space_enabled":
      return "Enabled for space";
    case "space_disabled":
      return "Disabled for space";
    case "install_record_removed":
      return "Removed install record";
  }
}

export function formatPluginRuntimeAuditAction(action: PluginRuntimeAuditAction): string {
  switch (action) {
    case "runtime_enable_blocked":
      return "Runtime enable blocked";
    case "runtime_space_enable_blocked":
      return "Space runtime enable blocked";
  }
}

function escapeCsvValue(value: string): string {
  if (!/[",\n\r]/.test(value)) return value;
  return `"${value.replace(/"/g, '""')}"`;
}

function normalizePluginInstallRecord(value: unknown): PluginInstallRecord | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Partial<PluginInstallRecord>;
  if (
    typeof record.id !== "string" ||
    !record.manifest ||
    typeof record.manifest !== "object" ||
    record.source !== "pasted-manifest"
  ) {
    return null;
  }
  return {
    id: record.id,
    manifest: record.manifest as PluginManifest,
    installedAt: typeof record.installedAt === "string" ? record.installedAt : "",
    enabled: record.enabled === true,
    enabledSpaceIds: Array.isArray(record.enabledSpaceIds)
      ? record.enabledSpaceIds.filter((spaceId): spaceId is string => typeof spaceId === "string")
      : [],
    source: "pasted-manifest",
    sourceDetails: normalizePluginInstallSourceDetails(
      record.sourceDetails,
      record.installedAt,
      record.manifest as PluginManifest,
    ),
  };
}

function normalizePluginInstallSourceDetails(
  value: unknown,
  fallbackCapturedAt: string | undefined,
  manifest?: PluginManifest,
): PluginInstallSourceDetails {
  if (value && typeof value === "object") {
    const details = value as Partial<PluginInstallSourceDetails>;
    if (
      typeof details.type === "string" &&
      isPluginInstallSourceType(details.type) &&
      typeof details.label === "string"
    ) {
      return {
        type: details.type,
        label: details.label,
        capturedAt:
          typeof details.capturedAt === "string"
            ? details.capturedAt
            : fallbackCapturedAt || new Date().toISOString(),
        manifestUrl: typeof details.manifestUrl === "string" ? details.manifestUrl : undefined,
        sourceUrl: typeof details.sourceUrl === "string" ? details.sourceUrl : undefined,
        homepage: typeof details.homepage === "string" ? details.homepage : manifest?.homepage,
        verified: details.verified === true,
      };
    }
  }

  return {
    type: "pasted-manifest",
    label: "Pasted plugin.json",
    capturedAt: fallbackCapturedAt || new Date().toISOString(),
    homepage: manifest?.homepage,
  };
}

function normalizePluginInstallAuditEvent(value: unknown): PluginInstallAuditEvent | null {
  if (!value || typeof value !== "object") return null;
  const event = value as Partial<PluginInstallAuditEvent>;
  if (
    typeof event.id !== "string" ||
    typeof event.pluginId !== "string" ||
    typeof event.pluginName !== "string" ||
    typeof event.action !== "string" ||
    typeof event.createdAt !== "string" ||
    !isPluginInstallAuditAction(event.action)
  ) {
    return null;
  }
  return {
    id: event.id,
    pluginId: event.pluginId,
    pluginName: event.pluginName,
    action: event.action,
    spaceId: typeof event.spaceId === "string" ? event.spaceId : undefined,
    spaceName: typeof event.spaceName === "string" ? event.spaceName : undefined,
    createdAt: event.createdAt,
  };
}

function normalizePluginRuntimeAuditEvent(value: unknown): PluginRuntimeAuditEvent | null {
  if (!value || typeof value !== "object") return null;
  const event = value as Partial<PluginRuntimeAuditEvent>;
  if (
    typeof event.id !== "string" ||
    typeof event.pluginId !== "string" ||
    typeof event.pluginName !== "string" ||
    typeof event.action !== "string" ||
    typeof event.runtimeState !== "string" ||
    typeof event.reason !== "string" ||
    typeof event.createdAt !== "string" ||
    !isPluginRuntimeAuditAction(event.action) ||
    !isPluginRuntimeState(event.runtimeState)
  ) {
    return null;
  }
  return {
    id: event.id,
    pluginId: event.pluginId,
    pluginName: event.pluginName,
    action: event.action,
    runtimeState: event.runtimeState,
    reason: event.reason,
    spaceId: typeof event.spaceId === "string" ? event.spaceId : undefined,
    spaceName: typeof event.spaceName === "string" ? event.spaceName : undefined,
    createdAt: event.createdAt,
  };
}

function isPluginInstallAuditAction(value: string): value is PluginInstallAuditAction {
  return [
    "install_record_saved",
    "install_record_enabled",
    "install_record_disabled",
    "space_enabled",
    "space_disabled",
    "install_record_removed",
  ].includes(value);
}

function isPluginRuntimeAuditAction(value: string): value is PluginRuntimeAuditAction {
  return ["runtime_enable_blocked", "runtime_space_enable_blocked"].includes(value);
}

function isPluginRuntimeState(value: string): value is PluginRuntimeStatus["runtime_state"] {
  return value === "disabled" || value === "blocked_runtime_unavailable";
}

function isPluginInstallSourceType(value: string): value is PluginInstallSourceType {
  return ["pasted-manifest", "registry-index", "zip-upload", "git-repository"].includes(value);
}

function makePluginInstallAuditEventId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `plugin-audit-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function getBrowserLocalStorage(): Storage | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage;
}
