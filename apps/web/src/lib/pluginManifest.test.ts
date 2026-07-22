import { describe, expect, it } from "vitest";
import {
  summarizePluginPermissionReview,
  validatePluginManifest,
  validatePluginRegistryIndex,
} from "./pluginManifest";

function makeManifest(overrides: Record<string, unknown> = {}) {
  return {
    id: "com.example.my-plugin",
    name: "My Plugin",
    version: "1.0.0",
    description: "Does something useful",
    author: "Jane Smith",
    license: "MIT",
    homepage: "https://github.com/example/my-plugin",
    starlane_version: ">=0.1.0",
    permissions: {
      server: ["read_docs", "http_fetch"],
      client: ["editor_commands", "sidebar_panel"],
    },
    http_fetch: ["api.example.com"],
    settings_schema: [
      {
        key: "api_key",
        label: "API Key",
        type: "secret",
        required: true,
      },
    ],
    ...overrides,
  };
}

function makeRegistryIndex(overrides: Record<string, unknown> = {}) {
  return {
    schema_version: 1,
    generated_at: "2026-07-07T00:00:00.000Z",
    plugins: [
      {
        id: "com.example.my-plugin",
        name: "My Plugin",
        version: "1.0.0",
        description: "Does something useful",
        manifest_url: "https://plugins.example.com/my-plugin/plugin.json",
        source_url: "https://github.com/example/my-plugin",
        homepage: "https://example.com/my-plugin",
        author: "Jane Smith",
        license: "MIT",
        categories: ["editor", "Automation"],
        verified: true,
      },
    ],
    ...overrides,
  };
}

describe("plugin manifest validation", () => {
  it("normalizes a valid plugin manifest", () => {
    const result = validatePluginManifest(makeManifest());

    expect(result.errors).toEqual([]);
    expect(result.manifest).toMatchObject({
      schema_version: 1,
      id: "com.example.my-plugin",
      version: "1.0.0",
      permissions: {
        server: ["read_docs", "http_fetch"],
        client: ["editor_commands", "sidebar_panel"],
      },
      http_fetch: ["api.example.com"],
      settings_schema: [{ key: "api_key", type: "secret", required: true }],
    });
  });

  it("rejects unknown permissions", () => {
    const result = validatePluginManifest(
      makeManifest({
        permissions: {
          server: ["read_docs", "delete_everything"],
          client: ["editor_commands", "unsafe_dom"],
        },
      }),
    );

    expect(result.manifest).toBeNull();
    expect(result.errors).toContain("permissions.server includes unknown permission: delete_everything.");
    expect(result.errors).toContain("permissions.client includes unknown permission: unsafe_dom.");
  });

  it("requires domain allowlists for http_fetch permission", () => {
    const result = validatePluginManifest(
      makeManifest({
        http_fetch: [],
      }),
    );

    expect(result.manifest).toBeNull();
    expect(result.errors).toContain(
      "permissions.server includes http_fetch, so http_fetch must list allowed domains.",
    );
  });

  it("rejects URL and wildcard http_fetch entries", () => {
    const result = validatePluginManifest(
      makeManifest({
        http_fetch: ["https://api.example.com", "*.example.com"],
      }),
    );

    expect(result.manifest).toBeNull();
    expect(result.errors).toContain("http_fetch includes invalid domain: https://api.example.com.");
    expect(result.errors).toContain("http_fetch includes invalid domain: *.example.com.");
  });

  it("validates settings schema fields", () => {
    const result = validatePluginManifest(
      makeManifest({
        settings_schema: [
          { key: "mode", label: "Mode", type: "select" },
          { key: "mode", label: "", type: "mystery" },
        ],
      }),
    );

    expect(result.manifest).toBeNull();
    expect(result.errors).toContain("settings_schema[0].options is required for select settings.");
    expect(result.errors).toContain("settings_schema[1].key must be unique.");
    expect(result.errors).toContain("settings_schema[1].label is required.");
    expect(result.errors).toContain("settings_schema[1].type is invalid.");
  });

  it("warns when http_fetch domains are listed without permission", () => {
    const result = validatePluginManifest(
      makeManifest({
        permissions: {
          server: ["read_docs"],
          client: [],
        },
      }),
    );

    expect(result.errors).toEqual([]);
    expect(result.warnings).toContain(
      "http_fetch domains are ignored unless permissions.server includes http_fetch.",
    );
  });

  it("summarizes plugin permission review risk", () => {
    const result = validatePluginManifest(makeManifest());
    expect(result.manifest).not.toBeNull();

    const review = summarizePluginPermissionReview(result.manifest!);

    expect(review.level).toBe("high");
    expect(review.items.map((item) => item.id)).toEqual(
      expect.arrayContaining(["server_http_fetch", "http_fetch_domains", "secret_settings"]),
    );
  });

  it("summarizes no-permission manifests as low review", () => {
    const result = validatePluginManifest(
      makeManifest({
        permissions: {
          server: [],
          client: [],
        },
        http_fetch: [],
        settings_schema: [],
      }),
    );
    expect(result.manifest).not.toBeNull();

    const review = summarizePluginPermissionReview(result.manifest!);

    expect(review).toMatchObject({
      level: "low",
      items: [
        {
          id: "no_permissions",
          level: "low",
        },
      ],
    });
  });
});

describe("plugin registry index validation", () => {
  it("normalizes a valid registry index", () => {
    const result = validatePluginRegistryIndex(makeRegistryIndex());

    expect(result.errors).toEqual([]);
    expect(result.index).toMatchObject({
      schema_version: 1,
      generated_at: "2026-07-07T00:00:00.000Z",
      plugins: [
        {
          id: "com.example.my-plugin",
          name: "My Plugin",
          version: "1.0.0",
          manifest_url: "https://plugins.example.com/my-plugin/plugin.json",
          source_url: "https://github.com/example/my-plugin",
          categories: ["editor", "automation"],
          verified: true,
        },
      ],
    });
  });

  it("rejects duplicate plugin id/version entries", () => {
    const result = validatePluginRegistryIndex(
      makeRegistryIndex({
        plugins: [
          makeRegistryIndex().plugins[0],
          {
            ...makeRegistryIndex().plugins[0],
            name: "Duplicate",
          },
        ],
      }),
    );

    expect(result.index).toBeNull();
    expect(result.errors).toContain(
      "plugins[1] duplicates plugin registry entry com.example.my-plugin@1.0.0.",
    );
  });

  it("validates registry URLs, categories, and versions", () => {
    const result = validatePluginRegistryIndex(
      makeRegistryIndex({
        generated_at: "not-a-date",
        plugins: [
          {
            id: "Bad Plugin",
            name: "Bad Plugin",
            version: "latest",
            description: "Broken",
            manifest_url: "file:///tmp/plugin.json",
            source_url: 42,
            categories: ["Good", "bad category"],
            verified: "yes",
          },
        ],
      }),
    );

    expect(result.index).toBeNull();
    expect(result.errors).toEqual(
      expect.arrayContaining([
        "generated_at must be an ISO date string when provided.",
        "plugins[0].source_url must be a URL string.",
        "plugins[0].manifest_url must use http or https.",
        "plugins[0].id must use reverse-DNS style lowercase segments.",
        "plugins[0].version must be a semantic version such as 1.0.0.",
        "plugins[0].categories includes invalid category: bad category.",
        "plugins[0].verified must be a boolean when provided.",
      ]),
    );
  });

  it("warns when registry entries lack review links", () => {
    const result = validatePluginRegistryIndex(
      makeRegistryIndex({
        plugins: [
          {
            id: "com.example.local",
            name: "Local Plugin",
            version: "1.0.0",
            description: "No review URL",
            manifest_url: "https://plugins.example.com/local/plugin.json",
          },
        ],
      }),
    );

    expect(result.errors).toEqual([]);
    expect(result.warnings).toContain(
      "plugins[0] has no source_url or homepage for operator review.",
    );
  });
});
