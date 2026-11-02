import { describe, expect, it } from "vitest";
import { unzipSync } from "fflate";
import { execFileSync } from "node:child_process";
import { chmodSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import type { LocalVaultSnapshot } from "@/lib/localVault";
import {
  buildStaticSiteBundle,
  buildStaticSiteHtml,
  buildStaticSiteZipArchive,
  formatStaticSiteBundleSummary,
} from "./staticSiteExport";

function makeSnapshot(overrides: Partial<LocalVaultSnapshot> = {}): LocalVaultSnapshot {
  return {
    schema_version: 1,
    spaces: [
      {
        id: "space-1",
        name: "Field Notes",
        slug: "field-notes",
        created_at: "2026-07-01T00:00:00.000Z",
        updated_at: "2026-07-06T00:00:00.000Z",
      },
    ],
    folders: [{ id: "folder-1", space_id: "space-1", name: "Ops" }],
    documents: [
      {
        id: "doc-1",
        space_id: "space-1",
        folder_id: "folder-1",
        title: "Launch Plan",
        content: "# Go\n\n- checklist\n\n<script>alert('nope')</script>",
        created_at: "2026-07-02T00:00:00.000Z",
        updated_at: "2026-07-06T00:00:00.000Z",
      },
    ],
    assets: [],
    templates: [],
    workflow_templates: [],
    document_workflows: [],
    document_workflow_events: [],
    property_schemas: [
      {
        id: "prop-1",
        space_id: "space-1",
        name: "Status",
        property_type: "select",
        sort_order: 1,
      },
    ],
    property_values: [
      {
        id: "value-1",
        document_id: "doc-1",
        property_id: "prop-1",
        value: "Ready",
      },
    ],
    tags: [
      {
        id: "tag-1",
        space_id: "space-1",
        name: "Release",
        color: "#2563eb",
      },
    ],
    doc_tags: [{ document_id: "doc-1", tag_id: "tag-1" }],
    search_index: [],
    sync_state: {
      state: "idle",
      updated_at: "2026-07-06T00:00:00.000Z",
      queued_operations: [],
    },
    ...overrides,
  };
}

describe("static site export", () => {
  it("builds a self-contained HTML export from a vault snapshot", () => {
    const result = buildStaticSiteHtml(makeSnapshot(), {
      generatedAt: "2026-07-06T00:00:00.000Z",
    });

    expect(result.filename).toBe("field-notes.html");
    expect(result.document_count).toBe(1);
    expect(result.html).toContain("<h1>Field Notes</h1>");
    expect(result.html).toContain("<h2>Launch Plan</h2>");
    expect(result.html).toContain("<li>checklist</li>");
    expect(result.html).toContain("<dt>Status</dt><dd>Ready</dd>");
    expect(result.html).toContain("Release");
    expect(result.html).toContain("&lt;script&gt;alert");
    expect(result.html).toContain("&lt;/script&gt;");
    expect(result.html).not.toContain("<script>alert");
  });

  it("uses a stable generic title for multi-space exports", () => {
    const result = buildStaticSiteHtml(
      makeSnapshot({
        spaces: [
          ...makeSnapshot().spaces,
          {
            id: "space-2",
            name: "Operations",
            slug: "ops",
            created_at: "2026-07-01T00:00:00.000Z",
            updated_at: "2026-07-06T00:00:00.000Z",
          },
        ],
      }),
    );

    expect(result.filename).toBe("starlane-notes-export.html");
    expect(result.html).toContain("Starlane Notes Export");
  });

  it("applies export title, description, and theme options", () => {
    const result = buildStaticSiteHtml(makeSnapshot(), {
      title: "Launch Site",
      description: "Published field notes",
      generatedAt: "2026-07-06T00:00:00.000Z",
      theme: "dark",
    });

    expect(result.filename).toBe("launch-site.html");
    expect(result.html).toContain('<meta name="description" content="Published field notes" />');
    expect(result.html).toContain('<body class="theme-dark">');
    expect(result.html).toContain('<p class="site-description">Published field notes</p>');
    expect(result.html).toContain("--bg: #111827");
  });

  it("builds a multi-file static site bundle with document pages", () => {
    const result = buildStaticSiteBundle(makeSnapshot(), {
      title: "Launch Site",
      generatedAt: "2026-07-06T00:00:00.000Z",
    });

    expect(result.filename).toBe("launch-site.zip");
    expect(result.document_count).toBe(1);
    expect(result.asset_count).toBe(0);
    expect(result.skipped_asset_count).toBe(0);
    expect(result.files.map((file) => file.path)).toEqual([
      "styles.css",
      "index.html",
      "site.json",
      "documents/launch-plan-doc-1.html",
      "integrity.json",
      "deploy-local.sh",
      "deploy-rsync.sh",
      "deploy.json",
    ]);
    expect(result.files.find((file) => file.path === "index.html")?.content).toContain(
      'href="documents/launch-plan-doc-1.html"',
    );
    expect(result.files.find((file) => file.path.includes("launch-plan"))?.content).toContain(
      '<link rel="stylesheet" href="../styles.css" />',
    );
    expect(
      JSON.parse(result.files.find((file) => file.path === "site.json")?.content as string),
    ).toMatchObject({
      schema_version: 1,
      title: "Launch Site",
      generated_at: "2026-07-06T00:00:00.000Z",
      document_count: 1,
      asset_count: 0,
      documents: [
        {
          id: "doc-1",
          title: "Launch Plan",
          path: "documents/launch-plan-doc-1.html",
          space_name: "Field Notes",
          folder_name: "Ops",
        },
      ],
    });
    expect(
      JSON.parse(result.files.find((file) => file.path === "deploy.json")?.content as string),
    ).toMatchObject({
      schema_version: 1,
      title: "Launch Site",
      strategy: "static_zip_manual_upload",
      publish_root: ".",
      entrypoint: "index.html",
      manifest: "site.json",
      integrity_manifest: "integrity.json",
      deploy_hooks: [
        {
          path: "deploy-local.sh",
          kind: "local_directory_copy",
          command: "sh deploy-local.sh <publish-directory>",
        },
        {
          path: "deploy-rsync.sh",
          kind: "ssh_rsync",
          command:
            "STARLANE_STATIC_DEPLOY_TARGET=user@example.com:/var/www/site sh deploy-rsync.sh",
          required_environment: ["STARLANE_STATIC_DEPLOY_TARGET"],
        },
      ],
      document_count: 1,
      asset_count: 0,
      skipped_asset_count: 0,
      upload_files: [
        { path: "styles.css", media_type: "text/css;charset=utf-8" },
        { path: "index.html", media_type: "text/html;charset=utf-8" },
        { path: "site.json", media_type: "application/json;charset=utf-8" },
        { path: "documents/launch-plan-doc-1.html", media_type: "text/html;charset=utf-8" },
        { path: "integrity.json", media_type: "application/json;charset=utf-8" },
      ],
    });
    expect(
      JSON.parse(result.files.find((file) => file.path === "deploy.json")?.content as string)
        .upload_files,
    ).not.toContainEqual(expect.objectContaining({ path: "deploy-local.sh" }));
    expect(
      JSON.parse(result.files.find((file) => file.path === "deploy.json")?.content as string)
        .upload_files,
    ).not.toContainEqual(expect.objectContaining({ path: "deploy-rsync.sh" }));
    expect(result.files.find((file) => file.path === "deploy-local.sh")?.content).toContain(
      "Usage: sh deploy-local.sh <publish-directory>",
    );
    expect(result.files.find((file) => file.path === "deploy-local.sh")?.content).toContain(
      '"documents/launch-plan-doc-1.html"',
    );
    expect(result.files.find((file) => file.path === "deploy-local.sh")?.content).toContain(
      '"integrity.json"',
    );
    expect(result.files.find((file) => file.path === "deploy-rsync.sh")?.content).toContain(
      "STARLANE_STATIC_DEPLOY_TARGET",
    );
    expect(
      JSON.parse(result.files.find((file) => file.path === "integrity.json")?.content as string),
    ).toMatchObject({
      schema_version: 1,
      checksum_algorithm: "crc32",
      file_count: 4,
      files: expect.arrayContaining([
        expect.objectContaining({
          path: "index.html",
          media_type: "text/html;charset=utf-8",
          checksum_crc32: expect.stringMatching(/^[0-9a-f]{8}$/),
        }),
        expect.objectContaining({
          path: "documents/launch-plan-doc-1.html",
          media_type: "text/html;charset=utf-8",
          checksum_crc32: expect.stringMatching(/^[0-9a-f]{8}$/),
        }),
      ]),
    });
  });

  it("keeps unsafe document titles inside sanitized static document paths", () => {
    const result = buildStaticSiteBundle(
      makeSnapshot({
        documents: [
          {
            ...makeSnapshot().documents[0],
            id: "doc-1",
            title: "../Secrets/Launch Plan",
          },
          {
            ...makeSnapshot().documents[0],
            id: "doc-2",
            title: "../Secrets/Launch Plan",
          },
        ],
      }),
    );

    const documentPaths = result.files
      .map((file) => file.path)
      .filter((path) => path.startsWith("documents/"))
      .sort();

    expect(documentPaths).toEqual([
      "documents/secrets-launch-plan-doc-1.html",
      "documents/secrets-launch-plan-doc-2.html",
    ]);
    expect(documentPaths.every((path) => !path.includes(".."))).toBe(true);
    expect(documentPaths.every((path) => !path.slice("documents/".length).includes("/"))).toBe(true);
  });

  it("extracts embedded data-url assets into static site bundles", () => {
    const imageUrl = "data:image/png;base64,aGVsbG8=";
    const snapshot = makeSnapshot({
      documents: [
        {
          ...makeSnapshot().documents[0],
          content: `# Diagram\n\n![Diagram](${imageUrl})`,
        },
      ],
      assets: [
        {
          id: "asset-1",
          name: "Diagram.PNG",
          mime_type: "image/png",
          size: 5,
          url: imageUrl,
          source: "browser",
          created_at: "2026-07-06T00:00:00.000Z",
        },
      ],
    });

    const result = buildStaticSiteBundle(snapshot);
    const assetFile = result.files.find((file) => file.path === "assets/diagram.png");
    const documentFile = result.files.find(
      (file) => file.path === "documents/launch-plan-doc-1.html",
    );

    expect(result.asset_count).toBe(1);
    expect(
      JSON.parse(result.files.find((file) => file.path === "site.json")?.content as string),
    ).toMatchObject({
      asset_count: 1,
      assets: [
        {
          id: "asset-1",
          name: "Diagram.PNG",
          path: "assets/diagram.png",
          media_type: "image/png",
          size: 5,
        },
      ],
    });
    expect(assetFile?.media_type).toBe("image/png");
    expect(assetFile?.content).toBeInstanceOf(Uint8Array);
    expect(Array.from(assetFile?.content as Uint8Array)).toEqual([104, 101, 108, 108, 111]);
    expect(documentFile?.content).toContain("../assets/diagram.png");
    expect(documentFile?.content).not.toContain(imageUrl);
  });

  it("keeps duplicate unsafe asset names in unique static bundle paths", () => {
    const firstImageUrl = "data:image/png;base64,aGVsbG8=";
    const secondImageUrl = "data:image/png;base64,d29ybGQ=";
    const snapshot = makeSnapshot({
      documents: [
        {
          ...makeSnapshot().documents[0],
          content:
            "# Diagrams\n\n![First](starlane://asset/asset-1)\n\n![Second](starlane://asset/asset-2)",
        },
      ],
      assets: [
        {
          id: "asset-1",
          name: "Diagram.PNG",
          mime_type: "image/png",
          size: 5,
          url: firstImageUrl,
          source: "browser",
          created_at: "2026-07-06T00:00:00.000Z",
        },
        {
          id: "asset-2",
          name: "../Diagram.PNG",
          mime_type: "image/png",
          size: 5,
          url: secondImageUrl,
          source: "browser",
          created_at: "2026-07-06T00:00:00.000Z",
        },
      ],
    });

    const result = buildStaticSiteBundle(snapshot);
    const siteManifest = JSON.parse(
      result.files.find((file) => file.path === "site.json")?.content as string,
    );
    const deployPlan = JSON.parse(
      result.files.find((file) => file.path === "deploy.json")?.content as string,
    );
    const documentFile = result.files.find(
      (file) => file.path === "documents/launch-plan-doc-1.html",
    );

    expect(result.asset_count).toBe(2);
    expect(result.files.map((file) => file.path)).toEqual(
      expect.arrayContaining(["assets/diagram.png", "assets/diagram-2.png"]),
    );
    expect(siteManifest.assets).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: "asset-1", path: "assets/diagram.png" }),
        expect.objectContaining({ id: "asset-2", path: "assets/diagram-2.png" }),
      ]),
    );
    expect(deployPlan.upload_files).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ path: "assets/diagram.png" }),
        expect.objectContaining({ path: "assets/diagram-2.png" }),
      ]),
    );
    expect(documentFile?.content).toContain("../assets/diagram.png");
    expect(documentFile?.content).toContain("../assets/diagram-2.png");
    expect(documentFile?.content).not.toContain("starlane://asset/asset-1");
    expect(documentFile?.content).not.toContain("starlane://asset/asset-2");
  });

  it("rewrites non-embedded document attachment references when asset bytes are available", () => {
    const imageUrl = "data:image/png;base64,aGVsbG8=";
    const vaultPath = "/Users/example/Starlane/attachments/doc-1/asset-1-diagram.png";
    const starlaneUrl = "starlane://asset/asset-1";
    const snapshot = makeSnapshot({
      documents: [
        {
          ...makeSnapshot().documents[0],
          content: `# Diagram\n\n![By path](${vaultPath})\n\n![By id](${starlaneUrl})`,
        },
      ],
      assets: [
        {
          id: "asset-1",
          name: "diagram.png",
          mime_type: "image/png",
          size: 5,
          url: imageUrl,
          vault_path: vaultPath,
          source: "tauri",
          created_at: "2026-07-06T00:00:00.000Z",
        },
      ],
    });

    const result = buildStaticSiteBundle(snapshot);
    const documentFile = result.files.find(
      (file) => file.path === "documents/launch-plan-doc-1.html",
    );

    expect(result.asset_count).toBe(1);
    expect(documentFile?.content).toContain("../assets/diagram.png");
    expect(documentFile?.content).not.toContain(vaultPath);
    expect(documentFile?.content).not.toContain(starlaneUrl);
  });

  it("records skipped static-site assets for non-embedded and invalid data URLs", () => {
    const remoteUrl = "https://cdn.example.test/diagram.png";
    const invalidDataUrl = "data:image/png,%E0%A4%A";
    const snapshot = makeSnapshot({
      documents: [
        {
          ...makeSnapshot().documents[0],
          content: `# Diagram\n\n![Remote](${remoteUrl})\n\n![Broken](${invalidDataUrl})`,
        },
      ],
      assets: [
        {
          id: "asset-remote",
          name: "Remote Diagram.png",
          mime_type: "image/png",
          size: 2048,
          url: remoteUrl,
          source: "browser",
          created_at: "2026-07-06T00:00:00.000Z",
        },
        {
          id: "asset-invalid",
          name: "Broken Diagram.png",
          mime_type: "image/png",
          size: 2048,
          url: invalidDataUrl,
          source: "browser",
          created_at: "2026-07-06T00:00:00.000Z",
        },
      ],
    });

    const result = buildStaticSiteBundle(snapshot);
    const siteManifest = JSON.parse(
      result.files.find((file) => file.path === "site.json")?.content as string,
    );
    const deployPlan = JSON.parse(
      result.files.find((file) => file.path === "deploy.json")?.content as string,
    );
    const documentFile = result.files.find(
      (file) => file.path === "documents/launch-plan-doc-1.html",
    );

    expect(result.asset_count).toBe(0);
    expect(result.skipped_asset_count).toBe(2);
    expect(siteManifest).toMatchObject({
      asset_count: 0,
      skipped_asset_count: 2,
      documents: [
        {
          id: "doc-1",
          skipped_assets: [
            {
              id: "asset-remote",
              reason: "remote_url_not_embedded",
              source_url: remoteUrl,
            },
            {
              id: "asset-invalid",
              reason: "invalid_data_url",
              source_url: invalidDataUrl,
            },
          ],
        },
      ],
      skipped_assets: [
        {
          id: "asset-remote",
          name: "Remote Diagram.png",
          reason: "remote_url_not_embedded",
          source_url: remoteUrl,
        },
        {
          id: "asset-invalid",
          name: "Broken Diagram.png",
          reason: "invalid_data_url",
          source_url: invalidDataUrl,
        },
      ],
    });
    expect(deployPlan).toMatchObject({
      asset_count: 0,
      skipped_asset_count: 2,
      skipped_assets: siteManifest.skipped_assets,
    });
    expect(formatStaticSiteBundleSummary(result)).toContain(
      "2 skipped assets listed in site.json/deploy.json",
    );
    expect(documentFile?.content).toContain(remoteUrl);
  });

  it("records skipped static-site assets against documents that reference local attachment paths", () => {
    const assetUrl = "asset://localhost/%2FUsers%2Feagle%2FStarlane%2Fattachments%2Fdoc-1%2Fasset-1-diagram.png";
    const vaultPath = "/Users/example/Starlane/attachments/doc-1/asset-1-diagram.png";
    const snapshot = makeSnapshot({
      documents: [
        {
          ...makeSnapshot().documents[0],
          content: `# Diagram\n\n![Diagram](${vaultPath})`,
        },
      ],
      assets: [
        {
          id: "asset-1",
          name: "diagram.png",
          mime_type: "image/png",
          size: 2048,
          url: assetUrl,
          vault_path: vaultPath,
          source: "tauri",
          created_at: "2026-07-06T00:00:00.000Z",
        },
      ],
    });

    const result = buildStaticSiteBundle(snapshot);
    const siteManifest = JSON.parse(
      result.files.find((file) => file.path === "site.json")?.content as string,
    );

    expect(result.asset_count).toBe(0);
    expect(result.skipped_asset_count).toBe(1);
    expect(siteManifest.documents[0].skipped_assets).toMatchObject([
      {
        id: "asset-1",
        name: "diagram.png",
        source_url: assetUrl,
        source_references: expect.arrayContaining([vaultPath, "starlane://asset/asset-1"]),
        reason: "remote_url_not_embedded",
      },
    ]);
  });

  it("writes a portable stored ZIP archive for static site bundles", () => {
    const archive = buildStaticSiteZipArchive(buildStaticSiteBundle(makeSnapshot()));
    const text = new TextDecoder().decode(archive);

    expect(archive[0]).toBe(0x50);
    expect(archive[1]).toBe(0x4b);
    expect(text).toContain("index.html");
    expect(text).toContain("styles.css");
    expect(text).toContain("site.json");
    expect(text).toContain("integrity.json");
    expect(text).toContain("deploy-local.sh");
    expect(text).toContain("deploy-rsync.sh");
    expect(text).toContain("deploy.json");
    expect(text).toContain("documents/launch-plan-doc-1.html");
  });

  it("generates a local deploy helper that copies publishable files", () => {
    const bundle = buildStaticSiteBundle(makeSnapshot());
    const root = mkdtempSync(join(tmpdir(), "starlane-static-site-"));
    const sourceRoot = join(root, "source");
    const targetRoot = join(root, "published");
    mkdirSync(sourceRoot);

    try {
      for (const file of bundle.files) {
        const filePath = join(sourceRoot, file.path);
        mkdirSync(dirname(filePath), { recursive: true });
        writeFileSync(filePath, file.content);
      }

      execFileSync("sh", ["deploy-local.sh", targetRoot], { cwd: sourceRoot });

      expect(readFileSync(join(targetRoot, "index.html"), "utf8")).toContain("Field Notes");
      expect(readFileSync(join(targetRoot, "documents/launch-plan-doc-1.html"), "utf8")).toContain(
        "Launch Plan",
      );
      expect(readFileSync(join(targetRoot, "integrity.json"), "utf8")).toContain(
        "checksum_algorithm",
      );
      expect(() => readFileSync(join(targetRoot, "deploy-local.sh"), "utf8")).toThrow();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("generates an SSH rsync hook that keeps destination and credentials outside the bundle", () => {
    const bundle = buildStaticSiteBundle(makeSnapshot());
    const root = mkdtempSync(join(tmpdir(), "starlane-static-site-rsync-"));
    const sourceRoot = join(root, "source");
    const binRoot = join(root, "bin");
    const argsPath = join(root, "rsync-args.txt");
    mkdirSync(sourceRoot);
    mkdirSync(binRoot);

    try {
      for (const file of bundle.files) {
        const filePath = join(sourceRoot, file.path);
        mkdirSync(dirname(filePath), { recursive: true });
        writeFileSync(filePath, file.content);
      }
      const rsyncStub = join(binRoot, "rsync");
      writeFileSync(rsyncStub, '#!/bin/sh\nprintf "%s\\n" "$@" > "$RSYNC_ARGS"\n');
      chmodSync(rsyncStub, 0o755);

      execFileSync("sh", ["deploy-rsync.sh"], {
        cwd: sourceRoot,
        env: {
          ...process.env,
          PATH: `${binRoot}:${process.env.PATH ?? ""}`,
          RSYNC_ARGS: argsPath,
          STARLANE_STATIC_DEPLOY_TARGET: "publisher@example.test:/var/www/field-notes",
        },
      });

      const args = readFileSync(argsPath, "utf8");
      expect(args).toContain("--archive");
      expect(args).toContain("./documents/launch-plan-doc-1.html");
      expect(args).toContain("publisher@example.test:/var/www/field-notes/");
      expect(args).not.toContain("deploy-local.sh");
      expect(args).not.toContain("deploy-rsync.sh");
      expect(readFileSync(join(sourceRoot, "deploy-rsync.sh"), "utf8")).not.toContain(
        "publisher@example.test:/var/www/field-notes",
      );
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("rejects unsafe rsync deployment destinations before invoking rsync", () => {
    const bundle = buildStaticSiteBundle(makeSnapshot());
    const root = mkdtempSync(join(tmpdir(), "starlane-static-site-rsync-invalid-"));
    const sourceRoot = join(root, "source");
    mkdirSync(sourceRoot);

    try {
      for (const file of bundle.files) {
        const filePath = join(sourceRoot, file.path);
        mkdirSync(dirname(filePath), { recursive: true });
        writeFileSync(filePath, file.content);
      }

      expect(() =>
        execFileSync("sh", ["deploy-rsync.sh"], {
          cwd: sourceRoot,
          env: {
            ...process.env,
            STARLANE_STATIC_DEPLOY_TARGET: "publisher@example.test:relative-path",
          },
          stdio: "pipe",
        }),
      ).toThrow();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});


describe("exact immutable asset reference export", () => {
  const asset = (id: string, name: string, body: string) => ({
    id,
    name,
    mime_type: "application/octet-stream",
    size: body.length,
    url: `data:application/octet-stream;base64,${Buffer.from(body).toString("base64")}`,
    source: "browser" as const,
    created_at: "2026-10-10T00:00:00Z",
  });
  const snapshot = (content: string, assets: LocalVaultSnapshot["assets"]) =>
    makeSnapshot({ documents: [{ ...makeSnapshot().documents[0], content }], assets });
  const html = (input: LocalVaultSnapshot) =>
    buildStaticSiteBundle(input).files.find((f) => f.path.startsWith("documents/"))!
      .content as string;
  it("keeps both equal-name different-byte explicit pins and leaves plain filename text alone", () => {
    const input = snapshot(
      "[Old](starlane://asset/version-1)\n\n[New](starlane://asset/version-2)\n\nPlain starlane://asset/same.bin",
      [asset("version-1", "same.bin", "OLD"), asset("version-2", "same.bin", "NEW")],
    );
    const before = JSON.stringify(input);
    const bundle = buildStaticSiteBundle(input);
    const page = html(input);
    expect(page).toContain('href="../assets/same.bin"');
    expect(page).toContain('href="../assets/same-2.bin"');
    expect(page).toContain("Plain starlane://asset/same.bin");
    expect(
      Array.from(bundle.files.find((f) => f.path === "assets/same.bin")!.content as Uint8Array),
    ).toEqual([...Buffer.from("OLD")]);
    expect(
      Array.from(bundle.files.find((f) => f.path === "assets/same-2.bin")!.content as Uint8Array),
    ).toEqual([...Buffer.from("NEW")]);
    expect(JSON.stringify(input)).toBe(before);
  });
  it("refuses a referenced ambiguous filename instead of selecting the last version", () => {
    const input = snapshot("[Ambiguous](starlane://asset/same.bin)", [
      asset("version-1", "same.bin", "OLD"),
      asset("version-2", "same.bin", "NEW"),
    ]);
    const before = JSON.stringify(input);
    expect(() => buildStaticSiteBundle(input)).toThrow(/ambiguous/i);
    expect(JSON.stringify(input)).toBe(before);
  });
  it("does not corrupt a longer immutable ID with a shorter ID prefix", () => {
    const input = snapshot(
      "[One](starlane://asset/version-1)\n\n[Ten](starlane://asset/version-10)",
      [asset("version-1", "one.bin", "ONE"), asset("version-10", "ten.bin", "TEN")],
    );
    const page = html(input);
    expect(page).toContain('href="../assets/one.bin"');
    expect(page).toContain('href="../assets/ten.bin"');
    expect(page).not.toContain("one.bin0");
  });
  it("rewrites accepted reference definitions without changing code, prose, titles or rejected duplicate definitions", () => {
    const pin = "starlane://asset/version-1";
    const body = `Plain ${pin}\r\n\r\n\`${pin}\`\r\n\r\n[Read][owned]\r\n\r\n[owned]: <${pin}> "${pin}"\r\n[owned]: ${pin}\r\n`;
    const page = html(snapshot(body, [asset("version-1", "one.bin", "ONE")]));
    expect(page).toContain('href="../assets/one.bin"');
    expect(page).toContain(`title="${pin}"`);
    expect(page).toContain(`<code>${pin}</code>`);
    expect(page).toContain(`Plain ${pin}`);
  });
  it("gives an exact immutable ID priority over another asset's filename alias", () => {
    const page = html(
      snapshot("[Exact](starlane://asset/version-1)", [
        asset("version-1", "one.bin", "ONE"),
        asset("version-2", "version-1", "TWO"),
      ]),
    );
    expect(page).toContain('href="../assets/one.bin"');
  });
});

describe("skipped asset exact alias ownership", () => {
  const local = {
    id: "local",
    name: "remote",
    mime_type: "application/octet-stream",
    size: 3,
    url: "data:application/octet-stream;base64,T05F",
    source: "browser" as const,
    created_at: "2026-10-10T00:00:00Z",
  };
  const remote = {
    id: "remote",
    name: "remote.bin",
    mime_type: "application/octet-stream",
    size: 3,
    url: "https://example.invalid/remote.bin",
    source: "browser" as const,
    created_at: "2026-10-10T00:00:00Z",
  };
  const input = (body: string, assets: LocalVaultSnapshot["assets"]) =>
    makeSnapshot({ documents: [{ ...makeSnapshot().documents[0], content: body }], assets });
  it("does not let a local filename impersonate a skipped remote exact ID", () => {
    const bundle = buildStaticSiteBundle(
      input("[Remote](starlane://asset/remote)", [local, remote]),
    );
    const html = bundle.files.find((f) => f.path.startsWith("documents/"))!.content as string;
    expect(html).toContain('href="starlane://asset/remote"');
    expect(html).not.toContain('href="../assets/remote"');
    expect(bundle.skipped_asset_count).toBe(1);
  });
  it("refuses a filename alias claimed by both exported and skipped versions", () => {
    expect(() =>
      buildStaticSiteBundle(
        input("[Ambiguous](starlane://asset/same.bin)", [
          { ...local, name: "same.bin" },
          { ...remote, name: "same.bin" },
        ]),
      ),
    ).toThrow(/ambiguous/);
  });
  it("refuses duplicate exact IDs even if only one copy can be embedded", () => {
    expect(() =>
      buildStaticSiteBundle(
        input("[Ambiguous](starlane://asset/local)", [local, { ...remote, id: "local" }]),
      ),
    ).toThrow(/ambiguous/);
  });
});

it("exports the pinned retained version bytes without retargeting a same-name newest version", () => {
  const pinned="11111111-1111-4111-8111-111111111111", newest="22222222-2222-4222-8222-222222222222";
  const snapshot=makeSnapshot({documents:[{id:"doc-moved-twice",space_id:"space-1",title:"Retained pin",content:`[file:Same name](starlane-attachment://${pinned})`,created_at:"2026-10-11T00:00:00Z",updated_at:"2026-10-11T00:00:00Z"}],assets:[
    {id:pinned,source:"browser",space_id:"space-1",name:"same.bin",url:"data:application/octet-stream;base64,AAH/",mime_type:"application/octet-stream",size:3,created_at:"2026-10-10T00:00:00Z"},
    {id:newest,source:"browser",space_id:"space-1",name:"same.bin",url:"data:application/octet-stream;base64,TkVX",mime_type:"application/octet-stream",size:3,created_at:"2026-10-11T00:00:00Z"}
  ]});
  const bundle=buildStaticSiteBundle(snapshot);
  const html=bundle.files.find(f=>f.path.startsWith("documents/")&&f.path.endsWith(".html"))!.content as string;
  expect(html).not.toContain(`starlane-attachment://${pinned}`);
  expect(html).toContain("../assets/same.bin");
  const original=bundle.files.find(f=>f.path=="assets/same.bin")!.content;
  expect(Array.from(original as Uint8Array)).toEqual([0,1,255]);
  const archive=unzipSync(buildStaticSiteZipArchive(bundle));
  expect(Array.from(archive["assets/same.bin"])).toEqual([0,1,255]);
  expect(Array.from(archive["assets/same-2.bin"])).toEqual([78,69,87]);
  expect(Array.from(bundle.files.find(f=>f.path=="assets/same-2.bin")!.content as Uint8Array)).toEqual([78,69,87]);
});
