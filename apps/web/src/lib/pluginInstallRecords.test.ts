import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginManifest } from "@/lib/pluginManifest";
import {
  appendPluginInstallAuditEvent,
  appendPluginRuntimeAuditEvent,
  filterPluginRuntimeAuditEvents,
  formatPluginInstallAuditAction,
  formatPluginInstallSourceDetails,
  formatPluginRuntimeAuditAction,
  getPluginInstallSourceDetailsForManifest,
  getPluginRuntimeStatus,
  guardPluginRuntimeAccess,
  readPluginInstallAuditEvents,
  readPluginInstallRecords,
  readPluginRuntimeAuditEvents,
  removePluginInstallRecord,
  setPluginInstallRecordEnabled,
  setPluginInstallRecordSpaceEnabled,
  serializePluginRuntimeAuditEvents,
  upsertPluginInstallRecord,
  writePluginInstallAuditEvents,
  writePluginInstallRecords,
  writePluginRuntimeAuditEvents,
} from "./pluginInstallRecords";

function makeManifest(overrides: Partial<PluginManifest> = {}): PluginManifest {
  return {
    schema_version: 1,
    id: "com.example.plugin",
    name: "Example Plugin",
    version: "1.0.0",
    description: "Adds useful commands.",
    author: "Jane Smith",
    license: "MIT",
    starlane_version: ">=0.1.0",
    permissions: { server: ["read_docs"], client: ["editor_commands"] },
    settings_schema: [],
    ...overrides,
  };
}

describe("plugin install records", () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      clear: () => store.clear(),
      getItem: (key: string) => store.get(key) ?? null,
      removeItem: (key: string) => store.delete(key),
      setItem: (key: string, value: string) => store.set(key, value),
    });
  });

  afterEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
  });

  it("upserts plugin manifests without enabling runtime execution", () => {
    const records = upsertPluginInstallRecord([], makeManifest());

    expect(records).toHaveLength(1);
    expect(records[0]).toMatchObject({
      id: "com.example.plugin",
      enabled: false,
      enabledSpaceIds: [],
      source: "pasted-manifest",
      sourceDetails: {
        type: "pasted-manifest",
        label: "Pasted plugin.json",
        homepage: undefined,
      },
    });

    const updated = upsertPluginInstallRecord(records, makeManifest({ version: "1.1.0" }));
    expect(updated).toHaveLength(1);
    expect(updated[0].manifest.version).toBe("1.1.0");
  });

  it("toggles, scopes, and removes records", () => {
    const records = upsertPluginInstallRecord([], makeManifest());
    const enabled = setPluginInstallRecordEnabled(records, "com.example.plugin", true);
    const scoped = setPluginInstallRecordSpaceEnabled(enabled, "com.example.plugin", "space-1", true);
    const unscoped = setPluginInstallRecordSpaceEnabled(scoped, "com.example.plugin", "space-1", false);

    expect(enabled[0].enabled).toBe(true);
    expect(scoped[0].enabledSpaceIds).toEqual(["space-1"]);
    expect(unscoped[0].enabledSpaceIds).toEqual([]);
    expect(removePluginInstallRecord(enabled, "com.example.plugin")).toEqual([]);
  });

  it("summarizes runtime status without enabling execution", () => {
    const records = upsertPluginInstallRecord([], makeManifest());
    expect(getPluginRuntimeStatus(records[0])).toMatchObject({
      enabled: false,
      enabled_space_count: 0,
      permission_review_level: "medium",
      runtime_state: "disabled",
    });

    const enabled = setPluginInstallRecordSpaceEnabled(
      setPluginInstallRecordEnabled(records, "com.example.plugin", true),
      "com.example.plugin",
      "space-1",
      true,
    );
    expect(getPluginRuntimeStatus(enabled[0])).toMatchObject({
      enabled: true,
      enabled_space_count: 1,
      permission_review_level: "medium",
      runtime_state: "blocked_runtime_unavailable",
      reason: "Plugin runtime execution is not available yet.",
    });
  });

  it("enforces install state, space scope, declared permissions, and fetch domains before runtime dispatch", () => {
    const [disabled] = upsertPluginInstallRecord([], makeManifest({
      permissions: { server: ["http_fetch"], client: ["editor_commands"] },
      http_fetch: ["api.example.com"],
    }));
    expect(guardPluginRuntimeAccess(disabled, { kind: "client", permission: "editor_commands" })).toMatchObject({ reason: "plugin_disabled" });

    const [enabled] = setPluginInstallRecordSpaceEnabled(
      setPluginInstallRecordEnabled([disabled], disabled.id, true), disabled.id, "space-1", true,
    );
    expect(guardPluginRuntimeAccess(enabled, { kind: "client", permission: "editor_commands" }, "space-2")).toMatchObject({ reason: "space_not_enabled" });
    expect(guardPluginRuntimeAccess(enabled, { kind: "client", permission: "sidebar_panel" }, "space-1")).toMatchObject({ reason: "permission_not_declared" });
    expect(guardPluginRuntimeAccess(enabled, { kind: "http_fetch", url: "http://api.example.com/data" }, "space-1")).toMatchObject({ reason: "http_fetch_url_invalid" });
    expect(guardPluginRuntimeAccess(enabled, { kind: "http_fetch", url: "https://evil.example/data" }, "space-1")).toMatchObject({ reason: "http_fetch_domain_not_allowed" });
    expect(guardPluginRuntimeAccess(enabled, { kind: "http_fetch", url: "https://api.example.com/data" }, "space-1")).toEqual({ allowed: false, reason: "runtime_unavailable", runtime_state: "blocked_runtime_unavailable" });
    expect(guardPluginRuntimeAccess(
      enabled,
      { kind: "client", permission: "editor_commands" },
      "space-1",
      { runtimeAvailable: true },
    )).toEqual({ allowed: true, runtime_state: "blocked_runtime_unavailable" });
  });

  it("reads normalized records from browser storage", () => {
    const records = upsertPluginInstallRecord([], makeManifest());
    writePluginInstallRecords([...records, { broken: true } as never]);

    expect(readPluginInstallRecords()).toEqual(records);
  });

  it("stores install-source details without enabling runtime execution", () => {
    const records = upsertPluginInstallRecord([], makeManifest(), {
      type: "registry-index",
      label: "Reviewed registry entry",
      manifestUrl: "https://plugins.example.com/example/plugin.json",
      sourceUrl: "https://github.com/example/plugin",
      verified: true,
      capturedAt: "2026-07-07T00:00:00.000Z",
    });

    expect(records[0]).toMatchObject({
      enabled: false,
      source: "pasted-manifest",
      sourceDetails: {
        type: "registry-index",
        label: "Reviewed registry entry",
        manifestUrl: "https://plugins.example.com/example/plugin.json",
        sourceUrl: "https://github.com/example/plugin",
        verified: true,
        capturedAt: "2026-07-07T00:00:00.000Z",
      },
    });
    expect(formatPluginInstallSourceDetails(records[0].sourceDetails)).toBe(
      "Reviewed registry entry / registry verified",
    );
    expect(getPluginRuntimeStatus(records[0]).runtime_state).toBe("disabled");
  });

  it("derives install-source details from matching registry entries", () => {
    const manifest = makeManifest();
    const details = getPluginInstallSourceDetailsForManifest(manifest, [
      {
        id: "com.example.plugin",
        name: "Example Plugin",
        version: "1.0.0",
        description: "Registry entry",
        manifest_url: "https://plugins.example.com/example/plugin.json",
        source_url: "https://github.com/example/plugin",
        categories: ["editor"],
        verified: true,
      },
    ]);

    expect(details).toMatchObject({
      type: "registry-index",
      label: "Matched registry entry",
      manifestUrl: "https://plugins.example.com/example/plugin.json",
      sourceUrl: "https://github.com/example/plugin",
      verified: true,
    });
  });

  it("falls back to pasted-manifest source details when no registry entry matches", () => {
    const details = getPluginInstallSourceDetailsForManifest(
      makeManifest({ homepage: "https://example.com/plugin" }),
      [
        {
          id: "com.example.other",
          name: "Other Plugin",
          version: "1.0.0",
          description: "Registry entry",
          manifest_url: "https://plugins.example.com/other/plugin.json",
          categories: ["editor"],
          verified: false,
        },
      ],
    );

    expect(details).toMatchObject({
      type: "pasted-manifest",
      label: "Pasted plugin.json",
      homepage: "https://example.com/plugin",
    });
  });

  it("backfills install-source details for legacy saved records", () => {
    const [record] = upsertPluginInstallRecord([], makeManifest({ homepage: "https://example.com" }));
    const legacy = { ...record };
    delete (legacy as Partial<typeof record>).sourceDetails;
    writePluginInstallRecords([legacy]);

    expect(readPluginInstallRecords()[0]).toMatchObject({
      sourceDetails: {
        type: "pasted-manifest",
        label: "Pasted plugin.json",
        homepage: "https://example.com",
      },
    });
  });

  it("records capped plugin install audit events", () => {
    const initial = appendPluginInstallAuditEvent([], {
      pluginId: "com.example.plugin",
      pluginName: "Example Plugin",
      action: "install_record_saved",
    });
    const scoped = appendPluginInstallAuditEvent(initial, {
      pluginId: "com.example.plugin",
      pluginName: "Example Plugin",
      action: "space_enabled",
      spaceId: "space-1",
      spaceName: "Operations",
    });

    writePluginInstallAuditEvents([...scoped, { broken: true } as never]);

    expect(readPluginInstallAuditEvents()).toHaveLength(2);
    expect(readPluginInstallAuditEvents()[0]).toMatchObject({
      pluginId: "com.example.plugin",
      action: "space_enabled",
      spaceName: "Operations",
    });
    expect(formatPluginInstallAuditAction("install_record_removed")).toBe(
      "Removed install record",
    );
  });

  it("records capped plugin runtime audit events without enabling execution", () => {
    const initial = appendPluginRuntimeAuditEvent([], {
      pluginId: "com.example.plugin",
      pluginName: "Example Plugin",
      action: "runtime_enable_blocked",
      runtimeState: "blocked_runtime_unavailable",
      reason: "Plugin runtime execution is not available yet.",
    });
    const scoped = appendPluginRuntimeAuditEvent(initial, {
      pluginId: "com.example.plugin",
      pluginName: "Example Plugin",
      action: "runtime_space_enable_blocked",
      runtimeState: "blocked_runtime_unavailable",
      reason: "Plugin runtime execution is not available yet.",
      spaceId: "space-1",
      spaceName: "Operations",
    });

    writePluginRuntimeAuditEvents([...scoped, { broken: true } as never]);

    expect(readPluginRuntimeAuditEvents()).toHaveLength(2);
    expect(readPluginRuntimeAuditEvents()[0]).toMatchObject({
      pluginId: "com.example.plugin",
      action: "runtime_space_enable_blocked",
      spaceName: "Operations",
      runtimeState: "blocked_runtime_unavailable",
    });
    expect(formatPluginRuntimeAuditAction("runtime_enable_blocked")).toBe(
      "Runtime enable blocked",
    );
  });

  it("filters and serializes plugin runtime audit events for export", () => {
    const events = [
      {
        id: "event-1",
        pluginId: "com.example.plugin",
        pluginName: "Example Plugin",
        action: "runtime_enable_blocked" as const,
        runtimeState: "blocked_runtime_unavailable" as const,
        reason: "Plugin runtime execution is not available yet.",
        createdAt: "2026-07-07T00:00:00.000Z",
      },
      {
        id: "event-2",
        pluginId: "com.example.plugin",
        pluginName: "Example Plugin",
        action: "runtime_space_enable_blocked" as const,
        runtimeState: "blocked_runtime_unavailable" as const,
        reason: "Blocked, still no runtime.",
        spaceId: "space-1",
        spaceName: "Operations, East",
        createdAt: "2026-07-07T00:01:00.000Z",
      },
    ];

    const filtered = filterPluginRuntimeAuditEvents(events, {
      action: "runtime_space_enable_blocked",
      spaceId: "space-1",
    });

    expect(filtered).toHaveLength(1);
    expect(JSON.parse(serializePluginRuntimeAuditEvents(filtered))).toMatchObject({
      schema_version: 1,
      event_count: 1,
      events: [{ id: "event-2", spaceName: "Operations, East" }],
    });
    expect(serializePluginRuntimeAuditEvents(filtered, "csv")).toContain(
      '"Operations, East"',
    );
  });

  it("escapes plugin runtime audit CSV export values with quotes and newlines", () => {
    const csv = serializePluginRuntimeAuditEvents(
      [
        {
          id: "event-1",
          pluginId: "com.example.plugin",
          pluginName: 'Example "Plugin"',
          action: "runtime_enable_blocked",
          runtimeState: "blocked_runtime_unavailable",
          reason: 'Runtime blocked until sandbox is ready.\nOperator said "hold".',
          createdAt: "2026-07-07T00:00:00.000Z",
        },
      ],
      "csv",
    );

    expect(csv).toContain('"Example ""Plugin"""');
    expect(csv).toContain('"Runtime blocked until sandbox is ready.\nOperator said ""hold""."');
  });
});
