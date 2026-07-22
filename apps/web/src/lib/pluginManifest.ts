export const PLUGIN_MANIFEST_SCHEMA_VERSION = 1;
export const PLUGIN_REGISTRY_INDEX_SCHEMA_VERSION = 1;

export const SERVER_PLUGIN_PERMISSIONS = [
  "read_docs",
  "write_docs",
  "read_spaces",
  "write_spaces",
  "read_users",
  "http_fetch",
  "search",
  "db_read",
  "db_write",
] as const;

export const CLIENT_PLUGIN_PERMISSIONS = [
  "editor_commands",
  "editor_blocks",
  "sidebar_panel",
  "toolbar_button",
  "properties",
  "context_menu",
  "settings_page",
] as const;

const SETTING_TYPES = ["string", "number", "boolean", "select", "secret"] as const;

export type ServerPluginPermission = (typeof SERVER_PLUGIN_PERMISSIONS)[number];
export type ClientPluginPermission = (typeof CLIENT_PLUGIN_PERMISSIONS)[number];
export type PluginSettingType = (typeof SETTING_TYPES)[number];

export interface PluginManifestSettingsField {
  key: string;
  label: string;
  type: PluginSettingType;
  required?: boolean;
  options?: string[];
}

export interface PluginManifest {
  schema_version: number;
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  license: string;
  homepage?: string;
  starlane_version: string;
  permissions: {
    server: ServerPluginPermission[];
    client: ClientPluginPermission[];
  };
  http_fetch?: string[];
  settings_schema: PluginManifestSettingsField[];
}

export interface PluginManifestValidationResult {
  manifest: PluginManifest | null;
  errors: string[];
  warnings: string[];
}

export interface PluginRegistryIndexEntry {
  id: string;
  name: string;
  version: string;
  description: string;
  manifest_url: string;
  source_url?: string;
  homepage?: string;
  author?: string;
  license?: string;
  categories: string[];
  verified: boolean;
}

export interface PluginRegistryIndex {
  schema_version: number;
  generated_at?: string;
  plugins: PluginRegistryIndexEntry[];
}

export interface PluginRegistryIndexValidationResult {
  index: PluginRegistryIndex | null;
  errors: string[];
  warnings: string[];
}

export type PluginPermissionReviewLevel = "low" | "medium" | "high";

export interface PluginPermissionReviewItem {
  id: string;
  label: string;
  description: string;
  level: PluginPermissionReviewLevel;
}

export interface PluginPermissionReview {
  level: PluginPermissionReviewLevel;
  items: PluginPermissionReviewItem[];
}

const SERVER_PERMISSION_SET = new Set<string>(SERVER_PLUGIN_PERMISSIONS);
const CLIENT_PERMISSION_SET = new Set<string>(CLIENT_PLUGIN_PERMISSIONS);
const SETTING_TYPE_SET = new Set<string>(SETTING_TYPES);
const PLUGIN_ID_PATTERN = /^[a-z][a-z0-9]*(?:[.-][a-z0-9]+)+$/;
const SEMVER_PATTERN = /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;

export function validatePluginManifest(value: unknown): PluginManifestValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];

  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return { manifest: null, errors: ["Manifest must be a JSON object."], warnings };
  }

  const source = value as Record<string, unknown>;
  const id = readString(source, "id", errors);
  const name = readString(source, "name", errors);
  const version = readString(source, "version", errors);
  const description = readString(source, "description", errors);
  const author = readString(source, "author", errors);
  const license = readString(source, "license", errors);
  const starlaneVersion = readString(source, "starlane_version", errors);
  const homepage = readOptionalUrl(source, "homepage", errors);

  if (id && !PLUGIN_ID_PATTERN.test(id)) {
    errors.push("id must use reverse-DNS style lowercase segments, such as com.example.plugin.");
  }
  if (version && !SEMVER_PATTERN.test(version)) {
    errors.push("version must be a semantic version such as 1.0.0.");
  }

  const serverPermissions = readPermissionList(
    source,
    "server",
    SERVER_PERMISSION_SET,
    "permissions.server",
    errors,
  ) as ServerPluginPermission[];
  const clientPermissions = readPermissionList(
    source,
    "client",
    CLIENT_PERMISSION_SET,
    "permissions.client",
    errors,
  ) as ClientPluginPermission[];
  const httpFetch = readHttpFetchDomains(source, errors);
  const settingsSchema = readSettingsSchema(source, errors);

  if (serverPermissions.includes("http_fetch") && httpFetch.length === 0) {
    errors.push("permissions.server includes http_fetch, so http_fetch must list allowed domains.");
  }
  if (!serverPermissions.includes("http_fetch") && httpFetch.length > 0) {
    warnings.push("http_fetch domains are ignored unless permissions.server includes http_fetch.");
  }

  if (errors.length > 0) {
    return { manifest: null, errors, warnings };
  }

  return {
    manifest: {
      schema_version: PLUGIN_MANIFEST_SCHEMA_VERSION,
      id,
      name,
      version,
      description,
      author,
      license,
      homepage,
      starlane_version: starlaneVersion,
      permissions: {
        server: serverPermissions,
        client: clientPermissions,
      },
      http_fetch: httpFetch.length > 0 ? httpFetch : undefined,
      settings_schema: settingsSchema,
    },
    errors,
    warnings,
  };
}

export function summarizePluginPermissionReview(manifest: PluginManifest): PluginPermissionReview {
  const items: PluginPermissionReviewItem[] = [];

  for (const permission of manifest.permissions.server) {
    items.push(describeServerPermission(permission));
  }
  for (const permission of manifest.permissions.client) {
    items.push(describeClientPermission(permission));
  }
  if (manifest.http_fetch && manifest.http_fetch.length > 0) {
    items.push({
      id: "http_fetch_domains",
      label: "Network allowlist",
      description: `May fetch ${manifest.http_fetch.length} approved domain${manifest.http_fetch.length === 1 ? "" : "s"}.`,
      level: "high",
    });
  }

  const secretSettings = manifest.settings_schema.filter((field) => field.type === "secret");
  if (secretSettings.length > 0) {
    items.push({
      id: "secret_settings",
      label: "Secret settings",
      description: `Requests ${secretSettings.length} secret setting${secretSettings.length === 1 ? "" : "s"} for credentials or tokens.`,
      level: "high",
    });
  }

  if (items.length === 0) {
    items.push({
      id: "no_permissions",
      label: "No runtime permissions",
      description: "Manifest does not request server, client, network, or secret-setting access.",
      level: "low",
    });
  }

  return {
    level: highestReviewLevel(items),
    items,
  };
}

export function validatePluginRegistryIndex(value: unknown): PluginRegistryIndexValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];

  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return { index: null, errors: ["Registry index must be a JSON object."], warnings };
  }

  const source = value as Record<string, unknown>;
  const schemaVersion = source.schema_version;
  if (schemaVersion !== PLUGIN_REGISTRY_INDEX_SCHEMA_VERSION) {
    errors.push(`schema_version must be ${PLUGIN_REGISTRY_INDEX_SCHEMA_VERSION}.`);
  }

  const generatedAt = source.generated_at;
  if (
    generatedAt !== undefined &&
    (typeof generatedAt !== "string" || Number.isNaN(new Date(generatedAt).getTime()))
  ) {
    errors.push("generated_at must be an ISO date string when provided.");
  }

  const plugins = source.plugins;
  if (!Array.isArray(plugins)) {
    errors.push("plugins must be an array.");
    return { index: null, errors, warnings };
  }

  const seen = new Set<string>();
  const entries = plugins
    .map((item, index) => readRegistryIndexEntry(item, index, seen, errors, warnings))
    .filter((entry): entry is PluginRegistryIndexEntry => entry !== null);

  if (errors.length > 0) return { index: null, errors, warnings };

  return {
    index: {
      schema_version: PLUGIN_REGISTRY_INDEX_SCHEMA_VERSION,
      generated_at: typeof generatedAt === "string" ? generatedAt : undefined,
      plugins: entries.sort((a, b) => a.name.localeCompare(b.name)),
    },
    errors,
    warnings,
  };
}

function describeServerPermission(permission: ServerPluginPermission): PluginPermissionReviewItem {
  switch (permission) {
    case "write_docs":
      return {
        id: "server_write_docs",
        label: "Write documents",
        description: "Can create or modify document content when runtime execution is enabled.",
        level: "high",
      };
    case "write_spaces":
      return {
        id: "server_write_spaces",
        label: "Write spaces",
        description: "Can change space-level records when runtime execution is enabled.",
        level: "high",
      };
    case "read_users":
      return {
        id: "server_read_users",
        label: "Read users",
        description: "Can inspect user-directory data exposed to plugin runtime calls.",
        level: "high",
      };
    case "http_fetch":
      return {
        id: "server_http_fetch",
        label: "Network fetch",
        description: "Can make outbound HTTP requests to its manifest allowlist.",
        level: "high",
      };
    case "db_write":
      return {
        id: "server_db_write",
        label: "Plugin database write",
        description: "Can write plugin-owned database records.",
        level: "medium",
      };
    case "read_docs":
      return {
        id: "server_read_docs",
        label: "Read documents",
        description: "Can read document content available to the plugin runtime.",
        level: "medium",
      };
    case "read_spaces":
      return {
        id: "server_read_spaces",
        label: "Read spaces",
        description: "Can read space metadata available to the plugin runtime.",
        level: "medium",
      };
    case "search":
      return {
        id: "server_search",
        label: "Search",
        description: "Can query readable document indexes through plugin runtime calls.",
        level: "medium",
      };
    case "db_read":
      return {
        id: "server_db_read",
        label: "Plugin database read",
        description: "Can read plugin-owned database records.",
        level: "low",
      };
  }
  return assertNever(permission);
}

function describeClientPermission(permission: ClientPluginPermission): PluginPermissionReviewItem {
  switch (permission) {
    case "editor_commands":
      return {
        id: "client_editor_commands",
        label: "Editor commands",
        description: "Can add command-palette/editor actions when the client sandbox exists.",
        level: "medium",
      };
    case "editor_blocks":
      return {
        id: "client_editor_blocks",
        label: "Editor blocks",
        description: "Can add custom document blocks when the client sandbox exists.",
        level: "medium",
      };
    case "properties":
      return {
        id: "client_properties",
        label: "Properties UI",
        description: "Can extend property surfaces when the client sandbox exists.",
        level: "medium",
      };
    case "context_menu":
      return {
        id: "client_context_menu",
        label: "Context menu",
        description: "Can add contextual actions when the client sandbox exists.",
        level: "medium",
      };
    case "sidebar_panel":
      return {
        id: "client_sidebar_panel",
        label: "Sidebar panel",
        description: "Can add a sidebar panel when the client sandbox exists.",
        level: "low",
      };
    case "toolbar_button":
      return {
        id: "client_toolbar_button",
        label: "Toolbar button",
        description: "Can add a toolbar action when the client sandbox exists.",
        level: "low",
      };
    case "settings_page":
      return {
        id: "client_settings_page",
        label: "Settings page",
        description: "Can expose plugin settings when the client sandbox exists.",
        level: "low",
      };
  }
  return assertNever(permission);
}

function highestReviewLevel(items: PluginPermissionReviewItem[]): PluginPermissionReviewLevel {
  if (items.some((item) => item.level === "high")) return "high";
  if (items.some((item) => item.level === "medium")) return "medium";
  return "low";
}

function assertNever(value: never): never {
  throw new Error(`Unhandled plugin permission: ${value}`);
}

function readString(source: Record<string, unknown>, key: string, errors: string[]): string {
  const value = source[key];
  if (typeof value === "string" && value.trim()) return value.trim();
  errors.push(`${key} is required.`);
  return "";
}

function readOptionalUrl(
  source: Record<string, unknown>,
  key: string,
  errors: string[],
): string | undefined {
  const value = source[key];
  if (value === undefined || value === null || value === "") return undefined;
  if (typeof value !== "string") {
    errors.push(`${key} must be a URL string.`);
    return undefined;
  }
  try {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      errors.push(`${key} must use http or https.`);
    }
    return url.toString();
  } catch {
    errors.push(`${key} must be a valid URL.`);
    return undefined;
  }
}

function readRegistryIndexEntry(
  value: unknown,
  index: number,
  seen: Set<string>,
  errors: string[],
  warnings: string[],
): PluginRegistryIndexEntry | null {
  const path = `plugins[${index}]`;
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    errors.push(`${path} must be an object.`);
    return null;
  }

  const source = value as Record<string, unknown>;
  const id = readRequiredStringAtPath(source, "id", `${path}.id`, errors);
  const name = readRequiredStringAtPath(source, "name", `${path}.name`, errors);
  const version = readRequiredStringAtPath(source, "version", `${path}.version`, errors);
  const description = readRequiredStringAtPath(
    source,
    "description",
    `${path}.description`,
    errors,
  );
  const manifestUrl = readRequiredHttpUrlAtPath(
    source,
    "manifest_url",
    `${path}.manifest_url`,
    errors,
  );
  const sourceUrl = readOptionalHttpUrlAtPath(source, "source_url", `${path}.source_url`, errors);
  const homepage = readOptionalHttpUrlAtPath(source, "homepage", `${path}.homepage`, errors);
  const author = readOptionalStringAtPath(source, "author", `${path}.author`, errors);
  const license = readOptionalStringAtPath(source, "license", `${path}.license`, errors);
  const categories = readRegistryCategories(source, `${path}.categories`, errors, warnings);
  const verified = source.verified === true;

  if (id && !PLUGIN_ID_PATTERN.test(id)) {
    errors.push(`${path}.id must use reverse-DNS style lowercase segments.`);
  }
  if (version && !SEMVER_PATTERN.test(version)) {
    errors.push(`${path}.version must be a semantic version such as 1.0.0.`);
  }
  if (id && version) {
    const key = `${id}@${version}`;
    if (seen.has(key)) {
      errors.push(`${path} duplicates plugin registry entry ${key}.`);
    }
    seen.add(key);
  }
  if (source.verified !== undefined && typeof source.verified !== "boolean") {
    errors.push(`${path}.verified must be a boolean when provided.`);
  }
  if (!sourceUrl && !homepage) {
    warnings.push(`${path} has no source_url or homepage for operator review.`);
  }

  return {
    id,
    name,
    version,
    description,
    manifest_url: manifestUrl,
    source_url: sourceUrl,
    homepage,
    author,
    license,
    categories,
    verified,
  };
}

function readRequiredStringAtPath(
  source: Record<string, unknown>,
  key: string,
  path: string,
  errors: string[],
): string {
  const value = source[key];
  if (typeof value === "string" && value.trim()) return value.trim();
  errors.push(`${path} is required.`);
  return "";
}

function readOptionalStringAtPath(
  source: Record<string, unknown>,
  key: string,
  path: string,
  errors: string[],
): string | undefined {
  const value = source[key];
  if (value === undefined || value === null || value === "") return undefined;
  if (typeof value === "string" && value.trim()) return value.trim();
  errors.push(`${path} must be a string when provided.`);
  return undefined;
}

function readRequiredHttpUrlAtPath(
  source: Record<string, unknown>,
  key: string,
  path: string,
  errors: string[],
): string {
  const raw = source[key];
  if (raw === undefined || raw === null || raw === "") {
    errors.push(`${path} is required.`);
    return "";
  }
  const value = readOptionalHttpUrlAtPath(source, key, path, errors);
  return value ?? "";
}

function readOptionalHttpUrlAtPath(
  source: Record<string, unknown>,
  key: string,
  path: string,
  errors: string[],
): string | undefined {
  const value = source[key];
  if (value === undefined || value === null || value === "") return undefined;
  if (typeof value !== "string") {
    errors.push(`${path} must be a URL string.`);
    return undefined;
  }
  try {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      errors.push(`${path} must use http or https.`);
    }
    return url.toString();
  } catch {
    errors.push(`${path} must be a valid URL.`);
    return undefined;
  }
}

function readRegistryCategories(
  source: Record<string, unknown>,
  path: string,
  errors: string[],
  warnings: string[],
): string[] {
  const value = source.categories;
  if (value === undefined) return [];
  if (!Array.isArray(value)) {
    errors.push(`${path} must be an array.`);
    return [];
  }

  const categories: string[] = [];
  for (const item of value) {
    if (typeof item !== "string" || !item.trim()) {
      errors.push(`${path} entries must be non-empty strings.`);
      continue;
    }
    const category = item.trim().toLowerCase();
    if (!/^[a-z][a-z0-9-]{0,31}$/.test(category)) {
      errors.push(`${path} includes invalid category: ${item}.`);
      continue;
    }
    if (!categories.includes(category)) categories.push(category);
  }
  if (categories.length > 8) {
    warnings.push(`${path} has more than 8 categories; registry UI may truncate them.`);
  }
  return categories;
}

function readPermissionList(
  source: Record<string, unknown>,
  key: "server" | "client",
  allowed: Set<string>,
  path: string,
  errors: string[],
): string[] {
  const permissions = source.permissions;
  if (!permissions || typeof permissions !== "object" || Array.isArray(permissions)) {
    errors.push("permissions is required.");
    return [];
  }

  const value = (permissions as Record<string, unknown>)[key];
  if (value === undefined) return [];
  if (!Array.isArray(value)) {
    errors.push(`${path} must be an array.`);
    return [];
  }

  const result: string[] = [];
  for (const item of value) {
    if (typeof item !== "string") {
      errors.push(`${path} entries must be strings.`);
      continue;
    }
    const normalized = item.trim();
    if (!allowed.has(normalized)) {
      errors.push(`${path} includes unknown permission: ${normalized}.`);
      continue;
    }
    if (!result.includes(normalized)) result.push(normalized);
  }
  return result;
}

function readHttpFetchDomains(source: Record<string, unknown>, errors: string[]): string[] {
  const value = source.http_fetch;
  if (value === undefined) return [];
  if (!Array.isArray(value)) {
    errors.push("http_fetch must be an array of allowed domains.");
    return [];
  }

  const domains: string[] = [];
  for (const item of value) {
    if (typeof item !== "string" || !item.trim()) {
      errors.push("http_fetch entries must be non-empty domain strings.");
      continue;
    }
    const domain = item.trim().toLowerCase();
    if (!isValidDomain(domain)) {
      errors.push(`http_fetch includes invalid domain: ${item}.`);
      continue;
    }
    if (!domains.includes(domain)) domains.push(domain);
  }
  return domains;
}

function readSettingsSchema(
  source: Record<string, unknown>,
  errors: string[],
): PluginManifestSettingsField[] {
  const value = source.settings_schema;
  if (value === undefined) return [];
  if (!Array.isArray(value)) {
    errors.push("settings_schema must be an array.");
    return [];
  }

  const keys = new Set<string>();
  const fields: PluginManifestSettingsField[] = [];
  value.forEach((item, index) => {
    const path = `settings_schema[${index}]`;
    if (!item || typeof item !== "object" || Array.isArray(item)) {
      errors.push(`${path} must be an object.`);
      return;
    }
    const field = item as Record<string, unknown>;
    const key = typeof field.key === "string" ? field.key.trim() : "";
    const label = typeof field.label === "string" ? field.label.trim() : "";
    const type = typeof field.type === "string" ? field.type.trim() : "";
    if (!/^[a-z][a-z0-9_]*$/.test(key)) errors.push(`${path}.key is invalid.`);
    if (keys.has(key)) errors.push(`${path}.key must be unique.`);
    if (!label) errors.push(`${path}.label is required.`);
    if (!SETTING_TYPE_SET.has(type)) errors.push(`${path}.type is invalid.`);
    const options = readSettingOptions(field, type, path, errors);
    keys.add(key);
    fields.push({
      key,
      label,
      type: type as PluginSettingType,
      required: field.required === true,
      options,
    });
  });
  return fields;
}

function readSettingOptions(
  field: Record<string, unknown>,
  type: string,
  path: string,
  errors: string[],
): string[] | undefined {
  if (field.options === undefined) {
    if (type === "select") errors.push(`${path}.options is required for select settings.`);
    return undefined;
  }
  if (!Array.isArray(field.options)) {
    errors.push(`${path}.options must be an array of strings.`);
    return undefined;
  }
  const options = field.options
    .filter((option): option is string => typeof option === "string" && option.trim().length > 0)
    .map((option) => option.trim());
  if (options.length !== field.options.length) {
    errors.push(`${path}.options entries must be non-empty strings.`);
  }
  if (type !== "select" && options.length > 0) {
    errors.push(`${path}.options is only supported for select settings.`);
  }
  return options.length > 0 ? options : undefined;
}

function isValidDomain(value: string): boolean {
  if (value.includes("://") || value.includes("/") || value.includes("*")) return false;
  return /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)+$/.test(
    value,
  );
}
