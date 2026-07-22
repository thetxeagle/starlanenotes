import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { __resetLocalDataForTests, dataClient } from "./dataClient";
import { setCurrentLaunchMode } from "./launchMode";
import {
  buildProviderRemoteMutationEnvelope,
  buildProviderRemoteMutationPlan,
  buildProviderRemoteSyncManifest,
} from "./providerAdapters";
import { writeProviderSession } from "./providerSession";
import { writeSharedSpaceDownloadCheckpoints } from "./sharedSpaceDownloads";
import { setSyncProvider } from "./storageProviders";

describe("dataClient local mode", () => {
  const storage = new Map<string, string>();

  beforeEach(async () => {
    storage.clear();
    const localStorageMock = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
      removeItem: (key: string) => {
        storage.delete(key);
      },
    };
    (globalThis as { window?: unknown }).window = {
      localStorage: localStorageMock,
    };
    (globalThis as { localStorage?: unknown }).localStorage = localStorageMock;
    setCurrentLaunchMode("local");
    setSyncProvider("none");
    await __resetLocalDataForTests();
  });

  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
    delete (globalThis as { localStorage?: unknown }).localStorage;
    delete (globalThis as { fetch?: unknown }).fetch;
  });

  it("seeds a local space and welcome doc", async () => {
    const spaces = await dataClient.listSpaces();
    expect(spaces.length).toBe(1);
    expect(spaces[0].id).toBe("local-space");
    expect(spaces[0].name).toBe("Personal Space");
    expect(spaces[0]).toMatchObject({
      owner_username: "local-user",
      owner_display_name: "Local User",
      created_by: "local-user",
    });

    await expect(dataClient.getSpace("local-space")).resolves.toMatchObject({
      owner_username: "local-user",
      owner_display_name: "Local User",
      created_by: "local-user",
    });

    const docs = await dataClient.listDocuments("local-space");
    expect(docs.some((d) => d.id === "local-welcome")).toBe(true);
  });

  it("keeps TOTP and passkey server-account actions unavailable in local mode", async () => {
    await expect(dataClient.setupTotp()).rejects.toThrow(
      "Two-factor authentication is unavailable in local mode.",
    );
    await expect(dataClient.verifyTotp("123456")).rejects.toThrow(
      "Two-factor authentication is unavailable in local mode.",
    );
    await expect(dataClient.disableTotp("123456")).rejects.toThrow(
      "Two-factor authentication is unavailable in local mode.",
    );

    await expect(dataClient.listPasskeys()).resolves.toEqual([]);
    await expect(dataClient.startPasskeyRegistration("Phone")).rejects.toThrow(
      "Passkeys are unavailable in local mode.",
    );
    await expect(
      dataClient.finishPasskeyRegistration("challenge-1", {}, "Phone"),
    ).rejects.toThrow("Passkeys are unavailable in local mode.");
    await expect(dataClient.deletePasskey("passkey-1")).rejects.toThrow(
      "Passkeys are unavailable in local mode.",
    );
    await expect(dataClient.startPasskeyLogin("eagle@example.test")).rejects.toThrow(
      "Passkey sign-in is unavailable in local mode.",
    );
    await expect(dataClient.finishPasskeyLogin("challenge-1", {})).rejects.toThrow(
      "Passkey sign-in is unavailable in local mode.",
    );
  });

  it("saves and reads document-owned text assets by filename", async () => {
    const firstScene = JSON.stringify({ elements: [{ id: "shape-1" }], appState: {}, files: {} });
    const secondScene = JSON.stringify({ elements: [{ id: "shape-2" }], appState: {}, files: {} });

    await dataClient.saveDocumentTextAsset({
      name: "architecture.excalidraw",
      mime_type: "application/vnd.excalidraw+json",
      text: firstScene,
      space_id: "local-space",
      document_id: "local-welcome",
    });
    await dataClient.saveDocumentTextAsset({
      name: "architecture.excalidraw",
      mime_type: "application/vnd.excalidraw+json",
      text: secondScene,
      space_id: "local-space",
      document_id: "local-welcome",
    });

    await expect(
      dataClient.readDocumentTextAsset("architecture.excalidraw", {
        space_id: "local-space",
        document_id: "local-welcome",
      }),
    ).resolves.toBe(secondScene);
  });

  it("renames the local personal space", async () => {
    const updated = await dataClient.updateSpace("local-space", { name: "Field Notes" });
    expect(updated.name).toBe("Field Notes");

    const spaces = await dataClient.listSpaces();
    expect(spaces[0].name).toBe("Field Notes");
  });

  it("creates and updates local documents", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Draft",
      content: "hello",
    });

    await dataClient.updateDocument("local-space", created.id, { title: "Draft 2" });
    await dataClient.updateDocumentContent("local-space", created.id, "# Body");

    const doc = await dataClient.getDocument("local-space", created.id);
    const content = await dataClient.getDocumentContent("local-space", created.id);
    expect(doc.title).toBe("Draft 2");
    expect(content.content).toBe("# Body");
  });

  it("applies matching template tags and property values to local documents", async () => {
    const property = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "select",
    });
    const tag = await dataClient.createTag("local-space", { name: "Planning", color: "#2563eb" });
    const template = await dataClient.createTemplate("local-space", {
      name: "Project brief",
      content: "# Brief",
      tags: ["planning", "missing"],
      properties: { status: "Draft", ignored: null },
    });

    const document = await dataClient.createDocument("local-space", {
      title: "Launch plan",
      template_id: template.id,
      content: "# Brief",
    });

    await expect(dataClient.listDocumentTags("local-space", document.id)).resolves.toEqual([
      expect.objectContaining({ id: tag.id, name: "Planning" }),
    ]);
    const documentProperties = await dataClient.listDocumentProperties("local-space", document.id);
    expect(documentProperties).toEqual([
      expect.objectContaining({ property_id: property.id, value: "Draft" }),
    ]);
    await expect(
      dataClient.searchDocuments("", 10, { has_tag: "true", has_property: "true" }),
    ).resolves.toEqual(expect.arrayContaining([expect.objectContaining({ document_id: document.id })]));
    const queued = await dataClient.listQueuedSyncOperations();
    const documentCreate = queued.find(
      (operation) => operation.entity_type === "document" && operation.entity_id === document.id,
    );
    const tagCreate = queued.find(
      (operation) => operation.entity_type === "document_tag" && operation.entity_id === `${document.id}:${tag.id}`,
    );
    const propertyCreate = queued.find(
      (operation) =>
        operation.entity_type === "document_property_values" && operation.entity_id === document.id,
    );
    expect(tagCreate?.payload).toEqual({ document_id: document.id, tag_id: tag.id, space_id: "local-space" });
    expect(propertyCreate?.payload).toEqual(
      expect.objectContaining({
        document_id: document.id,
        space_id: "local-space",
        values: [expect.objectContaining({ property_id: property.id, value: "Draft" })],
      }),
    );
    expect((documentCreate?.queued_at ?? "") < (tagCreate?.queued_at ?? "")).toBe(true);
    expect((tagCreate?.queued_at ?? "") < (propertyCreate?.queued_at ?? "")).toBe(true);
  });

  it("renames and deletes local folders with their documents", async () => {
    const folder = await dataClient.createFolder("local-space", "Projects");
    await dataClient.updateFolder("local-space", folder.id, { name: "Archive" });

    let folders = await dataClient.listFolders("local-space");
    expect(folders.find((item) => item.id === folder.id)?.name).toBe("Archive");

    const created = await dataClient.createDocument("local-space", {
      title: "Nested",
      folder_id: folder.id,
    });

    await dataClient.deleteFolder("local-space", folder.id);
    folders = await dataClient.listFolders("local-space");
    expect(folders.some((item) => item.id === folder.id)).toBe(false);
    await expect(dataClient.getDocument("local-space", created.id)).rejects.toThrow(
      "Document not found",
    );
  });

  it("limits local folder nesting to three levels", async () => {
    const root = await dataClient.createFolder("local-space", "Root");
    const child = await dataClient.createFolder("local-space", "Child", {
      parent_id: root.id,
    });
    const grandchild = await dataClient.createFolder("local-space", "Grandchild", {
      parent_id: child.id,
    });

    const created = await dataClient.createDocument("local-space", {
      title: "Level Three Page",
      folder_id: grandchild.id,
    });
    await expect(
      dataClient.listDocuments("local-space", { folder_id: grandchild.id }),
    ).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: created.id, folder_id: grandchild.id }),
      ]),
    );
    await expect(
      dataClient.createFolder("local-space", "Too Deep", { parent_id: grandchild.id }),
    ).rejects.toThrow("Folders can be nested up to 3 levels");

    await dataClient.updateFolder("local-space", child.id, { parent_id: null });
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: child.id, parent_id: undefined })]),
    );
  });

  it("blocks local folder moves into descendants", async () => {
    const root = await dataClient.createFolder("local-space", "Root");
    const child = await dataClient.createFolder("local-space", "Child", {
      parent_id: root.id,
    });

    await expect(
      dataClient.updateFolder("local-space", root.id, { parent_id: child.id }),
    ).rejects.toThrow("Folder cannot be moved inside one of its descendants");
  });

  it("moves local folders and their documents across spaces", async () => {
    const targetSpace = await dataClient.createSpace("Archive");
    const targetParent = await dataClient.createFolder(targetSpace.id, "Imported");
    const sourceFolder = await dataClient.createFolder("local-space", "Projects");
    const childFolder = await dataClient.createFolder("local-space", "Launch", {
      parent_id: sourceFolder.id,
    });
    const document = await dataClient.createDocument("local-space", {
      title: "Plan",
      folder_id: childFolder.id,
    });

    await dataClient.moveFolder("local-space", sourceFolder.id, {
      target_space_id: targetSpace.id,
      parent_id: targetParent.id,
    });

    await expect(dataClient.listFolders("local-space")).resolves.not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: sourceFolder.id })]),
    );
    await expect(dataClient.listFolders(targetSpace.id)).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: sourceFolder.id, parent_id: targetParent.id }),
        expect.objectContaining({ id: childFolder.id, parent_id: sourceFolder.id }),
      ]),
    );
    await expect(dataClient.getDocument(targetSpace.id, document.id)).resolves.toMatchObject({
      id: document.id,
      folder_id: childFolder.id,
    });
  });

  it("moves local document trees across spaces and into folders", async () => {
    const targetSpace = await dataClient.createSpace("Research");
    const targetFolder = await dataClient.createFolder(targetSpace.id, "Notes");
    const parent = await dataClient.createDocument("local-space", {
      title: "Parent",
      content: "parent",
    });
    const child = await dataClient.createDocument("local-space", {
      title: "Child",
      content: "child",
      parent_id: parent.id,
    });

    await dataClient.moveDocument("local-space", parent.id, {
      target_space_id: targetSpace.id,
      folder_id: targetFolder.id,
      parent_id: null,
    });

    await expect(dataClient.getDocument("local-space", parent.id)).rejects.toThrow(
      "Document not found",
    );
    await expect(dataClient.getDocument(targetSpace.id, parent.id)).resolves.toMatchObject({
      id: parent.id,
      folder_id: targetFolder.id,
      parent_id: undefined,
    });
    await expect(dataClient.getDocument(targetSpace.id, child.id)).resolves.toMatchObject({
      id: child.id,
      folder_id: targetFolder.id,
      parent_id: parent.id,
    });
  });

  it("requires local document folders to exist in the current space", async () => {
    await expect(
      dataClient.createDocument("local-space", {
        title: "Missing Folder Page",
        folder_id: "missing-folder",
      }),
    ).rejects.toThrow("Folder not found");
  });

  it("honors source ids for local folder and tag imports", async () => {
    const folder = await dataClient.createFolder("local-space", "Imported Folder", {
      source_id: "server-folder-1",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Imported Tag",
      source_id: "server-tag-1",
    });

    expect(folder.id).toBe("server-folder-1");
    expect(tag.id).toBe("server-tag-1");
    expect((await dataClient.listFolders("local-space")).map((item) => item.id)).toContain(
      "server-folder-1",
    );
    expect((await dataClient.listTags("local-space")).map((item) => item.id)).toContain(
      "server-tag-1",
    );
  });

  it("exports and imports a local vault snapshot", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Portable Note",
      content: "backup me",
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.documents.some((doc) => doc.id === created.id)).toBe(true);

    await dataClient.importLocalVaultSnapshot({
      ...exported,
      documents: exported.documents.map((doc) =>
        doc.id === created.id ? { ...doc, title: "Restored Note" } : doc,
      ),
    });

    const restored = await dataClient.getDocument("local-space", created.id);
    expect(restored.title).toBe("Restored Note");
  });

  it("exports local personal space owner metadata for mirror paths", async () => {
    const exported = await dataClient.exportLocalVaultSnapshot();
    const personalSpace = exported.spaces.find((space) => space.id === "local-space");

    expect(personalSpace).toMatchObject({
      owner_username: "local-user",
      owner_display_name: "Local User",
      created_by: "local-user",
    });
  });

  it("backfills legacy local personal space owner metadata on import", async () => {
    const exported = await dataClient.exportLocalVaultSnapshot();
    await dataClient.importLocalVaultSnapshot({
      ...exported,
      spaces: exported.spaces.map((space) =>
        space.id === "local-space"
          ? {
              id: space.id,
              name: space.name,
              slug: space.slug,
              created_at: space.created_at,
              updated_at: space.updated_at,
            }
          : space,
      ),
    });

    const reexported = await dataClient.exportLocalVaultSnapshot();
    const personalSpace = reexported.spaces.find((space) => space.id === "local-space");
    expect(personalSpace?.owner_username).toBe("local-user");
    expect(personalSpace?.owner_display_name).toBe("Local User");
  });

  it("previews standalone vault migration warnings", async () => {
    const exported = await dataClient.exportLocalVaultSnapshot();
    const summary = dataClient.previewVaultMigration({
      ...exported,
      assets: [
        {
          id: "asset-1",
          name: "diagram.png",
          size: 120,
          url: "asset://diagram.png",
          source: "browser",
          created_at: new Date().toISOString(),
        },
      ],
      tags: [
        {
          id: "tag-1",
          space_id: "local-space",
          name: "Roadmap",
          color: "#2563eb",
          created_at: "2026-07-05T00:00:00Z",
        },
      ],
      doc_tags: [{ document_id: "local-welcome", tag_id: "tag-1" }],
    });

    expect(summary.spaces).toBe(1);
    expect(summary.documents).toBe(1);
    expect(summary.assets).toBe(1);
    expect(summary.tags).toBe(1);
    expect(summary.warnings.some((warning) => warning.includes("Asset binaries"))).toBe(true);
    expect(summary.warnings.some((warning) => warning.includes("tag timestamps"))).toBe(false);
  });

  it("imports standalone vault content into a server with mapped folders", async () => {
    setCurrentLaunchMode("server");
    const assetUrl = "data:text/plain;base64,SGVsbG8=";
    const uploadedAssetUrl =
      "/api/v1/files/spaces/field-notes/attachments/source-doc/server-attachment.txt";
    const requests: Array<{ path: string; method: string; body?: unknown }> = [];
    let folderCount = 0;
    let propertyCount = 0;
    let tagCount = 0;

    (globalThis as { fetch?: unknown }).fetch = (async (url: string, init?: RequestInit) => {
      const path = url.replace("/api/v1", "");
      const method = init?.method ?? "GET";
      const body =
        typeof init?.body === "string" && init.body
          ? JSON.parse(init.body)
          : init?.body instanceof FormData
            ? init.body
            : undefined;
      requests.push({ path, method, body });

      let response: unknown = {};
      if (path === "/spaces" && method === "POST") {
        response = { id: body.source_id, name: body.name, slug: "field-notes" };
      } else if (path === "/spaces/source-space/folders" && method === "POST") {
        folderCount += 1;
        response = {
          id: body.source_id ?? `server-folder-${folderCount}`,
          name: body.name,
          parent_id: body.parent_id,
        };
      } else if (path === "/spaces/source-space/documents" && method === "POST") {
        response = { id: body.source_id };
      } else if (
        path === "/spaces/source-space/documents/source-doc/attachments" &&
        method === "POST"
      ) {
        response = {
          id: "server-attachment",
          filename: "server-attachment.txt",
          original_name: "diagram.txt",
          mime_type: "text/plain",
          size_bytes: 5,
          url: uploadedAssetUrl,
          created_at: "2026-07-05T00:00:00Z",
        };
      } else if (path === "/spaces/source-space/documents/source-doc/content" && method === "PUT") {
        response = {};
      } else if (path === "/spaces/source-space/templates" && method === "POST") {
        response = { id: body.source_id };
      } else if (path === "/spaces/source-space/properties" && method === "POST") {
        propertyCount += 1;
        response = {
          id: body.source_id ?? `server-property-${propertyCount}`,
          name: body.name,
          property_type: body.property_type,
          options: body.options,
          sort_order: propertyCount,
        };
      } else if (path === "/spaces/source-space/properties/reorder" && method === "PUT") {
        response = {};
      } else if (path === "/spaces/source-space/tags" && method === "POST") {
        tagCount += 1;
        response = {
          id: body.source_id ?? `server-tag-${tagCount}`,
          name: body.name,
          color: body.color ?? "#6b7280",
          created_at: body.created_at ?? "2026-07-05T00:00:00Z",
        };
      } else if (path === "/spaces/source-space/workflow-templates" && method === "POST") {
        response = { id: body.source_id };
      } else if (
        path === "/spaces/source-space/documents/source-doc/properties" &&
        method === "PUT"
      ) {
        response = {};
      } else if (path === "/spaces/source-space/documents/source-doc/tags" && method === "POST") {
        response = {};
      } else if (
        path === "/spaces/source-space/documents/source-doc/workflow" &&
        method === "PUT"
      ) {
        response = {
          id: "server-document-workflow",
          document_id: "source-doc",
          workflow_template_id: body.template_id,
          workflow_template_name: "Approval",
          current_stage_id: body.stage_id,
          current_stage_name: "Approved",
          status: "active",
          stages: [],
          approvers: [],
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        };
      } else if (
        path === "/spaces/source-space/documents/source-doc/workflow/events" &&
        method === "POST"
      ) {
        response = [];
      }

      return {
        ok: true,
        status: 200,
        json: async () => response,
      } as Response;
    }) as typeof fetch;

    const summary = await dataClient.importVaultSnapshotToServer({
      schema_version: 1,
      spaces: [
        {
          id: "source-space",
          name: "Field Notes",
          slug: "field-notes",
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        },
      ],
      folders: [
        { id: "parent-folder", space_id: "source-space", name: "Projects" },
        {
          id: "child-folder",
          space_id: "source-space",
          name: "Launch",
          parent_id: "parent-folder",
        },
      ],
      documents: [
        {
          id: "source-doc",
          space_id: "source-space",
          title: "Runbook",
          content: `# Runbook\n![Diagram](${assetUrl})`,
          folder_id: "child-folder",
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        },
      ],
      assets: [
        {
          id: "asset-1",
          name: "diagram.txt",
          mime_type: "text/plain",
          size: 5,
          url: assetUrl,
          source: "browser",
          created_at: "2026-07-05T00:00:00Z",
        },
      ],
      templates: [
        {
          id: "template-1",
          space_id: "source-space",
          name: "Brief",
          content: "# Brief",
          tags: [],
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        },
      ],
      workflow_templates: [
        {
          id: "workflow-1",
          space_id: "source-space",
          name: "Approval",
          stages: [
            { id: "draft", name: "Draft", sort_order: 0 },
            { id: "approved", name: "Approved", sort_order: 1 },
          ],
          approvers: [],
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        },
      ],
      document_workflows: [
        {
          id: "document-workflow-1",
          document_id: "source-doc",
          workflow_template_id: "workflow-1",
          current_stage_id: "approved",
          status: "active",
          created_at: "2026-07-05T00:00:00Z",
          updated_at: "2026-07-05T00:00:00Z",
        },
      ],
      document_workflow_events: [
        {
          id: "workflow-event-1",
          document_workflow_id: "document-workflow-1",
          document_id: "source-doc",
          to_stage_id: "draft",
          action: "enable",
          created_by: "source-user",
          created_at: "2026-07-05T00:00:00Z",
        },
        {
          id: "workflow-event-2",
          document_workflow_id: "document-workflow-1",
          document_id: "source-doc",
          from_stage_id: "draft",
          to_stage_id: "approved",
          action: "transition",
          note: "Approved after review",
          created_by: "source-user",
          created_at: "2026-07-05T00:05:00Z",
        },
        {
          id: "workflow-event-3",
          document_workflow_id: "document-workflow-1",
          document_id: "source-doc",
          to_stage_id: "approved",
          action: "delegate",
          note: "Delegated to editor",
          created_by: "source-user",
          created_at: "2026-07-05T00:10:00Z",
        },
      ],
      property_schemas: [
        {
          id: "property-1",
          space_id: "source-space",
          name: "Status",
          property_type: "select",
          options: ["Draft", "Approved"],
          sort_order: 2,
        },
        {
          id: "property-2",
          space_id: "source-space",
          name: "Priority",
          property_type: "text",
          sort_order: 1,
        },
      ],
      property_values: [
        {
          id: "property-value-1",
          document_id: "source-doc",
          property_id: "property-1",
          value: "Approved",
        },
      ],
      tags: [
        {
          id: "tag-1",
          space_id: "source-space",
          name: "Ops",
          color: "#2563eb",
          created_at: "2026-07-05T00:00:00Z",
        },
      ],
      doc_tags: [{ document_id: "source-doc", tag_id: "tag-1" }],
    });

    const folderRequests = requests.filter((request) => request.path.endsWith("/folders"));
    const spaceRequest = requests.find(
      (request) => request.path === "/spaces" && request.method === "POST",
    );
    const documentRequest = requests.find((request) => request.path.endsWith("/documents"));
    const templateRequest = requests.find(
      (request) => request.path === "/spaces/source-space/templates",
    );
    const attachmentRequest = requests.find(
      (request) => request.path === "/spaces/source-space/documents/source-doc/attachments",
    );
    const contentUpdateRequest = requests.find(
      (request) => request.path === "/spaces/source-space/documents/source-doc/content",
    );
    const propertyRequests = requests.filter(
      (request) => request.path === "/spaces/source-space/properties",
    );
    const propertyValueRequest = requests.find(
      (request) => request.path === "/spaces/source-space/documents/source-doc/properties",
    );
    const tagAssignmentRequest = requests.find(
      (request) => request.path === "/spaces/source-space/documents/source-doc/tags",
    );
    const tagRequest = requests.find((request) => request.path === "/spaces/source-space/tags");
    const workflowEnableRequest = requests.find(
      (request) => request.path === "/spaces/source-space/documents/source-doc/workflow",
    );
    const workflowTemplateRequest = requests.find(
      (request) => request.path === "/spaces/source-space/workflow-templates",
    );
    const workflowEventRequests = requests.filter(
      (request) => request.path === "/spaces/source-space/documents/source-doc/workflow/events",
    );

    expect(summary.spaces).toBe(1);
    expect(summary.folders).toBe(2);
    expect(summary.assets).toBe(1);
    expect(summary.property_schemas).toBe(2);
    expect(summary.property_values).toBe(1);
    expect(summary.tags).toBe(1);
    expect(summary.doc_tags).toBe(1);
    expect(summary.document_workflows).toBe(1);
    expect(spaceRequest?.body).toMatchObject({
      name: "Field Notes",
      source_id: "source-space",
      created_at: "2026-07-05T00:00:00Z",
      updated_at: "2026-07-05T00:00:00Z",
    });
    expect(folderRequests.map((request) => request.body)).toEqual([
      { name: "Projects", parent_id: null, source_id: "parent-folder" },
      { name: "Launch", parent_id: "parent-folder", source_id: "child-folder" },
    ]);
    expect(documentRequest?.body).toMatchObject({
      title: "Runbook",
      content: `# Runbook\n![Diagram](${assetUrl})`,
      folder_id: "child-folder",
      source_id: "source-doc",
      created_at: "2026-07-05T00:00:00Z",
      updated_at: "2026-07-05T00:00:00Z",
    });
    expect(attachmentRequest?.body).toBeInstanceOf(FormData);
    expect(contentUpdateRequest?.body).toEqual({
      content: `# Runbook\n![Diagram](${uploadedAssetUrl})`,
    });
    expect(templateRequest?.body).toMatchObject({
      name: "Brief",
      source_id: "template-1",
      created_at: "2026-07-05T00:00:00Z",
      updated_at: "2026-07-05T00:00:00Z",
    });
    expect(propertyRequests.map((request) => request.body)).toEqual([
      { name: "Priority", property_type: "text", source_id: "property-2" },
      {
        name: "Status",
        property_type: "select",
        options: ["Draft", "Approved"],
        source_id: "property-1",
      },
    ]);
    expect(propertyValueRequest?.body).toEqual({
      values: [{ property_id: "property-1", value: "Approved", source_id: "property-value-1" }],
    });
    expect(tagAssignmentRequest?.body).toEqual({ tag_id: "tag-1" });
    expect(tagRequest?.body).toMatchObject({
      name: "Ops",
      source_id: "tag-1",
      created_at: "2026-07-05T00:00:00Z",
    });
    expect(workflowTemplateRequest?.body).toMatchObject({
      name: "Approval",
      source_id: "workflow-1",
      created_at: "2026-07-05T00:00:00Z",
      updated_at: "2026-07-05T00:00:00Z",
    });
    expect(workflowEnableRequest?.body).toEqual({
      template_id: "workflow-1",
      stage_id: "approved",
      source_id: "document-workflow-1",
      event_source_id: "workflow-event-1",
      event_created_by: "source-user",
      event_created_at: "2026-07-05T00:00:00Z",
      created_at: "2026-07-05T00:00:00Z",
      updated_at: "2026-07-05T00:00:00Z",
    });
    expect(workflowEventRequests.map((request) => request.body)).toEqual([
      {
        action: "transition",
        note: "Approved after review",
        source_id: "workflow-event-2",
        created_by: "source-user",
        created_at: "2026-07-05T00:05:00Z",
        from_stage_id: "draft",
        to_stage_id: "approved",
      },
      {
        action: "delegate",
        note: "Delegated to editor",
        source_id: "workflow-event-3",
        created_by: "source-user",
        created_at: "2026-07-05T00:10:00Z",
        from_stage_id: undefined,
        to_stage_id: "approved",
      },
    ]);
  });

  it("creates, updates, deletes, and exports local templates", async () => {
    const created = await dataClient.createTemplate("local-space", {
      name: "Project Brief",
      description: "Reusable kickoff template",
      content: "# {{project}}\n\n## Goals",
      tags: ["project", "brief"],
      properties: { status: "draft" },
      source_id: "source-template",
    });
    expect(created.id).toBe("source-template");

    let templates = await dataClient.listTemplates("local-space");
    expect(templates.map((template) => template.name)).toEqual(["Project Brief"]);
    expect(templates[0].tags).toEqual(["project", "brief"]);

    await dataClient.updateTemplate("local-space", created.id, {
      name: "Project Brief Updated",
      content: "# Updated",
      tags: ["project"],
    });

    const updated = await dataClient.getTemplate("local-space", created.id);
    expect(updated.name).toBe("Project Brief Updated");
    expect(updated.content).toBe("# Updated");

    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.templates.some((template) => template.id === created.id)).toBe(true);

    await dataClient.deleteTemplate("local-space", created.id);
    templates = await dataClient.listTemplates("local-space");
    expect(templates).toEqual([]);

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    expect(manifest.operations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          entity_type: "template",
          entity_id: "source-template",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "source-template",
            name: "Project Brief",
            tags: ["project", "brief"],
          }),
        }),
        expect.objectContaining({
          entity_type: "template",
          entity_id: "source-template",
          action: "update",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "source-template",
            name: "Project Brief Updated",
            tags: ["project"],
          }),
        }),
        expect.objectContaining({
          entity_type: "template",
          entity_id: "source-template",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "source-template",
            name: "Project Brief Updated",
          }),
        }),
      ]),
    );
  });

  it("creates, updates, deletes, and exports local workflow templates", async () => {
    const created = await dataClient.createWorkflowTemplate("local-space", {
      name: "Draft Review",
      description: "Default document approval flow",
      source_id: "workflow-template-source",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
        { id: "approved", name: "Approved", sort_order: 2 },
      ],
      approvers: [{ approver_type: "role", approver_id: "owner", stage_id: "review" }],
    });
    expect(created.id).toBe("workflow-template-source");

    let workflows = await dataClient.listWorkflowTemplates("local-space");
    expect(workflows.map((template) => template.name)).toEqual(["Draft Review"]);
    expect(workflows[0].stages.map((stage) => stage.name)).toEqual(["Draft", "Review", "Approved"]);
    expect(workflows[0].approvers[0].approver_id).toBe("owner");

    await dataClient.updateWorkflowTemplate("local-space", created.id, {
      name: "Legal Review",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "legal", name: "Legal", sort_order: 1 },
        { id: "published", name: "Published", sort_order: 2 },
      ],
      approvers: [{ approver_type: "role", approver_id: "admin", stage_id: "legal" }],
    });

    const updated = await dataClient.getWorkflowTemplate("local-space", created.id);
    expect(updated.name).toBe("Legal Review");
    expect(updated.stages.map((stage) => stage.id)).toEqual(["draft", "legal", "published"]);

    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.workflow_templates?.some((template) => template.id === created.id)).toBe(true);

    await dataClient.deleteWorkflowTemplate("local-space", created.id);
    workflows = await dataClient.listWorkflowTemplates("local-space");
    expect(workflows).toEqual([]);

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    expect(manifest.operations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          entity_type: "workflow_template",
          entity_id: "workflow-template-source",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "workflow-template-source",
            name: "Draft Review",
            stages: expect.arrayContaining([expect.objectContaining({ id: "draft" })]),
          }),
        }),
        expect.objectContaining({
          entity_type: "workflow_template",
          entity_id: "workflow-template-source",
          action: "update",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "workflow-template-source",
            name: "Legal Review",
            stages: expect.arrayContaining([expect.objectContaining({ id: "legal" })]),
          }),
        }),
        expect.objectContaining({
          entity_type: "workflow_template",
          entity_id: "workflow-template-source",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "workflow-template-source",
            name: "Legal Review",
          }),
        }),
      ]),
    );
  });

  it("enables, exports, and disables local document workflows", async () => {
    const created = await dataClient.createWorkflowTemplate("local-space", {
      name: "Draft Review",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [{ approver_type: "role", approver_id: "owner", stage_id: "review" }],
    });

    const workflow = await dataClient.enableDocumentWorkflow("local-space", "local-welcome", {
      template_id: created.id,
    });
    expect(workflow.workflow_template_name).toBe("Draft Review");
    expect(workflow.current_stage_name).toBe("Draft");

    const loaded = await dataClient.getDocumentWorkflow("local-space", "local-welcome");
    expect(loaded?.workflow_template_id).toBe(created.id);

    const transitioned = await dataClient.transitionDocumentWorkflow(
      "local-space",
      "local-welcome",
      { stage_id: "review" },
    );
    expect(transitioned.current_stage_name).toBe("Review");
    expect(transitioned.status).toBe("completed");

    await dataClient.recordDocumentWorkflowEvent("local-space", "local-welcome", {
      action: "delegate",
      note: "Hand off review",
      source_id: "workflow-event-delegate-source",
      created_by: "user-imported",
      created_at: "2026-07-08T12:00:00.000Z",
      from_stage_id: "draft",
      to_stage_id: "review",
    });

    const events = await dataClient.listDocumentWorkflowEvents("local-space", "local-welcome");
    expect(events.map((event) => event.action).sort()).toEqual([
      "delegate",
      "enable",
      "transition",
    ]);
    expect(events.find((event) => event.action === "delegate")).toMatchObject({
      id: "workflow-event-delegate-source",
      created_by: "user-imported",
      created_at: "2026-07-08T12:00:00.000Z",
    });

    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.document_workflows?.some((item) => item.document_id === "local-welcome")).toBe(
      true,
    );
    expect(
      exported.document_workflow_events?.some((item) => item.document_id === "local-welcome"),
    ).toBe(true);

    await dataClient.disableDocumentWorkflow("local-space", "local-welcome");
    await expect(
      dataClient.getDocumentWorkflow("local-space", "local-welcome"),
    ).resolves.toBeNull();

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    expect(manifest.operations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          entity_type: "document_workflow",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({
            document_id: "local-welcome",
            workflow_template_id: created.id,
            current_stage_id: "draft",
            space_id: "local-space",
            event: expect.objectContaining({ action: "enable", to_stage_id: "draft" }),
          }),
        }),
        expect.objectContaining({
          entity_type: "document_workflow",
          action: "transition",
          payload_status: "available",
          payload: expect.objectContaining({
            document_id: "local-welcome",
            current_stage_id: "review",
            space_id: "local-space",
            event: expect.objectContaining({
              action: "transition",
              from_stage_id: "draft",
              to_stage_id: "review",
            }),
          }),
        }),
        expect.objectContaining({
          entity_type: "document_workflow_event",
          action: "delegate",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "workflow-event-delegate-source",
            document_id: "local-welcome",
            document_workflow_id: expect.any(String),
            action: "delegate",
            created_by: "user-imported",
            created_at: "2026-07-08T12:00:00.000Z",
            space_id: "local-space",
          }),
        }),
        expect.objectContaining({
          entity_type: "document_workflow",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({
            document_id: "local-welcome",
            workflow_template_id: created.id,
            space_id: "local-space",
          }),
        }),
      ]),
    );
  });

  it("filters local document lists by workflow state", async () => {
    const created = await dataClient.createWorkflowTemplate("local-space", {
      name: "Approval Queue",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
    });
    const queued = await dataClient.createDocument("local-space", {
      title: "Queued",
      content: "needs review",
    });
    const folder = await dataClient.createFolder("local-space", "Nested");
    const nestedQueued = await dataClient.createDocument("local-space", {
      title: "Nested queued",
      folder_id: folder.id,
      content: "also needs review",
    });
    const unqueued = await dataClient.createDocument("local-space", {
      title: "Unqueued",
      content: "no workflow",
    });

    await dataClient.enableDocumentWorkflow("local-space", queued.id, {
      template_id: created.id,
    });
    await dataClient.enableDocumentWorkflow("local-space", nestedQueued.id, {
      template_id: created.id,
    });

    const draftDocs = await dataClient.listDocuments("local-space", {
      include_nested: true,
      workflow_status: "active",
      workflow_stage_id: "draft",
    });
    expect(draftDocs.map((doc) => doc.id)).toContain(queued.id);
    expect(draftDocs.map((doc) => doc.id)).toContain(nestedQueued.id);
    expect(draftDocs.map((doc) => doc.id)).not.toContain(unqueued.id);
    expect(draftDocs.find((doc) => doc.id === queued.id)?.workflow_stage_name).toBe("Draft");

    await dataClient.transitionDocumentWorkflow("local-space", queued.id, {
      stage_id: "review",
    });
    const completedDocs = await dataClient.listDocuments("local-space", {
      workflow_status: "completed",
      workflow_template_id: created.id,
    });
    expect(completedDocs.map((doc) => doc.id)).toContain(queued.id);
  });

  it("enforces local workflow stage approvers", async () => {
    const created = await dataClient.createWorkflowTemplate("local-space", {
      name: "Approval Guard",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "owner-review", name: "Owner Review", sort_order: 1 },
        { id: "admin-review", name: "Admin Review", sort_order: 2 },
      ],
      approvers: [
        { approver_type: "role", approver_id: "owner", stage_id: "owner-review" },
        { approver_type: "role", approver_id: "admin", stage_id: "admin-review" },
      ],
    });

    await dataClient.enableDocumentWorkflow("local-space", "local-welcome", {
      template_id: created.id,
    });
    await expect(
      dataClient.transitionDocumentWorkflow("local-space", "local-welcome", {
        stage_id: "owner-review",
      }),
    ).resolves.toMatchObject({ current_stage_id: "owner-review" });
    await expect(
      dataClient.transitionDocumentWorkflow("local-space", "local-welcome", {
        stage_id: "admin-review",
      }),
    ).rejects.toThrow("Workflow transition requires an assigned approver");
  });

  it("creates, updates, deletes, and exports local properties and tags", async () => {
    const createdDoc = await dataClient.createDocument("local-space", {
      title: "Structured",
      content: "with metadata",
    });

    const property = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "select",
      options: ["Draft", "Published"],
      source_id: "property-status",
    });
    expect(property.id).toBe("property-status");
    await dataClient.setDocumentPropertyValues("local-space", createdDoc.id, [
      { property_id: property.id, value: "Draft", source_id: "value-status" },
    ]);

    let values = await dataClient.listDocumentProperties("local-space", createdDoc.id);
    expect(values.find((value) => value.property_id === property.id)?.value).toBe("Draft");
    expect(values.find((value) => value.property_id === property.id)?.options).toEqual([
      "Draft",
      "Published",
    ]);

    await dataClient.updatePropertySchema("local-space", property.id, {
      name: "State",
      property_type: "url",
    });
    values = await dataClient.listDocumentProperties("local-space", createdDoc.id);
    expect(values.find((value) => value.property_id === property.id)?.property_name).toBe("State");
    expect(values.find((value) => value.property_id === property.id)?.property_type).toBe("url");

    const priorityProperty = await dataClient.createPropertySchema("local-space", {
      name: "Priority",
      property_type: "text",
      source_id: "property-priority",
    });
    const ownerProperty = await dataClient.createPropertySchema("local-space", {
      name: "Owner",
      property_type: "user",
      source_id: "property-owner",
    });
    await dataClient.reorderPropertySchemas("local-space", {
      property_ids: [priorityProperty.id, property.id, ownerProperty.id],
    });
    await dataClient.setDocumentPropertyValues("local-space", createdDoc.id, [
      { property_id: ownerProperty.id, value: "local-user" },
    ]);
    values = await dataClient.listDocumentProperties("local-space", createdDoc.id);
    expect(values.map((value) => value.property_id)).toEqual([
      priorityProperty.id,
      property.id,
      ownerProperty.id,
    ]);
    expect(values.find((value) => value.property_id === ownerProperty.id)?.value).toBe(
      "local-user",
    );
    await expect(dataClient.listUserDirectory()).resolves.toEqual([
      {
        id: "local-user",
        email: "local@starlane.notes",
        display_name: "Local User",
        avatar_url: null,
      },
    ]);

    const tag = await dataClient.createTag("local-space", {
      name: "Project",
      color: "#2563eb",
    });
    await dataClient.addDocumentTag("local-space", createdDoc.id, tag.id);
    let docTags = await dataClient.listDocumentTags("local-space", createdDoc.id);
    expect(docTags.map((item) => item.name)).toEqual(["Project"]);

    await dataClient.updateTag("local-space", tag.id, { name: "Work", color: "#059669" });
    docTags = await dataClient.listDocumentTags("local-space", createdDoc.id);
    expect(docTags[0].name).toBe("Work");
    expect(docTags[0].color).toBe("#059669");

    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.property_schemas?.some((item) => item.id === property.id)).toBe(true);
    expect(exported.property_values?.some((item) => item.property_id === property.id)).toBe(true);
    expect(exported.tags?.find((item) => item.id === tag.id)?.created_at).toBeTruthy();
    expect(exported.doc_tags?.some((item) => item.tag_id === tag.id)).toBe(true);

    await dataClient.deletePropertySchema("local-space", property.id);
    await dataClient.deletePropertySchema("local-space", priorityProperty.id);
    await dataClient.deletePropertySchema("local-space", ownerProperty.id);
    expect(await dataClient.listDocumentProperties("local-space", createdDoc.id)).toEqual([]);

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    expect(manifest.operations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          entity_type: "property_schema",
          entity_id: "property-status",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "property-status",
            name: "Status",
            property_type: "select",
          }),
        }),
        expect.objectContaining({
          entity_type: "property_schema",
          entity_id: "property-status",
          action: "update",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "property-status",
            name: "State",
            property_type: "url",
          }),
        }),
        expect.objectContaining({
          entity_type: "property_schema",
          entity_id: "local-space",
          action: "reorder",
          payload_status: "available",
          payload: expect.objectContaining({
            space_id: "local-space",
            property_ids: ["property-priority", "property-status", "property-owner"],
          }),
        }),
        expect.objectContaining({
          entity_type: "document_property_values",
          entity_id: createdDoc.id,
          action: "set",
          payload_status: "available",
          payload: expect.objectContaining({
            document_id: createdDoc.id,
            space_id: "local-space",
            values: expect.arrayContaining([
              expect.objectContaining({
                id: "value-status",
                property_id: "property-status",
                value: "Draft",
              }),
            ]),
          }),
        }),
        expect.objectContaining({
          entity_type: "property_schema",
          entity_id: "property-status",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({
            id: "property-status",
            space_id: "local-space",
          }),
        }),
      ]),
    );

    await dataClient.deleteTag("local-space", tag.id);
    expect(await dataClient.listDocumentTags("local-space", createdDoc.id)).toEqual([]);
  });

  it("rejects invalid local vault imports", async () => {
    await expect(dataClient.importLocalVaultSnapshot({ schema_version: 999 })).rejects.toThrow(
      "Unsupported vault schema version",
    );
  });

  it("validates required title and unknown space in local mode", async () => {
    await expect(dataClient.createDocument("local-space", { title: "   " })).rejects.toThrow(
      "Document title is required",
    );

    await expect(dataClient.listDocuments("missing-space")).rejects.toThrow("Space not found");
  });

  it("builds local graph nodes from local documents", async () => {
    const created = await dataClient.createDocument("local-space", { title: "Graph Node" });
    const graph = await dataClient.getGraph("local-space");
    expect(graph.nodes.some((n) => n.id === created.id)).toBe(true);
    expect(graph.edges.length).toBe(0);
  });

  it("reports local sync state", async () => {
    expect(await dataClient.getSyncState()).toBe("idle");
  });

  it("records and clears queued local sync operations", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Queued Write",
      content: "first draft",
    });
    const asset = await dataClient.saveBrowserAsset({
      name: "diagram.txt",
      mime_type: "text/plain",
      size: 5,
      url: "data:text/plain;base64,SGVsbG8=",
      space_id: "local-space",
      document_id: created.id,
    });
    const folder = await dataClient.createFolder("local-space", "Queued Folder");
    await dataClient.updateDocumentContent("local-space", created.id, "second draft");
    const tag = await dataClient.createTag("local-space", { name: "Queued" });
    await dataClient.addDocumentTag("local-space", created.id, tag.id);
    await dataClient.removeDocumentTag("local-space", created.id, tag.id);
    await dataClient.deleteTag("local-space", tag.id);
    await dataClient.deleteFolder("local-space", folder.id);
    writeSharedSpaceDownloadCheckpoints([
      {
        spaceId: "server-space",
        spaceName: "Server Space",
        slug: "server-space",
        sourceServerUrl: "https://notes.example.test",
        downloadedByUserId: "admin-user",
        downloadedByDisplayName: "Admin User",
        downloadedAt: "2026-07-07T01:00:00.000Z",
        documentCount: 12,
        folderCount: 3,
        tagCount: 2,
        propertyCount: 1,
      },
    ]);

    const operations = await dataClient.listQueuedSyncOperations();
    expect(operations.map((operation) => `${operation.entity_type}:${operation.action}`)).toEqual(
      expect.arrayContaining([
        "document:create",
        "document:update_content",
        "folder:create",
        "folder:delete",
        "tag:create",
        "tag:delete",
        "document_tag:add",
        "document_tag:remove",
        "asset:create",
      ]),
    );

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    expect(manifest.operation_count).toBe(operations.length);
    expect(manifest.replay_contract).toEqual({
      schema_version: 1,
      operation_order: "queued_at_ascending",
      idempotency_key: "operation.id",
      transport: "server-api",
      server_backends: ["sqlite", "postgresql"],
      required_payload_status: "available",
      conflict_policy: "server_validates_version_and_permissions",
      web_canonical_store: "server-api",
    });
    expect(manifest.replay_readiness).toEqual({
      ready: true,
      ready_operation_count: operations.length,
      blocked_operation_count: 0,
      blockers: [],
      target_backends: ["sqlite", "postgresql"],
    });
    expect(manifest.operation_summary).toEqual({
      total_operations: operations.length,
      missing_payload_count: 0,
      ready_operation_count: operations.length,
      by_entity_type: expect.objectContaining({
        document: expect.any(Number),
        document_tag: 2,
        asset: 1,
        folder: 2,
        tag: 2,
      }),
      by_action: expect.objectContaining({
        create: expect.any(Number),
        delete: 2,
        update_content: 1,
        add: 1,
        remove: 1,
      }),
      unavailable_payloads: [],
    });
    expect(manifest.provider).toBe("none");
    expect(manifest.recovery_checkpoint_count).toBe(1);
    expect(manifest.recovery_checkpoints).toEqual([
      expect.objectContaining({
        spaceId: "server-space",
        spaceName: "Server Space",
        sourceServerUrl: "https://notes.example.test",
        downloadedByUserId: "admin-user",
        documentCount: 12,
      }),
    ]);
    expect(manifest.operations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          entity_type: "document",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({ id: created.id, title: "Queued Write" }),
        }),
        expect.objectContaining({
          entity_type: "document_tag",
          action: "add",
          payload_status: "available",
          payload: { document_id: created.id, tag_id: tag.id },
        }),
        expect.objectContaining({
          entity_type: "document_tag",
          action: "remove",
          payload_status: "available",
          payload: { document_id: created.id, tag_id: tag.id, space_id: "local-space" },
        }),
        expect.objectContaining({
          entity_type: "tag",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({ id: tag.id, space_id: "local-space" }),
        }),
        expect.objectContaining({
          entity_type: "folder",
          action: "delete",
          payload_status: "available",
          payload: expect.objectContaining({ id: folder.id, space_id: "local-space" }),
        }),
        expect.objectContaining({
          entity_type: "asset",
          action: "create",
          payload_status: "available",
          payload: expect.objectContaining({
            id: asset.id,
            name: "diagram.txt",
            space_id: "local-space",
            document_id: created.id,
            data_url: "data:text/plain;base64,SGVsbG8=",
          }),
        }),
      ]),
    );

    const subsetManifest = await dataClient.exportQueuedSyncReplayManifest([operations[0].id]);
    expect(subsetManifest.operation_count).toBe(1);
    expect(subsetManifest.operations).toHaveLength(1);
    expect(subsetManifest.operations[0]?.id).toBe(operations[0].id);

    await dataClient.clearQueuedSyncOperations([operations[0].id]);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operations.length - 1);

    await dataClient.clearQueuedSyncOperations();
    expect(await dataClient.listQueuedSyncOperations()).toEqual([]);
  });

  it("applies remote provider space create and update imports through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-space-create-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-space-create-1",
            entity_type: "space",
            entity_id: "remote-space-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-space-created",
              name: "Remote Space",
              slug: "remote-space",
              owner_username: "remote-owner",
              owner_display_name: "Remote Owner",
              created_by: "remote-owner",
              created_at: "2026-07-09T12:00:00.000Z",
              updated_at: "2026-07-09T12:01:00.000Z",
            },
          },
          {
            id: "remote-space-update-1",
            entity_type: "space",
            entity_id: "local-space",
            action: "update",
            payload_status: "available",
            payload: {
              id: "local-space",
              name: "Remote Renamed Personal Space",
              slug: "remote-renamed-personal-space",
              owner_username: "local-user",
              owner_display_name: "Local User",
              updated_at: "2026-07-09T12:02:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 2,
      applied_operation_count: 2,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-space-create-1",
          status: "applied",
        },
        {
          operation_id: "remote-space-update-1",
          status: "applied",
        },
      ],
    });
    expect(await dataClient.getSyncState()).toBe("idle");
    await expect(dataClient.listSpaces()).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-space-created",
          name: "Remote Space",
          slug: "remote-space",
        }),
        expect.objectContaining({
          id: "local-space",
          name: "Remote Renamed Personal Space",
          slug: "remote-renamed-personal-space",
        }),
      ]),
    );
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    expect(snapshot.spaces).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-space-created",
          owner_username: "remote-owner",
          owner_display_name: "Remote Owner",
          created_by: "remote-owner",
          created_at: "2026-07-09T12:00:00.000Z",
          updated_at: "2026-07-09T12:01:00.000Z",
        }),
        expect.objectContaining({
          id: "local-space",
          owner_username: "local-user",
          owner_display_name: "Local User",
          updated_at: "2026-07-09T12:02:00.000Z",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("recovers a provider-deleted space without touching assets owned by another space", async () => {
    const deletedSpace = await dataClient.createSpace("Deleted Remote Space");
    const survivorSpace = await dataClient.createSpace("Surviving Local Space");
    const deletedDocument = await dataClient.createDocument(deletedSpace.id, { title: "Delete me" });
    const survivorDocument = await dataClient.createDocument(survivorSpace.id, { title: "Keep me" });
    await dataClient.saveBrowserAsset({
      name: "delete.txt",
      size: 5,
      url: "data:text/plain;base64,SGVsbG8=",
      space_id: deletedSpace.id,
      document_id: deletedDocument.id,
    });
    await dataClient.saveBrowserAsset({
      name: "keep.txt",
      size: 5,
      url: "data:text/plain;base64,SGVsbG8=",
      space_id: survivorSpace.id,
      document_id: survivorDocument.id,
    });
    await dataClient.clearQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        { provider: "google-drive", connected: true, account_label: "Google account", scopes: ["drive.file"], sync_mode: "manual" },
      ),
      "2026-07-13T12:00:00.000Z",
      "remote-space-recovery",
    );
    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [{
          id: "remote-space-delete",
          entity_type: "space",
          entity_id: deletedSpace.id,
          action: "delete",
          payload_status: "available",
          payload: { id: deletedSpace.id },
        }],
      }),
      { allowDestructiveSpaceRecovery: true },
    );

    expect(result).toMatchObject({ status: "completed", applied_operation_count: 1 });
    await expect(dataClient.getSpace(deletedSpace.id)).rejects.toThrow("Space not found");
    await expect(dataClient.getDocument(survivorSpace.id, survivorDocument.id)).resolves.toMatchObject({ id: survivorDocument.id });
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    expect(snapshot.assets).toEqual(
      expect.arrayContaining([expect.objectContaining({ name: "keep.txt", space_id: survivorSpace.id })]),
    );
    expect(snapshot.assets).not.toEqual(
      expect.arrayContaining([expect.objectContaining({ name: "delete.txt", space_id: deletedSpace.id })]),
    );
  });

  it("handles idempotent and blocked remote provider space imports", async () => {
    await dataClient.clearQueuedSyncOperations();
    const initialSnapshot = await dataClient.exportLocalVaultSnapshot();
    const existingSpace = initialSnapshot.spaces.find((space) => space.id === "local-space");
    if (!existingSpace) {
      throw new Error("Expected local-space to be present in local vault snapshot");
    }
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-space-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 4,
        operations: [
          {
            id: "remote-space-create-existing-match",
            entity_type: "space",
            entity_id: "local-space",
            action: "create",
            payload_status: "available",
            payload: existingSpace,
          },
          {
            id: "remote-space-create-existing-conflict",
            entity_type: "space",
            entity_id: "local-space",
            action: "create",
            payload_status: "available",
            payload: {
              id: "local-space",
              name: "Conflicting Remote Space",
              slug: existingSpace.slug,
              created_at: existingSpace.created_at,
              updated_at: existingSpace.updated_at,
            },
          },
          {
            id: "remote-space-update-missing",
            entity_type: "space",
            entity_id: "missing-space",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-space",
              name: "Missing Remote Space",
              slug: "missing-remote-space",
            },
          },
          {
            id: "remote-space-create-after-block",
            entity_type: "space",
            entity_id: "remote-space-after-block",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-space-after-block",
              name: "Should Not Apply",
              slug: "should-not-apply",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-space-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-space-create-existing-conflict",
          status: "blocked",
          message: "Remote space create target already exists locally with different metadata",
        }),
        expect.objectContaining({
          operation_id: "remote-space-update-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-space-create-after-block",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    expect(await dataClient.getSyncState()).toBe("conflict");
    await expect(dataClient.listSpaces()).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "local-space",
          name: existingSpace.name,
          slug: existingSpace.slug,
        }),
      ]),
    );
    await expect(dataClient.listSpaces()).resolves.not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: "remote-space-after-block" })]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document creates through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    await dataClient.createFolder("local-space", "Remote Folder", { source_id: "remote-folder" });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-create-1",
            entity_type: "document",
            entity_id: "remote-doc-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-doc-created",
              space_id: "local-space",
              title: "Remote Created Doc",
              content: "created remotely",
              folder_id: "remote-folder",
              created_by: "Remote User",
              last_updated_by: "Remote User",
              created_at: "2026-07-09T12:00:00.000Z",
              updated_at: "2026-07-09T12:01:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-doc-create-1",
          status: "applied",
        },
      ],
    });
    expect(await dataClient.getSyncState()).toBe("idle");
    await expect(
      dataClient.getDocumentContent("local-space", "remote-doc-created"),
    ).resolves.toMatchObject({
      title: "Remote Created Doc",
      content: "created remotely",
    });
    await expect(dataClient.listDocuments("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-doc-created",
          title: "Remote Created Doc",
          folder_id: "remote-folder",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider document creates for missing local folders", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-create-missing-folder",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-create-missing-folder-1",
            entity_type: "document",
            entity_id: "remote-doc-missing-folder",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-doc-missing-folder",
              space_id: "local-space",
              title: "Remote Missing Folder Doc",
              content: "created remotely",
              folder_id: "missing-remote-folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-create-missing-folder-1",
          status: "blocked",
          message: "Folder not found",
        },
      ],
    });
    await expect(
      dataClient.getDocumentContent("local-space", "remote-doc-missing-folder"),
    ).rejects.toThrow("Document not found");
    expect(await dataClient.getSyncState()).toBe("conflict");
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document deletes through the local vault merge executor", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Delete Target",
      content: "local content",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Delete Cleanup",
      color: "#2563eb",
    });
    await dataClient.addDocumentTag("local-space", created.id, tag.id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-delete",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-delete-1",
            entity_type: "document",
            entity_id: created.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Delete Target",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-doc-delete-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).rejects.toThrow(
      "Document not found",
    );
    await expect(dataClient.listDocumentTags("local-space", created.id)).rejects.toThrow(
      "Document not found",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("skips remote provider document deletes when the target document is already absent locally", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-delete-missing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-delete-missing",
            entity_type: "document",
            entity_id: "missing-document",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-document",
              space_id: "local-space",
              title: "Missing Remote Delete",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-doc-delete-missing",
          status: "skipped",
        },
      ],
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider folder creates through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-create-1",
            entity_type: "folder",
            entity_id: "remote-folder-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-folder-created",
              space_id: "local-space",
              name: "Remote Folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-create-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-folder-created",
          name: "Remote Folder",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider folder creates beyond the local hierarchy limit", async () => {
    const root = await dataClient.createFolder("local-space", "Root");
    const child = await dataClient.createFolder("local-space", "Child", {
      parent_id: root.id,
    });
    const grandchild = await dataClient.createFolder("local-space", "Grandchild", {
      parent_id: child.id,
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-create-too-deep",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-create-too-deep",
            entity_type: "folder",
            entity_id: "remote-folder-too-deep",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-folder-too-deep",
              space_id: "local-space",
              name: "Too Deep",
              parent_id: grandchild.id,
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-create-too-deep",
          status: "blocked",
          message: "Folders can be nested up to 3 levels",
        },
      ],
    });
    expect((await dataClient.listFolders("local-space")).map((folder) => folder.id)).not.toContain(
      "remote-folder-too-deep",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document content updates through the local vault merge executor", async () => {
    const folder = await dataClient.createFolder("local-space", "Remote Content Folder");
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Merge Target",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-update-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update_content",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Merge Target",
              content: "remote draft",
              folder_id: folder.id,
              updated_at: "2026-07-09T12:01:00.000Z",
              last_updated_by: "Remote User",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-doc-update-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "remote draft",
    });
    await expect(dataClient.listDocuments("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: created.id,
          folder_id: folder.id,
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider document content updates with mismatched payload identity", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Identity Target",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-update-mismatch",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-update-mismatch-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update_content",
            payload_status: "available",
            payload: {
              id: "different-remote-doc",
              space_id: "local-space",
              title: "Remote Identity Target",
              content: "remote draft",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-update-mismatch-1",
          status: "failed",
          message:
            "Remote replay operation document:update_content payload ID does not match operation entity ID",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local draft",
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider document content updates with mismatched payload spaces", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Space Target",
      content: "local draft",
    });
    const otherSpace = await dataClient.createSpace("Other Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-update-space-mismatch",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-update-space-mismatch-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update_content",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: otherSpace.id,
              title: "Remote Space Target",
              content: "remote draft",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-update-space-mismatch-1",
          status: "failed",
          message: "Remote document content update payload space does not match local document",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local draft",
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider document content updates with missing folder targets", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Folder Target",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-update-missing-folder",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-update-missing-folder-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update_content",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Folder Target",
              content: "remote draft",
              folder_id: "missing-remote-folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-update-missing-folder-1",
          status: "blocked",
          message: "Folder not found",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local draft",
    });
    expect(await dataClient.getSyncState()).toBe("conflict");
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document metadata updates through the local vault merge executor", async () => {
    const folder = await dataClient.createFolder("local-space", "Remote Metadata Folder");
    const created = await dataClient.createDocument("local-space", {
      title: "Local Metadata Target",
      content: "metadata update should not replace content",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-metadata-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-metadata-update-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Metadata Title",
              icon: "sparkles",
              cover_image: "data:image/png;base64,cmVtb3Rl",
              folder_id: folder.id,
              updated_at: "2026-07-09T12:02:00.000Z",
              last_updated_by: "Remote User",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-doc-metadata-update-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.getDocument("local-space", created.id)).resolves.toMatchObject({
      title: "Remote Metadata Title",
      icon: "sparkles",
      cover_image: "data:image/png;base64,cmVtb3Rl",
      last_updated_by: "Remote User",
      updated_at: "2026-07-09T12:02:00.000Z",
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "metadata update should not replace content",
    });
    expect((await dataClient.listDocuments("local-space")).find((doc) => doc.id === created.id))
      .toMatchObject({ folder_id: folder.id });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider document metadata updates with missing folder targets", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Local Metadata Folder Target",
      content: "metadata update should not move to a missing folder",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-metadata-missing-folder",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-metadata-missing-folder-1",
            entity_type: "document",
            entity_id: created.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Metadata Folder Move",
              folder_id: "missing-remote-folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-metadata-missing-folder-1",
          status: "blocked",
          message: "Folder not found",
        },
      ],
    });
    await expect(dataClient.getDocument("local-space", created.id)).resolves.toMatchObject({
      title: "Local Metadata Folder Target",
    });
    expect((await dataClient.listDocuments("local-space")).find((doc) => doc.id === created.id))
      .toMatchObject({ folder_id: undefined });
    expect(await dataClient.getSyncState()).toBe("conflict");
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider space metadata updates through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-space-metadata-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-space-metadata-update-1",
            entity_type: "space",
            entity_id: "local-space",
            action: "update",
            payload_status: "available",
            payload: {
              id: "local-space",
              name: "Remote Personal Space",
              slug: "remote-personal-space",
              owner_username: "remote-owner",
              owner_display_name: "Remote Owner",
              updated_at: "2026-07-09T12:03:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-space-metadata-update-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.getSpace("local-space")).resolves.toMatchObject({
      name: "Remote Personal Space",
      slug: "remote-personal-space",
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider space creates through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-space-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-space-create-1",
            entity_type: "space",
            entity_id: "remote-space",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-space",
              name: "Remote Research",
              slug: "remote-research",
              owner_username: "remote-owner",
              owner_display_name: "Remote Owner",
              created_by: "remote-owner",
              created_at: "2026-07-09T12:02:00.000Z",
              updated_at: "2026-07-09T12:03:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-space-create-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.getSpace("remote-space")).resolves.toMatchObject({
      id: "remote-space",
      name: "Remote Research",
      slug: "remote-research",
    });
    expect(await dataClient.listSpaces()).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: "remote-space", name: "Remote Research" }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);

    const duplicateResult = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(
        buildProviderRemoteMutationEnvelope(
          buildProviderRemoteMutationPlan(
            "google-drive",
            "Google Drive",
            "push",
            {
              status: "enabled",
              root_strategy: "drive-appdata-or-user-selected-folder",
              auth_model: "oauth2-offline-access",
              required_scopes: ["drive.file"],
              write_model: "local-sqlite-first-then-provider-api-push",
              conflict_model: "provider-revision-plus-local-replay-queue-validation",
            },
            {
              provider: "google-drive",
              connected: true,
              account_label: "Google account",
              scopes: ["drive.file"],
              sync_mode: "manual",
            },
          ),
          "2026-07-09T12:04:00.000Z",
          "remote-import-space-create-duplicates",
        ),
        {
          schema_version: 1,
          operation_count: 2,
          operations: [
            {
              id: "remote-space-create-duplicate-match",
              entity_type: "space",
              entity_id: "remote-space",
              action: "create",
              payload_status: "available",
              payload: {
                id: "remote-space",
                name: "Remote Research",
                slug: "remote-research",
                owner_username: "remote-owner",
                owner_display_name: "Remote Owner",
                created_by: "remote-owner",
                created_at: "2026-07-09T12:02:00.000Z",
                updated_at: "2026-07-09T12:03:00.000Z",
              },
            },
            {
              id: "remote-space-create-duplicate-conflict",
              entity_type: "space",
              entity_id: "remote-space",
              action: "create",
              payload_status: "available",
              payload: {
                id: "remote-space",
                name: "Different Remote Research",
                slug: "different-remote-research",
                updated_at: "2026-07-09T12:05:00.000Z",
              },
            },
          ],
        },
      ),
    );

    expect(duplicateResult).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-space-create-duplicate-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-space-create-duplicate-conflict",
          status: "blocked",
        }),
      ]),
    });
    await expect(dataClient.getSpace("remote-space")).resolves.toMatchObject({
      name: "Remote Research",
      slug: "remote-research",
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider folder updates through the local vault merge executor", async () => {
    const folder = await dataClient.createFolder("local-space", "Local Folder");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-update-1",
            entity_type: "folder",
            entity_id: folder.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: folder.id,
              space_id: "local-space",
              name: "Remote Folder Rename",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-update-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: folder.id,
          name: "Remote Folder Rename",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider folder updates that would create hierarchy cycles", async () => {
    const parent = await dataClient.createFolder("local-space", "Parent Folder");
    const child = await dataClient.createFolder("local-space", "Child Folder", {
      parent_id: parent.id,
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-update-cycle",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-update-cycle-1",
            entity_type: "folder",
            entity_id: parent.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: parent.id,
              space_id: "local-space",
              name: "Parent Folder",
              parent_id: child.id,
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-update-cycle-1",
          status: "blocked",
          message: "Folder cannot be moved inside one of its descendants",
        },
      ],
    });
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: parent.id, parent_id: undefined }),
        expect.objectContaining({ id: child.id, parent_id: parent.id }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider folder deletes through the local vault merge executor", async () => {
    const parent = await dataClient.createFolder("local-space", "Parent Folder");
    const child = await dataClient.createFolder("local-space", "Child Folder", {
      parent_id: parent.id,
    });
    const doc = await dataClient.createDocument("local-space", {
      title: "Nested Folder Doc",
      content: "nested local document",
      folder_id: child.id,
    });
    const property = await dataClient.createPropertySchema("local-space", {
      name: "Folder Delete State",
      property_type: "text",
    });
    await dataClient.setDocumentPropertyValues("local-space", doc.id, [
      { property_id: property.id, value: "remove me" },
    ]);
    const tag = await dataClient.createTag("local-space", {
      name: "Folder Delete Tag",
      color: "#2563eb",
    });
    await dataClient.addDocumentTag("local-space", doc.id, tag.id);
    const workflowTemplate = await dataClient.createWorkflowTemplate("local-space", {
      name: "Folder Delete Workflow",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "done", name: "Done", sort_order: 1 },
      ],
    });
    await dataClient.enableDocumentWorkflow("local-space", doc.id, {
      template_id: workflowTemplate.id,
    });
    await dataClient.transitionDocumentWorkflow("local-space", doc.id, {
      stage_id: "done",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-delete",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-delete-1",
            entity_type: "folder",
            entity_id: parent.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: parent.id,
              space_id: "local-space",
              name: "Parent Folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-delete-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listFolders("local-space")).resolves.not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: parent.id,
        }),
        expect.objectContaining({
          id: child.id,
        }),
      ]),
    );
    await expect(dataClient.getDocumentContent("local-space", doc.id)).rejects.toThrow(
      "Document not found",
    );
    const snapshot = await dataClient.exportLocalVaultSnapshot();
    expect(snapshot.doc_tags?.some((docTag) => docTag.document_id === doc.id)).toBe(false);
    expect(snapshot.property_values?.some((value) => value.document_id === doc.id)).toBe(false);
    expect(
      snapshot.document_workflows?.some((workflow) => workflow.document_id === doc.id),
    ).toBe(false);
    expect(
      snapshot.document_workflow_events?.some((event) => event.document_id === doc.id),
    ).toBe(false);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("skips remote provider folder deletes when the target folder is already absent locally", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-delete-missing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-folder-delete-missing",
            entity_type: "folder",
            entity_id: "missing-folder",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-folder",
              space_id: "local-space",
              name: "Missing Folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-folder-delete-missing",
          status: "skipped",
        },
      ],
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider tag creates through the local vault merge executor", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-tag-create-1",
            entity_type: "tag",
            entity_id: "remote-tag-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-tag-created",
              space_id: "local-space",
              name: "Remote Tag",
              color: "#2563eb",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-tag-create-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listTags("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-tag-created",
          name: "Remote Tag",
          color: "#2563eb",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider tag create duplicates", async () => {
    const tag = await dataClient.createTag("local-space", {
      name: "Existing Local Tag",
      color: "#6b7280",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-create-existing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-tag-create-existing-match",
            entity_type: "tag",
            entity_id: tag.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: tag.id,
              space_id: "local-space",
              name: tag.name,
              color: tag.color,
              created_at: tag.created_at,
            },
          },
          {
            id: "remote-tag-create-existing-conflict",
            entity_type: "tag",
            entity_id: tag.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: tag.id,
              space_id: "local-space",
              name: "Remote Duplicate Tag",
              color: "#2563eb",
              created_at: tag.created_at,
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-tag-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-tag-create-existing-conflict",
          status: "blocked",
        }),
      ]),
    });
    await expect(dataClient.listTags("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: tag.id,
          name: "Existing Local Tag",
          color: "#6b7280",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider tag updates through the local vault merge executor", async () => {
    const tag = await dataClient.createTag("local-space", {
      name: "Local Tag",
      color: "#6b7280",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-update",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-tag-update-1",
            entity_type: "tag",
            entity_id: tag.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: tag.id,
              space_id: "local-space",
              name: "Remote Tag Rename",
              color: "#16a34a",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-tag-update-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listTags("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: tag.id,
          name: "Remote Tag Rename",
          color: "#16a34a",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider tag deletes through the local vault merge executor", async () => {
    const doc = await dataClient.createDocument("local-space", {
      title: "Tagged Local Doc",
      content: "tagged locally",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Local Tag",
      color: "#6b7280",
    });
    await dataClient.addDocumentTag("local-space", doc.id, tag.id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-delete",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-tag-delete-1",
            entity_type: "tag",
            entity_id: tag.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: tag.id,
              space_id: "local-space",
              name: "Local Tag",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-tag-delete-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listTags("local-space")).resolves.not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: tag.id,
        }),
      ]),
    );
    await expect(dataClient.listDocumentTags("local-space", doc.id)).resolves.toEqual([]);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document tag adds through the local vault merge executor", async () => {
    const doc = await dataClient.createDocument("local-space", {
      title: "Remote Tag Target",
      content: "local document",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Remote Linked Tag",
      color: "#2563eb",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-tag-add",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-document-tag-add-1",
            entity_type: "document_tag",
            entity_id: `${doc.id}:${tag.id}`,
            action: "add",
            payload_status: "available",
            payload: {
              document_id: doc.id,
              tag_id: tag.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-document-tag-add-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listDocumentTags("local-space", doc.id)).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: tag.id,
          name: "Remote Linked Tag",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document tag removes through the local vault merge executor", async () => {
    const doc = await dataClient.createDocument("local-space", {
      title: "Remote Untag Target",
      content: "local document",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Remote Removed Tag",
      color: "#2563eb",
    });
    await dataClient.addDocumentTag("local-space", doc.id, tag.id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-tag-remove",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-document-tag-remove-1",
            entity_type: "document_tag",
            entity_id: `${doc.id}:${tag.id}`,
            action: "remove",
            payload_status: "available",
            payload: {
              document_id: doc.id,
              tag_id: tag.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-document-tag-remove-1",
          status: "applied",
        },
      ],
    });
    await expect(dataClient.listDocumentTags("local-space", doc.id)).resolves.toEqual([]);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider document create duplicates", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Existing Local Doc",
      content: "local copy",
    });
    const existingVaultDocument = (await dataClient.exportLocalVaultSnapshot()).documents.find(
      (document) => document.id === created.id,
    );
    if (!existingVaultDocument) {
      throw new Error("Expected document to be present in local vault snapshot");
    }
    await dataClient.clearQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-create-existing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-doc-create-existing-match",
            entity_type: "document",
            entity_id: created.id,
            action: "create",
            payload_status: "available",
            payload: existingVaultDocument,
          },
          {
            id: "remote-doc-create-existing",
            entity_type: "document",
            entity_id: created.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Remote Duplicate",
              content: "remote duplicate",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-doc-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-doc-create-existing",
          status: "blocked",
        }),
      ]),
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      title: "Existing Local Doc",
      content: "local copy",
    });
  });

  it("handles idempotent and blocked remote provider folder create duplicates", async () => {
    const folder = await dataClient.createFolder("local-space", "Existing Local Folder");
    await dataClient.clearQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-create-existing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-folder-create-existing-match",
            entity_type: "folder",
            entity_id: folder.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: folder.id,
              space_id: "local-space",
              name: "Existing Local Folder",
            },
          },
          {
            id: "remote-folder-create-existing-conflict",
            entity_type: "folder",
            entity_id: folder.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: folder.id,
              space_id: "local-space",
              name: "Remote Duplicate Folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-folder-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-folder-create-existing-conflict",
          status: "blocked",
        }),
      ]),
    });
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: folder.id,
          name: "Existing Local Folder",
        }),
      ]),
    );
  });

  it("blocks remote provider folder updates when the target folder is missing locally", async () => {
    const existing = await dataClient.createFolder("local-space", "Existing Local Folder");
    const otherSpace = await dataClient.createSpace("Other Folder Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-folder-update-missing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-folder-update-missing",
            entity_type: "folder",
            entity_id: "missing-folder",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-folder",
              space_id: "local-space",
              name: "Missing Remote Folder",
            },
          },
          {
            id: "remote-folder-update-wrong-space",
            entity_type: "folder",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: otherSpace.id,
              name: "Wrong Space Remote Folder",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-folder-update-missing",
          status: "failed",
        }),
        expect.objectContaining({
          operation_id: "remote-folder-update-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.listFolders("local-space")).resolves.not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "missing-folder",
        }),
      ]),
    );
    await expect(dataClient.listFolders("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: existing.id,
          name: "Existing Local Folder",
        }),
      ]),
    );
    await expect(dataClient.listFolders(otherSpace.id)).resolves.not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: existing.id })]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider tag updates when the target tag is missing locally", async () => {
    const existing = await dataClient.createTag("local-space", {
      name: "Existing Local Tag",
      color: "#6b7280",
    });
    const otherSpace = await dataClient.createSpace("Other Tag Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-update-missing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-tag-update-missing",
            entity_type: "tag",
            entity_id: "missing-tag",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-tag",
              space_id: "local-space",
              name: "Missing Remote Tag",
              color: "#2563eb",
            },
          },
          {
            id: "remote-tag-update-wrong-space",
            entity_type: "tag",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: otherSpace.id,
              name: "Wrong Space Remote Tag",
              color: "#16a34a",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-tag-update-missing",
          status: "failed",
        }),
        expect.objectContaining({
          operation_id: "remote-tag-update-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.listTags("local-space")).resolves.not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "missing-tag",
        }),
      ]),
    );
    await expect(dataClient.listTags("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: existing.id,
          name: "Existing Local Tag",
          color: "#6b7280",
        }),
      ]),
    );
    await expect(dataClient.listTags(otherSpace.id)).resolves.not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: existing.id })]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("skips remote provider tag deletes when the target tag is already absent locally", async () => {
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-tag-delete-missing",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-tag-delete-missing",
            entity_type: "tag",
            entity_id: "missing-tag",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-tag",
              space_id: "local-space",
              name: "Missing Remote Tag",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-tag-delete-missing",
          status: "skipped",
        },
      ],
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider document tag imports", async () => {
    const doc = await dataClient.createDocument("local-space", {
      title: "Remote Tag Edge Cases",
      content: "local document",
    });
    const tag = await dataClient.createTag("local-space", {
      name: "Existing Relationship",
      color: "#2563eb",
    });
    const unassignedTag = await dataClient.createTag("local-space", {
      name: "Unassigned Relationship",
      color: "#16a34a",
    });
    await dataClient.addDocumentTag("local-space", doc.id, tag.id);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-tag-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-document-tag-add-existing",
            entity_type: "document_tag",
            entity_id: `${doc.id}:${tag.id}`,
            action: "add",
            payload_status: "available",
            payload: {
              document_id: doc.id,
              tag_id: tag.id,
              space_id: "local-space",
            },
          },
          {
            id: "remote-document-tag-remove-absent",
            entity_type: "document_tag",
            entity_id: `${doc.id}:${unassignedTag.id}`,
            action: "remove",
            payload_status: "available",
            payload: {
              document_id: doc.id,
              tag_id: unassignedTag.id,
              space_id: "local-space",
            },
          },
          {
            id: "remote-document-tag-add-missing-tag",
            entity_type: "document_tag",
            entity_id: `${doc.id}:missing-tag`,
            action: "add",
            payload_status: "available",
            payload: {
              document_id: doc.id,
              tag_id: "missing-tag",
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 3,
      applied_operation_count: 0,
      skipped_operation_count: 2,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-document-tag-add-existing",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-document-tag-remove-absent",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-document-tag-add-missing-tag",
          status: "failed",
        }),
      ]),
    });
    await expect(dataClient.listDocumentTags("local-space", doc.id)).resolves.toEqual([
      expect.objectContaining({
        id: tag.id,
      }),
    ]);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider template creates, updates, and deletes through the local vault merge executor", async () => {
    const existing = await dataClient.createTemplate("local-space", {
      name: "Local Template",
      content: "# Local",
      tags: ["local"],
    });
    const deleted = await dataClient.createTemplate("local-space", {
      name: "Deleted Template",
      content: "# Delete",
      tags: ["cleanup"],
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-template-crud",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-template-create-1",
            entity_type: "template",
            entity_id: "remote-template-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-template-created",
              space_id: "local-space",
              name: "Remote Template",
              description: "Created remotely",
              content: "# Remote",
              tags: ["remote"],
              created_at: "2026-07-09T12:00:00.000Z",
              updated_at: "2026-07-09T12:01:00.000Z",
            },
          },
          {
            id: "remote-template-update-1",
            entity_type: "template",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Remote Template Rename",
              content: "# Remote Update",
              tags: ["updated"],
              updated_at: "2026-07-09T12:02:00.000Z",
            },
          },
          {
            id: "remote-template-delete-1",
            entity_type: "template",
            entity_id: deleted.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: deleted.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 3,
      applied_operation_count: 3,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    await expect(
      dataClient.getTemplate("local-space", "remote-template-created"),
    ).resolves.toMatchObject({
      name: "Remote Template",
      content: "# Remote",
      tags: ["remote"],
    });
    await expect(dataClient.getTemplate("local-space", existing.id)).resolves.toMatchObject({
      name: "Remote Template Rename",
      content: "# Remote Update",
      tags: ["updated"],
    });
    await expect(dataClient.getTemplate("local-space", deleted.id)).rejects.toThrow(
      "Template not found",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("fails remote provider template imports that violate local template limits", async () => {
    const existing = await dataClient.createTemplate("local-space", {
      name: "Template Limit Target",
      content: "# Local",
    });
    await dataClient.clearQueuedSyncOperations();
    const oversizedContent = "x".repeat(5 * 1024 * 1024 + 1);
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-template-limits",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-template-create-long-name",
            entity_type: "template",
            entity_id: "remote-template-long-name",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-template-long-name",
              space_id: "local-space",
              name: "x".repeat(101),
              content: "# Remote",
            },
          },
          {
            id: "remote-template-update-long-description",
            entity_type: "template",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Template Limit Target",
              description: "x".repeat(1001),
              content: "# Remote Update",
            },
          },
          {
            id: "remote-template-create-oversized-content",
            entity_type: "template",
            entity_id: "remote-template-oversized-content",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-template-oversized-content",
              space_id: "local-space",
              name: "Oversized Template",
              content: oversizedContent,
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      failed_operation_count: 3,
      results: [
        {
          operation_id: "remote-template-create-long-name",
          status: "failed",
          message: "Remote replay operation template:create payload name is too long",
        },
        {
          operation_id: "remote-template-update-long-description",
          status: "failed",
          message: "Remote replay operation template:update payload description is too long",
        },
        {
          operation_id: "remote-template-create-oversized-content",
          status: "failed",
          message: "Remote replay operation template:create payload content exceeds 5 MB limit",
        },
      ],
    });
    await expect(dataClient.getTemplate("local-space", existing.id)).resolves.toMatchObject({
      name: "Template Limit Target",
      content: "# Local",
    });
    await expect(
      dataClient.getTemplate("local-space", "remote-template-long-name"),
    ).rejects.toThrow("Template not found");
    await expect(
      dataClient.getTemplate("local-space", "remote-template-oversized-content"),
    ).rejects.toThrow("Template not found");
  });

  it("handles idempotent and blocked remote provider template imports", async () => {
    const existing = await dataClient.createTemplate("local-space", {
      name: "Existing Template",
      content: "# Existing",
      tags: ["existing"],
    });
    const existingVaultTemplate = (await dataClient.exportLocalVaultSnapshot()).templates.find(
      (template) => template.id === existing.id,
    );
    expect(existingVaultTemplate).toBeDefined();
    const otherSpace = await dataClient.createSpace("Other Template Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-template-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-template-create-existing-match",
            entity_type: "template",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: existingVaultTemplate,
          },
          {
            id: "remote-template-create-existing",
            entity_type: "template",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Duplicate Template",
              content: "# Duplicate",
            },
          },
          {
            id: "remote-template-update-missing",
            entity_type: "template",
            entity_id: "missing-template",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-template",
              space_id: "local-space",
              name: "Missing Template",
              content: "# Missing",
            },
          },
          {
            id: "remote-template-update-wrong-space",
            entity_type: "template",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: otherSpace.id,
              name: "Wrong Space Template",
              content: "# Wrong Space",
            },
          },
          {
            id: "remote-template-delete-missing",
            entity_type: "template",
            entity_id: "missing-delete-template",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-delete-template",
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-template-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-template-create-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-template-update-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-template-update-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-template-delete-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.getTemplate("local-space", existing.id)).resolves.toMatchObject({
      name: "Existing Template",
      content: "# Existing",
    });
    await expect(dataClient.getTemplate(otherSpace.id, existing.id)).rejects.toThrow(
      "Template not found",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider workflow template create, update, and delete imports", async () => {
    const existing = await dataClient.createWorkflowTemplate("local-space", {
      name: "Existing Flow",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [{ approver_type: "role", approver_id: "owner", stage_id: "review" }],
    });
    const deleted = await dataClient.createWorkflowTemplate("local-space", {
      name: "Deleted Flow",
      stages: [
        { id: "open", name: "Open", sort_order: 0 },
        { id: "closed", name: "Closed", sort_order: 1 },
      ],
      approvers: [],
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-workflow-template-crud",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 3,
        operations: [
          {
            id: "remote-workflow-template-create-1",
            entity_type: "workflow_template",
            entity_id: "remote-workflow-template-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-workflow-template-created",
              space_id: "local-space",
              name: "Remote Flow",
              description: "Created remotely",
              stages: [
                { id: "intake", name: "Intake", sort_order: 0 },
                { id: "approve", name: "Approve", sort_order: 1 },
              ],
              approvers: [{ approver_type: "role", approver_id: "admin", stage_id: "approve" }],
              created_at: "2026-07-09T12:00:00.000Z",
              updated_at: "2026-07-09T12:01:00.000Z",
            },
          },
          {
            id: "remote-workflow-template-update-1",
            entity_type: "workflow_template",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Remote Flow Rename",
              stages: [
                { id: "draft", name: "Draft", sort_order: 0 },
                { id: "legal", name: "Legal", sort_order: 1 },
                { id: "published", name: "Published", sort_order: 2 },
              ],
              approvers: [{ approver_type: "role", approver_id: "legal", stage_id: "legal" }],
              updated_at: "2026-07-09T12:02:00.000Z",
            },
          },
          {
            id: "remote-workflow-template-delete-1",
            entity_type: "workflow_template",
            entity_id: deleted.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: deleted.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 3,
      applied_operation_count: 3,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    await expect(
      dataClient.getWorkflowTemplate("local-space", "remote-workflow-template-created"),
    ).resolves.toMatchObject({
      name: "Remote Flow",
      stages: [
        { id: "intake", name: "Intake", sort_order: 0 },
        { id: "approve", name: "Approve", sort_order: 1 },
      ],
      approvers: [expect.objectContaining({ approver_id: "admin", stage_id: "approve" })],
    });
    await expect(dataClient.getWorkflowTemplate("local-space", existing.id)).resolves.toMatchObject(
      {
        name: "Remote Flow Rename",
        stages: expect.arrayContaining([expect.objectContaining({ id: "legal" })]),
        approvers: [expect.objectContaining({ approver_id: "legal", stage_id: "legal" })],
      },
    );
    await expect(dataClient.getWorkflowTemplate("local-space", deleted.id)).rejects.toThrow(
      "Workflow template not found",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider workflow template imports", async () => {
    const existing = await dataClient.createWorkflowTemplate("local-space", {
      name: "Existing Flow",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [],
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    const existingVaultWorkflowTemplate = (exported.workflow_templates ?? []).find(
      (template) => template.id === existing.id,
    );
    if (!existingVaultWorkflowTemplate) {
      throw new Error("Expected workflow template to be present in local vault snapshot");
    }
    const otherSpace = await dataClient.createSpace("Other Workflow Template Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-workflow-template-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-workflow-template-create-existing-match",
            entity_type: "workflow_template",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: existingVaultWorkflowTemplate,
          },
          {
            id: "remote-workflow-template-create-existing",
            entity_type: "workflow_template",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Duplicate Flow",
              stages: [
                { id: "draft", name: "Draft", sort_order: 0 },
                { id: "review", name: "Review", sort_order: 1 },
              ],
              approvers: [],
            },
          },
          {
            id: "remote-workflow-template-update-missing",
            entity_type: "workflow_template",
            entity_id: "missing-workflow-template",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-workflow-template",
              space_id: "local-space",
              name: "Missing Flow",
              stages: [
                { id: "draft", name: "Draft", sort_order: 0 },
                { id: "review", name: "Review", sort_order: 1 },
              ],
              approvers: [],
            },
          },
          {
            id: "remote-workflow-template-update-wrong-space",
            entity_type: "workflow_template",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: otherSpace.id,
              name: "Wrong Space Flow",
              stages: [
                { id: "draft", name: "Draft", sort_order: 0 },
                { id: "review", name: "Review", sort_order: 1 },
              ],
              approvers: [],
            },
          },
          {
            id: "remote-workflow-template-delete-missing",
            entity_type: "workflow_template",
            entity_id: "missing-delete-workflow-template",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-delete-workflow-template",
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-workflow-template-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-workflow-template-create-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-workflow-template-update-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-workflow-template-update-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-workflow-template-delete-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.getWorkflowTemplate("local-space", existing.id)).resolves.toMatchObject(
      {
        name: "Existing Flow",
      },
    );
    await expect(dataClient.getWorkflowTemplate(otherSpace.id, existing.id)).rejects.toThrow(
      "Workflow template not found",
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider property schema create, update, reorder, and delete imports", async () => {
    const updated = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "text",
    });
    const deleted = await dataClient.createPropertySchema("local-space", {
      name: "Deprecated",
      property_type: "text",
    });
    const first = await dataClient.createPropertySchema("local-space", {
      name: "First",
      property_type: "text",
    });
    const second = await dataClient.createPropertySchema("local-space", {
      name: "Second",
      property_type: "text",
    });
    await dataClient.setDocumentPropertyValues("local-space", "local-welcome", [
      { property_id: deleted.id, value: "remove me" },
    ]);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-property-schema-crud",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 4,
        operations: [
          {
            id: "remote-property-schema-create-1",
            entity_type: "property_schema",
            entity_id: "remote-property-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-property-created",
              space_id: "local-space",
              name: "Remote Status",
              property_type: "select",
              options: { values: ["Draft", "Published"] },
              sort_order: 12,
            },
          },
          {
            id: "remote-property-schema-update-1",
            entity_type: "property_schema",
            entity_id: updated.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: updated.id,
              space_id: "local-space",
              name: "Remote State",
              property_type: "url",
              options: { placeholder: "https://example.test" },
            },
          },
          {
            id: "remote-property-schema-reorder-1",
            entity_type: "property_schema",
            entity_id: "local-space",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "local-space",
              property_ids: [second.id, first.id],
            },
          },
          {
            id: "remote-property-schema-delete-1",
            entity_type: "property_schema",
            entity_id: deleted.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: deleted.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 4,
      applied_operation_count: 4,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    const schemas = await dataClient.listPropertySchemas("local-space");
    expect(schemas).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-property-created",
          name: "Remote Status",
          property_type: "select",
          sort_order: 12,
        }),
        expect.objectContaining({
          id: updated.id,
          name: "Remote State",
          property_type: "url",
        }),
        expect.objectContaining({ id: second.id, sort_order: 1 }),
        expect.objectContaining({ id: first.id, sort_order: 2 }),
      ]),
    );
    expect(schemas.some((schema) => schema.id === deleted.id)).toBe(false);
    const values = await dataClient.listDocumentProperties("local-space", "local-welcome");
    expect(values.some((value) => value.property_id === deleted.id)).toBe(false);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider property schema imports", async () => {
    const existing = await dataClient.createPropertySchema("local-space", {
      name: "Existing Status",
      property_type: "text",
    });
    const reorderDuplicate = await dataClient.createPropertySchema("local-space", {
      name: "Duplicate Order Status",
      property_type: "text",
    });
    const otherSpace = await dataClient.createSpace("Other Property Space");
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-property-schema-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 7,
        operations: [
          {
            id: "remote-property-schema-create-existing-match",
            entity_type: "property_schema",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: existing.name,
              property_type: existing.property_type,
              options: existing.options,
              sort_order: existing.sort_order,
            },
          },
          {
            id: "remote-property-schema-create-existing",
            entity_type: "property_schema",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              name: "Duplicate Status",
              property_type: "text",
            },
          },
          {
            id: "remote-property-schema-update-missing",
            entity_type: "property_schema",
            entity_id: "missing-property-schema",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-property-schema",
              space_id: "local-space",
              name: "Missing Status",
              property_type: "text",
            },
          },
          {
            id: "remote-property-schema-update-wrong-space",
            entity_type: "property_schema",
            entity_id: existing.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: otherSpace.id,
              name: "Wrong Space Status",
              property_type: "select",
              options: { values: ["Wrong"] },
            },
          },
          {
            id: "remote-property-schema-reorder-missing",
            entity_type: "property_schema",
            entity_id: "local-space",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "local-space",
              property_ids: ["missing-property-schema"],
            },
          },
          {
            id: "remote-property-schema-reorder-duplicate",
            entity_type: "property_schema",
            entity_id: "local-space",
            action: "reorder",
            payload_status: "available",
            payload: {
              space_id: "local-space",
              property_ids: [reorderDuplicate.id, reorderDuplicate.id],
            },
          },
          {
            id: "remote-property-schema-delete-missing",
            entity_type: "property_schema",
            entity_id: "missing-delete-property-schema",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-delete-property-schema",
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 2,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-property-schema-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-create-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-update-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-update-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-reorder-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-reorder-duplicate",
          status: "skipped",
          message:
            "Remote replay operation property_schema:reorder payload includes duplicate property IDs",
        }),
        expect.objectContaining({
          operation_id: "remote-property-schema-delete-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.listPropertySchemas("local-space")).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: existing.id, name: "Existing Status" }),
        expect.objectContaining({
          id: reorderDuplicate.id,
          sort_order: reorderDuplicate.sort_order,
        }),
      ]),
    );
    await expect(dataClient.listPropertySchemas(otherSpace.id)).resolves.not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: existing.id })]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document property value set imports", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Properties",
      content: "local draft",
    });
    const status = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "text",
    });
    const owner = await dataClient.createPropertySchema("local-space", {
      name: "Owner",
      property_type: "user",
    });
    await dataClient.setDocumentPropertyValues("local-space", created.id, [
      {
        source_id: "local-status-value",
        property_id: status.id,
        value: "Local",
        created_at: "2026-07-09T11:00:00.000Z",
      },
    ]);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-property-values-set",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-document-property-values-set-1",
            entity_type: "document_property_values",
            entity_id: created.id,
            action: "set",
            payload_status: "available",
            payload: {
              document_id: created.id,
              space_id: "local-space",
              values: [
                {
                  id: "remote-status-value",
                  property_id: status.id,
                  value: "Remote",
                  created_at: "2026-07-09T12:00:00.000Z",
                  updated_at: "2026-07-09T12:01:00.000Z",
                },
                {
                  id: "remote-owner-value",
                  property_id: owner.id,
                  value: "local-user",
                  created_at: "2026-07-09T12:00:00.000Z",
                },
              ],
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    await expect(dataClient.listDocumentProperties("local-space", created.id)).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ property_id: status.id, value: "Remote" }),
        expect.objectContaining({ property_id: owner.id, value: "local-user" }),
      ]),
    );
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.property_values).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-status-value",
          document_id: created.id,
          property_id: status.id,
          value: "Remote",
        }),
        expect.objectContaining({
          id: "remote-owner-value",
          document_id: created.id,
          property_id: owner.id,
          value: "local-user",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("fails remote provider document property value imports with missing prerequisites", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Property Prereqs",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-property-values-prereqs",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-document-property-values-missing-doc",
            entity_type: "document_property_values",
            entity_id: "missing-document",
            action: "set",
            payload_status: "available",
            payload: {
              document_id: "missing-document",
              space_id: "local-space",
              values: [],
            },
          },
          {
            id: "remote-document-property-values-missing-schema",
            entity_type: "document_property_values",
            entity_id: created.id,
            action: "set",
            payload_status: "available",
            payload: {
              document_id: created.id,
              space_id: "local-space",
              values: [{ property_id: "missing-property", value: "Remote" }],
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-document-property-values-missing-doc",
          status: "failed",
        }),
        expect.objectContaining({
          operation_id: "remote-document-property-values-missing-schema",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(dataClient.listDocumentProperties("local-space", created.id)).resolves.toEqual([]);
    expect(await dataClient.getSyncState()).toBe("conflict");
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("rejects remote provider document property value ID conflicts and duplicates", async () => {
    const conflictDoc = await dataClient.createDocument("local-space", {
      title: "Remote Property Conflict",
      content: "local draft",
    });
    const duplicateDoc = await dataClient.createDocument("local-space", {
      title: "Remote Property Duplicate",
      content: "local draft",
    });
    const ownerDoc = await dataClient.createDocument("local-space", {
      title: "Remote Property Existing Owner",
      content: "local draft",
    });
    const status = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "text",
    });
    await dataClient.setDocumentPropertyValues("local-space", ownerDoc.id, [
      {
        source_id: "existing-property-value-id",
        property_id: status.id,
        value: "Existing",
      },
    ]);
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-property-values-integrity",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-document-property-values-id-conflict",
            entity_type: "document_property_values",
            entity_id: conflictDoc.id,
            action: "set",
            payload_status: "available",
            payload: {
              document_id: conflictDoc.id,
              space_id: "local-space",
              values: [
                {
                  id: "existing-property-value-id",
                  property_id: status.id,
                  value: "Remote",
                },
              ],
            },
          },
          {
            id: "remote-document-property-values-duplicate-property",
            entity_type: "document_property_values",
            entity_id: duplicateDoc.id,
            action: "set",
            payload_status: "available",
            payload: {
              document_id: duplicateDoc.id,
              space_id: "local-space",
              values: [
                { property_id: status.id, value: "First" },
                { property_id: status.id, value: "Second" },
              ],
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-document-property-values-id-conflict",
          status: "failed",
          message:
            "Remote document property values payload ID conflicts with another local property value",
        }),
        expect.objectContaining({
          operation_id: "remote-document-property-values-duplicate-property",
          status: "skipped",
          message:
            "Remote replay operation document_property_values:set payload includes duplicate property values",
        }),
      ]),
    });
    await expect(dataClient.listDocumentProperties("local-space", conflictDoc.id)).resolves.toEqual(
      expect.arrayContaining([expect.objectContaining({ property_id: status.id, value: null })]),
    );
    await expect(dataClient.listDocumentProperties("local-space", duplicateDoc.id)).resolves.toEqual(
      expect.arrayContaining([expect.objectContaining({ property_id: status.id, value: null })]),
    );
    await expect(dataClient.listDocumentProperties("local-space", ownerDoc.id)).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ property_id: status.id, value: "Existing" }),
      ]),
    );
    expect(await dataClient.getSyncState()).toBe("error");
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider browser asset create imports", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Asset",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-asset-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-asset-create-1",
            entity_type: "asset",
            entity_id: "remote-asset-1",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-asset-1",
              space_id: "local-space",
              document_id: created.id,
              name: "remote.txt",
              mime_type: "text/plain",
              size: 5,
              url: "data:text/plain;base64,SGVsbG8=",
              source: "browser",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.assets).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-asset-1",
          name: "remote.txt",
          mime_type: "text/plain",
          size: 5,
          url: "data:text/plain;base64,SGVsbG8=",
          source: "browser",
          space_id: "local-space",
          document_id: created.id,
          created_at: "2026-07-09T12:00:00.000Z",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles duplicate and missing-byte remote provider asset imports", async () => {
    const existing = await dataClient.saveBrowserAsset({
      name: "existing.txt",
      mime_type: "text/plain",
      size: 5,
      url: "data:text/plain;base64,SGVsbG8=",
      space_id: "local-space",
      document_id: "local-welcome",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-asset-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-asset-create-existing-match",
            entity_type: "asset",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              document_id: "local-welcome",
              name: existing.name,
              mime_type: existing.mime_type,
              size: existing.size,
              url: existing.url,
              source: "browser",
              created_at: existing.created_at,
            },
          },
          {
            id: "remote-asset-create-existing",
            entity_type: "asset",
            entity_id: existing.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existing.id,
              space_id: "local-space",
              document_id: "local-welcome",
              name: "duplicate.txt",
              size: 5,
              url: "data:text/plain;base64,SGVsbG8=",
              source: "browser",
            },
          },
          {
            id: "remote-asset-create-missing-bytes",
            entity_type: "asset",
            entity_id: "remote-asset-missing-bytes",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-asset-missing-bytes",
              space_id: "local-space",
              document_id: "local-welcome",
              name: "missing.txt",
              size: 5,
              url: "asset://localhost/missing.txt",
              source: "tauri",
            },
          },
          {
            id: "remote-asset-create-missing-doc",
            entity_type: "asset",
            entity_id: "remote-asset-missing-doc",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-asset-missing-doc",
              space_id: "local-space",
              document_id: "missing-document",
              name: "missing-doc.txt",
              size: 5,
              url: "data:text/plain;base64,SGVsbG8=",
              source: "browser",
            },
          },
          {
            id: "remote-asset-create-size-mismatch",
            entity_type: "asset",
            entity_id: "remote-asset-size-mismatch",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-asset-size-mismatch",
              space_id: "local-space",
              document_id: "local-welcome",
              name: "wrong-size.txt",
              size: 99,
              url: "data:text/plain;base64,SGVsbG8=",
              source: "browser",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 3,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-asset-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-asset-create-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-asset-create-missing-bytes",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-asset-create-missing-doc",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-asset-create-size-mismatch",
          status: "skipped",
          message:
            "Remote replay operation asset:create payload byte size does not match declared size",
        }),
      ]),
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.assets.filter((asset) => asset.id === existing.id)).toHaveLength(1);
    expect(exported.assets.some((asset) => asset.id === "remote-asset-missing-bytes")).toBe(false);
    expect(exported.assets.some((asset) => asset.id === "remote-asset-missing-doc")).toBe(false);
    expect(exported.assets.some((asset) => asset.id === "remote-asset-size-mismatch")).toBe(false);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider asset deletes through the local vault merge executor", async () => {
    const existing = await dataClient.saveBrowserAsset({
      name: "remove.txt",
      mime_type: "text/plain",
      size: 6,
      url: "data:text/plain;base64,UmVtb3Zl",
      space_id: "local-space",
      document_id: "local-welcome",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-asset-delete",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 2,
        operations: [
          {
            id: "remote-asset-delete-existing",
            entity_type: "asset",
            entity_id: existing.id,
            action: "delete",
            payload_status: "available",
          },
          {
            id: "remote-asset-delete-missing",
            entity_type: "asset",
            entity_id: "missing-asset",
            action: "delete",
            payload_status: "available",
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 2,
      applied_operation_count: 1,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      results: [
        {
          operation_id: "remote-asset-delete-existing",
          status: "applied",
        },
        {
          operation_id: "remote-asset-delete-missing",
          status: "skipped",
        },
      ],
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.assets.some((asset) => asset.id === existing.id)).toBe(false);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks remote provider asset deletes when a local queued operation targets the same asset", async () => {
    const existing = await dataClient.saveBrowserAsset({
      name: "pending-local-asset.txt",
      mime_type: "text/plain",
      size: 7,
      url: "data:text/plain;base64,UGVuZGluZw==",
      space_id: "local-space",
      document_id: "local-welcome",
    });
    const queuedOperations = await dataClient.listQueuedSyncOperations();
    expect(
      queuedOperations.some(
        (operation) =>
          operation.entity_type === "asset" && operation.entity_id === existing.id,
      ),
    ).toBe(true);
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-asset-delete-conflict",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-asset-delete-conflict",
            entity_type: "asset",
            entity_id: existing.id,
            action: "delete",
            payload_status: "available",
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-asset-delete-conflict",
          status: "blocked",
          message: "A local queued operation targets the same entity",
        },
      ],
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.assets.some((asset) => asset.id === existing.id)).toBe(true);
    expect(await dataClient.getSyncState()).toBe("conflict");
  });

  it("rejects remote provider asset delete tombstones with mismatched document context", async () => {
    const otherSpace = await dataClient.createSpace("Other Space");
    const existing = await dataClient.saveBrowserAsset({
      name: "mismatched-context.txt",
      mime_type: "text/plain",
      size: 7,
      url: "data:text/plain;base64,Q29udGV4dA==",
      space_id: "local-space",
      document_id: "local-welcome",
    });
    await dataClient.clearQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-asset-delete-context",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-asset-delete-context",
            entity_type: "asset",
            entity_id: existing.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: existing.id,
              document_id: "local-welcome",
              space_id: otherSpace.id,
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 1,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 1,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-asset-delete-context",
          status: "failed",
          message: "Remote asset delete payload space does not match local document",
        },
      ],
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.assets.some((asset) => asset.id === existing.id)).toBe(true);
    expect(await dataClient.getSyncState()).toBe("error");
  });

  it("applies remote provider document workflow create, update, transition, and delete imports", async () => {
    const template = await dataClient.createWorkflowTemplate("local-space", {
      name: "Remote Workflow",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
        { id: "done", name: "Done", sort_order: 2 },
      ],
      approvers: [{ approver_type: "role", approver_id: "admin", stage_id: "review" }],
    });
    const createdDoc = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Create",
    });
    const updatedDoc = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Update",
    });
    const transitionedDoc = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Transition",
    });
    const deletedDoc = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Delete",
    });
    const updatedWorkflow = await dataClient.enableDocumentWorkflow("local-space", updatedDoc.id, {
      template_id: template.id,
      stage_id: "draft",
    });
    const transitionedWorkflow = await dataClient.enableDocumentWorkflow(
      "local-space",
      transitionedDoc.id,
      { template_id: template.id, stage_id: "draft" },
    );
    const deletedWorkflow = await dataClient.enableDocumentWorkflow("local-space", deletedDoc.id, {
      template_id: template.id,
      stage_id: "draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-workflow-crud",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 4,
        operations: [
          {
            id: "remote-document-workflow-create-1",
            entity_type: "document_workflow",
            entity_id: "remote-workflow-created",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-workflow-created",
              document_id: createdDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "draft",
              status: "active",
              created_by: "remote-user",
              created_at: "2026-07-09T12:00:00.000Z",
              updated_at: "2026-07-09T12:01:00.000Z",
            },
          },
          {
            id: "remote-document-workflow-update-1",
            entity_type: "document_workflow",
            entity_id: updatedWorkflow.id,
            action: "update",
            payload_status: "available",
            payload: {
              id: updatedWorkflow.id,
              document_id: updatedDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "review",
              status: "active",
              updated_at: "2026-07-09T12:02:00.000Z",
            },
          },
          {
            id: "remote-document-workflow-transition-1",
            entity_type: "document_workflow",
            entity_id: transitionedWorkflow.id,
            action: "transition",
            payload_status: "available",
            payload: {
              id: transitionedWorkflow.id,
              document_id: transitionedDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "done",
              status: "completed",
              updated_at: "2026-07-09T12:03:00.000Z",
            },
          },
          {
            id: "remote-document-workflow-delete-1",
            entity_type: "document_workflow",
            entity_id: deletedWorkflow.id,
            action: "delete",
            payload_status: "available",
            payload: {
              id: deletedWorkflow.id,
              document_id: deletedDoc.id,
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 4,
      applied_operation_count: 4,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    await expect(
      dataClient.getDocumentWorkflow("local-space", createdDoc.id),
    ).resolves.toMatchObject({
      id: "remote-workflow-created",
      current_stage_id: "draft",
      status: "active",
    });
    await expect(
      dataClient.getDocumentWorkflow("local-space", updatedDoc.id),
    ).resolves.toMatchObject({
      current_stage_id: "review",
      status: "active",
    });
    await expect(
      dataClient.getDocumentWorkflow("local-space", transitionedDoc.id),
    ).resolves.toMatchObject({
      current_stage_id: "done",
      status: "completed",
    });
    await expect(dataClient.getDocumentWorkflow("local-space", deletedDoc.id)).resolves.toBeNull();
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and blocked remote provider document workflow imports", async () => {
    const template = await dataClient.createWorkflowTemplate("local-space", {
      name: "Existing Workflow",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [],
    });
    const existingDoc = await dataClient.createDocument("local-space", {
      title: "Existing Workflow",
    });
    const existingWorkflow = await dataClient.enableDocumentWorkflow(
      "local-space",
      existingDoc.id,
      {
        template_id: template.id,
        stage_id: "draft",
      },
    );
    const exported = await dataClient.exportLocalVaultSnapshot();
    const existingVaultWorkflow = (exported.document_workflows ?? []).find(
      (workflow) => workflow.id === existingWorkflow.id,
    );
    if (!existingVaultWorkflow) {
      throw new Error("Expected document workflow to be present in local vault snapshot");
    }
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-workflow-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 7,
        operations: [
          {
            id: "remote-document-workflow-create-existing-match",
            entity_type: "document_workflow",
            entity_id: existingWorkflow.id,
            action: "create",
            payload_status: "available",
            payload: {
              ...existingVaultWorkflow,
              space_id: "local-space",
            },
          },
          {
            id: "remote-document-workflow-create-existing",
            entity_type: "document_workflow",
            entity_id: existingWorkflow.id,
            action: "create",
            payload_status: "available",
            payload: {
              id: existingWorkflow.id,
              document_id: existingDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "review",
              status: "completed",
            },
          },
          {
            id: "remote-document-workflow-create-existing-document",
            entity_type: "document_workflow",
            entity_id: "remote-workflow-different-id",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-workflow-different-id",
              document_id: existingDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "draft",
              status: "active",
            },
          },
          {
            id: "remote-document-workflow-update-missing",
            entity_type: "document_workflow",
            entity_id: "missing-workflow",
            action: "update",
            payload_status: "available",
            payload: {
              id: "missing-workflow",
              document_id: "missing-document",
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "draft",
              status: "active",
            },
          },
          {
            id: "remote-document-workflow-update-different-id",
            entity_type: "document_workflow",
            entity_id: "remote-workflow-update-different-id",
            action: "update",
            payload_status: "available",
            payload: {
              id: "remote-workflow-update-different-id",
              document_id: existingDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "review",
              status: "active",
            },
          },
          {
            id: "remote-document-workflow-transition-missing-stage",
            entity_type: "document_workflow",
            entity_id: existingWorkflow.id,
            action: "transition",
            payload_status: "available",
            payload: {
              id: existingWorkflow.id,
              document_id: existingDoc.id,
              space_id: "local-space",
              workflow_template_id: template.id,
              current_stage_id: "missing-stage",
              status: "active",
            },
          },
          {
            id: "remote-document-workflow-delete-missing",
            entity_type: "document_workflow",
            entity_id: "missing-delete-workflow",
            action: "delete",
            payload_status: "available",
            payload: {
              id: "missing-delete-workflow",
              document_id: "missing-delete-document",
              space_id: "local-space",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 1,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-document-workflow-create-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-create-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-create-existing-document",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-update-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-update-different-id",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-transition-missing-stage",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-delete-missing",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
      ]),
    });
    await expect(
      dataClient.getDocumentWorkflow("local-space", existingDoc.id),
    ).resolves.toMatchObject({
      id: existingWorkflow.id,
      current_stage_id: "draft",
    });
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("applies remote provider document workflow event imports", async () => {
    const template = await dataClient.createWorkflowTemplate("local-space", {
      name: "Remote Workflow Events",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [],
    });
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Event",
      content: "local draft",
    });
    const workflow = await dataClient.enableDocumentWorkflow("local-space", created.id, {
      template_id: template.id,
      stage_id: "draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-workflow-event-delegate",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-document-workflow-event-delegate-1",
            entity_type: "document_workflow_event",
            entity_id: "remote-workflow-event-1",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "remote-workflow-event-1",
              document_workflow_id: workflow.id,
              document_id: created.id,
              space_id: "local-space",
              from_stage_id: "draft",
              to_stage_id: "review",
              action: "delegate",
              note: "Remote handoff",
              created_by: "remote-user",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "completed",
      applied: true,
      attempted_operation_count: 1,
      applied_operation_count: 1,
      skipped_operation_count: 0,
      failed_operation_count: 0,
    });
    await expect(dataClient.listDocumentWorkflowEvents("local-space", created.id)).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "remote-workflow-event-1",
          document_workflow_id: workflow.id,
          action: "delegate",
          note: "Remote handoff",
          created_by: "remote-user",
        }),
      ]),
    );
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("handles idempotent and invalid remote provider document workflow event imports", async () => {
    const template = await dataClient.createWorkflowTemplate("local-space", {
      name: "Remote Workflow Event Edges",
      stages: [
        { id: "draft", name: "Draft", sort_order: 0 },
        { id: "review", name: "Review", sort_order: 1 },
      ],
      approvers: [],
    });
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Workflow Event Edges",
      content: "local draft",
    });
    const workflow = await dataClient.enableDocumentWorkflow("local-space", created.id, {
      template_id: template.id,
      stage_id: "draft",
    });
    const otherSpace = await dataClient.createSpace("Other Workflow Event Space");
    await dataClient.recordDocumentWorkflowEvent("local-space", created.id, {
      action: "delegate",
      note: "Existing event",
      source_id: "existing-workflow-event",
      to_stage_id: "review",
    });
    const exported = await dataClient.exportLocalVaultSnapshot();
    const existingVaultEvent = (exported.document_workflow_events ?? []).find(
      (event) => event.id === "existing-workflow-event",
    );
    if (!existingVaultEvent) {
      throw new Error("Expected workflow event to be present in local vault snapshot");
    }
    await dataClient.clearQueuedSyncOperations();
    const operationsBefore = await dataClient.listQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-document-workflow-event-edge-cases",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 5,
        operations: [
          {
            id: "remote-document-workflow-event-existing-match",
            entity_type: "document_workflow_event",
            entity_id: "existing-workflow-event",
            action: "delegate",
            payload_status: "available",
            payload: {
              ...existingVaultEvent,
              space_id: "local-space",
            },
          },
          {
            id: "remote-document-workflow-event-existing",
            entity_type: "document_workflow_event",
            entity_id: "existing-workflow-event",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "existing-workflow-event",
              document_workflow_id: workflow.id,
              document_id: created.id,
              space_id: "local-space",
              to_stage_id: "review",
              action: "delegate",
              note: "Duplicate event",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
          {
            id: "remote-document-workflow-event-existing-wrong-space",
            entity_type: "document_workflow_event",
            entity_id: "existing-workflow-event",
            action: "delegate",
            payload_status: "available",
            payload: {
              ...existingVaultEvent,
              space_id: otherSpace.id,
            },
          },
          {
            id: "remote-document-workflow-event-missing-workflow",
            entity_type: "document_workflow_event",
            entity_id: "missing-workflow-event",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "missing-workflow-event",
              document_workflow_id: "missing-workflow",
              document_id: created.id,
              space_id: "local-space",
              to_stage_id: "review",
              action: "delegate",
              note: "Missing workflow",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
          {
            id: "remote-document-workflow-event-bad-action",
            entity_type: "document_workflow_event",
            entity_id: "bad-action-workflow-event",
            action: "delegate",
            payload_status: "available",
            payload: {
              id: "bad-action-workflow-event",
              document_workflow_id: workflow.id,
              document_id: created.id,
              space_id: "local-space",
              to_stage_id: "review",
              action: "escalate",
              note: "Bad action",
              created_at: "2026-07-09T12:00:00.000Z",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 2,
      applied_operation_count: 0,
      skipped_operation_count: 2,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: expect.arrayContaining([
        expect.objectContaining({
          operation_id: "remote-document-workflow-event-existing-match",
          status: "skipped",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-event-existing-wrong-space",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-event-existing",
          status: "blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-event-missing-workflow",
          status: "blocked",
          message:
            "Remote replay operation was not applied because an earlier selected operation failed or was blocked",
        }),
        expect.objectContaining({
          operation_id: "remote-document-workflow-event-bad-action",
          status: "skipped",
          message:
            "Remote replay operation document_workflow_event:delegate payload action does not match operation action",
        }),
      ]),
    });
    const events = await dataClient.listDocumentWorkflowEvents("local-space", created.id);
    expect(events.filter((event) => event.id === "existing-workflow-event")).toHaveLength(1);
    expect(events.some((event) => event.id === "missing-workflow-event")).toBe(false);
    expect(events.some((event) => event.id === "bad-action-workflow-event")).toBe(false);
    expect(await dataClient.listQueuedSyncOperations()).toHaveLength(operationsBefore.length);
  });

  it("blocks server-only remote provider import operations without mutating local data", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Unsupported Remote Merge",
      content: "local draft",
    });
    await dataClient.clearQueuedSyncOperations();
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-comment-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 9,
        operations: [
          {
            id: "remote-comment-create-1",
            entity_type: "comment",
            entity_id: "remote-comment-1",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-comment-1",
              document_id: created.id,
              body: "Unsupported remote comment",
            },
          },
          {
            id: "remote-document-publish-1",
            entity_type: "document",
            entity_id: created.id,
            action: "publish",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              public_slug: "remote-public-slug",
            },
          },
          {
            id: "remote-document-unpublish-1",
            entity_type: "document",
            entity_id: created.id,
            action: "unpublish",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
            },
          },
          {
            id: "remote-document-reaction-toggle-1",
            entity_type: "document_reaction",
            entity_id: `${created.id}:thumbs-up`,
            action: "toggle",
            payload_status: "available",
            payload: {
              document_id: created.id,
              space_id: "local-space",
              emoji: "thumbs-up",
            },
          },
          {
            id: "remote-document-version-restore-1",
            entity_type: "document_version",
            entity_id: "remote-version-1",
            action: "restore",
            payload_status: "available",
            payload: {
              id: "remote-version-1",
              document_id: created.id,
              space_id: "local-space",
            },
          },
          {
            id: "remote-webhook-create-1",
            entity_type: "webhook",
            entity_id: "remote-webhook-1",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-webhook-1",
              url: "https://example.com/webhook",
            },
          },
          {
            id: "remote-notification-read-1",
            entity_type: "notification",
            entity_id: "remote-notification-1",
            action: "read",
            payload_status: "available",
            payload: {
              id: "remote-notification-1",
              read: true,
            },
          },
          {
            id: "remote-api-key-create-1",
            entity_type: "api_key",
            entity_id: "remote-api-key-1",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-api-key-1",
              name: "Remote key",
            },
          },
          {
            id: "remote-space-member-add-1",
            entity_type: "space_member",
            entity_id: "local-space:remote-user-1",
            action: "add",
            payload_status: "available",
            payload: {
              space_id: "local-space",
              user_id: "remote-user-1",
              role: "viewer",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-comment-create-1",
          status: "blocked",
          message: "Remote replay operation comment:create is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-document-publish-1",
          status: "blocked",
          message:
            "Remote replay operation document:publish is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-document-unpublish-1",
          status: "blocked",
          message:
            "Remote replay operation document:unpublish is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-document-reaction-toggle-1",
          status: "blocked",
          message:
            "Remote replay operation document_reaction:toggle is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-document-version-restore-1",
          status: "blocked",
          message:
            "Remote replay operation document_version:restore is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-webhook-create-1",
          status: "blocked",
          message:
            "Remote replay operation webhook:create is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-notification-read-1",
          status: "blocked",
          message:
            "Remote replay operation notification:read is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-api-key-create-1",
          status: "blocked",
          message:
            "Remote replay operation api_key:create is server-only and cannot be imported into a local vault",
        },
        {
          operation_id: "remote-space-member-add-1",
          status: "blocked",
          message:
            "Remote replay operation space_member:add is server-only and cannot be imported into a local vault",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local draft",
    });
    expect(await dataClient.getSyncState()).toBe("conflict");
  });

  it("blocks oversized remote provider document creates without mutating local data", async () => {
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-oversized-document-create",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-document-create-oversized",
            entity_type: "document",
            entity_id: "remote-oversized-doc",
            action: "create",
            payload_status: "available",
            payload: {
              id: "remote-oversized-doc",
              space_id: "local-space",
              title: "Oversized Remote Doc",
              content: "x".repeat(5 * 1024 * 1024 + 1),
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 1,
      results: [
        {
          operation_id: "remote-document-create-oversized",
          status: "failed",
          message: "Remote replay operation document:create payload content exceeds 5 MB limit",
        },
      ],
    });
    await expect(dataClient.getDocument("local-space", "remote-oversized-doc")).rejects.toThrow(
      "Document not found",
    );
    expect(await dataClient.getSyncState()).toBe("error");
  });

  it("blocks remote provider document imports when a local queued operation targets the same entity", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Local Pending Edit",
      content: "local draft",
    });
    await dataClient.updateDocumentContent("local-space", created.id, "local pending draft");
    const envelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-09T12:00:00.000Z",
      "remote-import-doc-conflict",
    );

    const result = await dataClient.applyProviderRemoteImportManifest(
      buildProviderRemoteSyncManifest(envelope, {
        schema_version: 1,
        operation_count: 1,
        operations: [
          {
            id: "remote-doc-update-conflict",
            entity_type: "document",
            entity_id: created.id,
            action: "update_content",
            payload_status: "available",
            payload: {
              id: created.id,
              space_id: "local-space",
              title: "Local Pending Edit",
              content: "remote draft",
            },
          },
        ],
      }),
    );

    expect(result).toMatchObject({
      status: "blocked",
      applied: false,
      attempted_operation_count: 0,
      applied_operation_count: 0,
      skipped_operation_count: 0,
      failed_operation_count: 0,
      blockers: ["One or more remote replay operations failed or remained blocked"],
      results: [
        {
          operation_id: "remote-doc-update-conflict",
          status: "blocked",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local pending draft",
    });
    expect(await dataClient.getSyncState()).toBe("conflict");
  });

  it("blocks queued asset replay when binary bytes are unavailable", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Remote Asset",
      content: "asset reference",
    });
    await dataClient.saveBrowserAsset({
      name: "remote.png",
      mime_type: "image/png",
      size: 128,
      url: "asset://localhost/local-only.png",
      space_id: "local-space",
      document_id: created.id,
    });

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    const assetOperation = manifest.operations.find(
      (operation) => operation.entity_type === "asset" && operation.action === "create",
    );
    expect(assetOperation).toMatchObject({
      payload_status: "missing",
      payload: expect.objectContaining({
        name: "remote.png",
        space_id: "local-space",
        document_id: created.id,
      }),
    });
    expect(manifest.replay_readiness.ready).toBe(false);
    expect(manifest.operation_summary.missing_payload_count).toBe(1);
    expect(manifest.operation_summary.unavailable_payloads).toEqual([
      expect.objectContaining({
        operation_id: assetOperation?.id,
        entity_type: "asset",
        action: "create",
        reason: "Asset binary bytes are unavailable for replay",
        asset_name: "remote.png",
        document_id: created.id,
        source_url: "asset://localhost/local-only.png",
        byte_sources_checked: ["asset://localhost/local-only.png"],
        recovery_action:
          "Restore the asset file under the active vault or reattach the asset so replay can embed bytes.",
      }),
    ]);
  });

  it("preserves native asset byte packing failures in replay blockers", async () => {
    (
      window as unknown as { __TAURI_INTERNALS__?: { invoke: () => Promise<string> } }
    ).__TAURI_INTERNALS__ = {
      invoke: async () => {
        throw new Error("Native replay asset file was not found");
      },
    };
    const created = await dataClient.createDocument("local-space", {
      title: "Native Asset",
      content: "asset reference",
    });
    await dataClient.saveBrowserAsset({
      name: "native.png",
      mime_type: "image/png",
      size: 128,
      url: "asset://localhost/native.png",
      space_id: "local-space",
      document_id: created.id,
    });

    const manifest = await dataClient.exportQueuedSyncReplayManifest();
    const assetOperation = manifest.operations.find(
      (operation) => operation.entity_type === "asset" && operation.action === "create",
    );
    expect(assetOperation).toMatchObject({
      payload_status: "missing",
      payload: expect.objectContaining({
        native_pack_error: "Native replay asset file was not found",
      }),
    });
    expect(manifest.operation_summary.unavailable_payloads).toEqual([
      expect.objectContaining({
        operation_id: assetOperation?.id,
        reason: "Native replay asset file was not found",
        byte_sources_checked: ["asset://localhost/native.png"],
      }),
    ]);
  });

  it("returns local search results and empty notifications in local mode", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Roadmap Notes",
      content: "alpha beta gamma",
    });
    const untagged = await dataClient.createDocument("local-space", {
      title: "Plain Notes",
      content: "plain beta gamma",
    });
    const tag = await dataClient.createTag("local-space", { name: "Planning" });
    await dataClient.addDocumentTag("local-space", created.id, tag.id);
    const property = await dataClient.createPropertySchema("local-space", {
      name: "Status",
      property_type: "select",
      options: ["Draft", "Published"],
    });
    await dataClient.setDocumentPropertyValues("local-space", created.id, [
      { property_id: property.id, value: "Draft" },
    ]);
    await dataClient.updateDocument("local-space", created.id, {
      cover_image: "starlane://asset/cover-roadmap",
    });
    await dataClient.saveBrowserAsset({
      name: "roadmap.txt",
      mime_type: "text/plain",
      size: 5,
      url: "data:text/plain;base64,SGVsbG8=",
      space_id: "local-space",
      document_id: created.id,
    });

    const results = await dataClient.searchDocuments("alpha", 10);
    expect(results.some((r) => r.document_id === created.id)).toBe(true);
    let exported = await dataClient.exportLocalVaultSnapshot();
    expect(exported.search_index?.find((entry) => entry.document_id === created.id)).toMatchObject({
      title: "Roadmap Notes",
      space_id: "local-space",
    });

    await dataClient.updateDocumentContent("local-space", created.id, "delta epsilon");
    const updatedResults = await dataClient.searchDocuments("delta", 10);
    expect(updatedResults.map((r) => r.document_id)).toContain(created.id);
    exported = await dataClient.exportLocalVaultSnapshot();
    expect(
      exported.search_index?.find((entry) => entry.document_id === created.id)?.search_text,
    ).toContain("delta");

    const tagResults = await dataClient.searchDocuments("", 10, { tag: "Planning" });
    expect(tagResults.map((r) => r.document_id)).toContain(created.id);

    const folder = await dataClient.createFolder("local-space", "Roadmaps");
    await dataClient.updateDocument("local-space", created.id, { folder_id: folder.id });
    const folderResults = await dataClient.searchDocuments("", 10, { folder: "Roadmaps" });
    expect(folderResults.map((r) => r.document_id)).toContain(created.id);
    const folderIdResults = await dataClient.searchDocuments("", 10, { folder: folder.id });
    expect(folderIdResults.map((r) => r.document_id)).toContain(created.id);

    const propertyResults = await dataClient.searchDocuments("", 10, {
      property_name: "Status",
      property_value: "Draft",
    });
    expect(propertyResults.map((r) => r.document_id)).toContain(created.id);

    const combinedResults = await dataClient.searchDocuments("alpha", 10, {
      property_name: "Status",
      property_value: "Missing",
    });
    expect(combinedResults.map((r) => r.document_id)).not.toContain(created.id);

    const negativePresenceResults = await dataClient.searchDocuments("", 20, {
      has_attachment: "false",
      has_cover: "false",
      has_workflow: "false",
      has_tag: "false",
      has_property: "false",
      is_locked: "false",
    });
    expect(negativePresenceResults.map((r) => r.document_id)).toContain(untagged.id);
    expect(negativePresenceResults.map((r) => r.document_id)).not.toContain(created.id);

    const storedDocument = await dataClient.getDocument("local-space", created.id);
    expect(storedDocument.created_at).toBeTruthy();
    expect(storedDocument.updated_at).toBeTruthy();
    const createdDateResults = await dataClient.searchDocuments("", 20, {
      created_on: storedDocument.created_at!.slice(0, 10),
    });
    expect(createdDateResults.map((r) => r.document_id)).toContain(created.id);
    const updatedDateResults = await dataClient.searchDocuments("", 20, {
      updated_on: storedDocument.updated_at!.slice(0, 10),
    });
    expect(updatedDateResults.map((r) => r.document_id)).toContain(created.id);
    const missingDateResults = await dataClient.searchDocuments("", 20, {
      updated_on: "1900-01-01",
    });
    expect(missingDateResults.map((r) => r.document_id)).not.toContain(created.id);

    const notifications = await dataClient.listNotifications();
    expect(notifications).toEqual([]);
  });

  it("passes notification pagination and unread filters to the server", async () => {
    setCurrentLaunchMode("server");
    const requests: string[] = [];
    (globalThis as { fetch?: typeof fetch }).fetch = (async (input) => {
      requests.push(String(input));
      return {
        ok: true,
        status: 200,
        json: async () => [],
      } as Response;
    }) as typeof fetch;

    await expect(
      dataClient.listNotifications({
        limit: 25,
        unreadOnly: true,
        before: "2026-07-12T20:00:00Z|notification-1",
      }),
    ).resolves.toEqual([]);

    expect(requests).toEqual([
      "/api/v1/notifications?limit=25&before=2026-07-12T20%3A00%3A00Z%7Cnotification-1&unread_only=true",
    ]);
  });

  it("queries local documents with first-pass Dataview predicates", async () => {
    const created = await dataClient.createDocument("local-space", {
      title: "Dataview Alpha",
      content: "release checklist",
    });
    const other = await dataClient.createDocument("local-space", {
      title: "Dataview Beta",
      content: "published notes",
    });
    const tag = await dataClient.createTag("local-space", { name: "Roadmap" });
    await dataClient.addDocumentTag("local-space", created.id, tag.id);
    const status = await dataClient.createPropertySchema("local-space", {
      name: "reviewDate",
      property_type: "date",
    });
    await dataClient.setDocumentPropertyValues("local-space", created.id, [
      { property_id: status.id, value: "2026-7-6" },
    ]);

    const results = await dataClient.queryDataview(
      'title:"Dataview Alpha" AND tag:#roadmap AND reviewDate is 2026-07-06',
    );
    expect(results.map((result) => result.document_id)).toEqual([created.id]);
    expect(results.map((result) => result.document_id)).not.toContain(other.id);
  });

  it("supports provider sync no-op and conflict simulation in local mode", async () => {
    const pull = await dataClient.syncWithProvider("pull");
    expect(pull.conflicts).toBe(0);
    expect(await dataClient.getSyncState()).toBe("idle");

    await dataClient.simulateSyncConflict();
    const readiness = await dataClient.getProviderSyncReadiness();
    expect(readiness.ready).toBe(false);
    expect(readiness.pending_conflict_count).toBe(1);
    expect(readiness.blockers[0]).toContain("must be resolved");
    await expect(dataClient.syncWithProvider("push")).rejects.toThrow("Provider sync is not ready");
    expect(await dataClient.getSyncState()).toBe("conflict");

    await dataClient.clearSyncIssue();
    expect(await dataClient.getSyncState()).toBe("idle");
  });

  it("supports provider connect/disconnect and conflict resolution actions", async () => {
    setSyncProvider("google-drive");
    const disconnectedReadiness = await dataClient.getProviderSyncReadiness();
    expect(disconnectedReadiness.ready).toBe(false);
    expect(disconnectedReadiness.blockers).toContain("Google Drive is not connected");

    const connected = await dataClient.connectProvider();
    expect(connected.connected).toBe(true);
    expect((await dataClient.getProviderStatus()).connected).toBe(true);
    const connectedReadiness = await dataClient.getProviderSyncReadiness();
    expect(connectedReadiness.ready).toBe(true);
    expect(connectedReadiness.remote_mutation_plans).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          direction: "push",
          status: "dry_run_only",
          remote_mutation_enabled: false,
        }),
      ]),
    );

    await dataClient.simulateSyncConflict();
    await dataClient.resolveSyncConflict("keep-local");
    expect(await dataClient.getSyncState()).toBe("idle");

    await dataClient.simulateSyncConflict();
    await dataClient.resolveSyncConflict("merge");
    expect(await dataClient.getSyncState()).toBe("idle");

    const disconnected = await dataClient.disconnectProvider();
    expect(disconnected.connected).toBe(false);
  });

  it("exposes explicit provider remote sync enablement in local mode", async () => {
    setSyncProvider("google-drive");
    writeProviderSession("google-drive", {
      provider: "google-drive",
      connected: true,
      sync_mode: "manual",
      auth: {
        access_token: "access-token",
        refresh_token: "refresh-token",
        account_label: "eagle@example.com",
        scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
        issued_at: "2026-07-08T12:00:00.000Z",
        expires_at: "2099-07-08T13:00:00.000Z",
        provider_root_id: "drive-root-id",
        provider_root_name: "Starlane Notes",
      },
      last_operation: "connect",
    });

    const enabled = await dataClient.enableProviderRemoteSync();
    expect(enabled.remote_contract.status).toBe("enabled");
    expect(enabled.remote_sync_enablement.enabled).toBe(true);
    expect(enabled.remote_mutation_plans).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          direction: "push",
          status: "ready",
          remote_mutation_enabled: true,
          dry_run: false,
        }),
      ]),
    );

    const fetchCalls: Array<{ input: string; init: RequestInit }> = [];
    (globalThis as { fetch?: typeof fetch }).fetch = (async (input, init) => {
      fetchCalls.push({ input: String(input), init: init ?? {} });
      return {
        ok: true,
        status: 200,
        json: async () => ({
          id: "drive-provider-sync-manifest",
          name: "provider-sync-manifest.json",
        }),
      } as Response;
    }) as typeof fetch;

    const syncResult = await dataClient.syncWithProvider("push");
    expect(syncResult.pushed).toBe(1);
    expect(syncResult.remote_mutation_attempt).toEqual(
      expect.objectContaining({
        provider: "google-drive",
        direction: "push",
        status: "applied",
        transport_kind: "provider-api",
        applied: true,
        blockers: [],
      }),
    );
    expect(fetchCalls).toHaveLength(1);
    expect(fetchCalls[0].input).toBe(
      "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart",
    );
    expect(fetchCalls[0].init.method).toBe("POST");
    expect(fetchCalls[0].init.headers).toMatchObject({
      Authorization: "Bearer access-token",
      "X-Starlane-Idempotency-Key": expect.stringContaining("google-drive-push"),
    });
    expect(String(fetchCalls[0].init.body)).toContain("provider-sync-manifest.json");
    expect(String(fetchCalls[0].init.body)).toContain("starlane-provider-sync-manifest");
    expect(String(fetchCalls[0].init.body)).toContain('"replay_manifest"');
    expect(String(fetchCalls[0].init.body)).toContain('"operation_summary"');
    expect(await dataClient.getSyncState()).toBe("idle");

    const disabled = await dataClient.disableProviderRemoteSync("test disabled");
    expect(disabled.remote_contract.status).toBe("contract_only");
    expect(disabled.remote_sync_enablement).toEqual(
      expect.objectContaining({
        enabled: false,
        reason: "test disabled",
      }),
    );
  });

  it("reviews provider pull imports against local queued operations", async () => {
    setSyncProvider("google-drive");
    writeProviderSession("google-drive", {
      provider: "google-drive",
      connected: true,
      sync_mode: "manual",
      auth: {
        access_token: "access-token",
        refresh_token: "refresh-token",
        account_label: "eagle@example.com",
        scopes: ["drive.file", "drive.metadata.readonly", "offline_access"],
        issued_at: "2026-07-08T12:00:00.000Z",
        expires_at: "2099-07-08T13:00:00.000Z",
        provider_root_id: "drive-root-id",
        provider_root_name: "Starlane Notes",
      },
      last_operation: "connect",
    });
    await dataClient.enableProviderRemoteSync();
    const created = await dataClient.createDocument("local-space", {
      title: "Pending Local",
      content: "local draft",
    });
    await dataClient.updateDocumentContent("local-space", created.id, "local queued draft");
    const tag = await dataClient.createTag("local-space", { name: "Pending Tag" });
    await dataClient.updateTag("local-space", tag.id, { name: "Pending Tag Rename" });

    const remoteEnvelope = buildProviderRemoteMutationEnvelope(
      buildProviderRemoteMutationPlan(
        "google-drive",
        "Google Drive",
        "push",
        {
          status: "enabled",
          root_strategy: "drive-appdata-or-user-selected-folder",
          auth_model: "oauth2-offline-access",
          required_scopes: ["drive.file"],
          write_model: "local-sqlite-first-then-provider-api-push",
          conflict_model: "provider-revision-plus-local-replay-queue-validation",
        },
        {
          provider: "google-drive",
          connected: true,
          account_label: "Google account",
          scopes: ["drive.file"],
          sync_mode: "manual",
        },
      ),
      "2026-07-10T12:00:00.000Z",
      "remote-pull-review-conflict",
    );
    const fetchCalls: Array<{ input: string; init: RequestInit }> = [];
    (globalThis as { fetch?: typeof fetch }).fetch = (async (input, init) => {
      fetchCalls.push({ input: String(input), init: init ?? {} });
      if (String(input).includes("/drive/v3/files/drive-provider-sync-manifest")) {
        return {
          ok: true,
          status: 200,
          json: async () =>
            buildProviderRemoteSyncManifest(remoteEnvelope, {
              schema_version: 1,
              operation_count: 2,
              operation_summary: {
                ready_operation_count: 2,
                missing_payload_count: 0,
              },
              operations: [
                {
                  id: "remote-doc-update-conflict",
                  entity_type: "document",
                  entity_id: created.id,
                  action: "update_content",
                  payload_status: "available",
                  payload: {
                    id: created.id,
                    space_id: "local-space",
                    title: "Pending Local",
                    content: "remote draft",
                  },
                },
                {
                  id: "remote-document-tag-related-conflict",
                  entity_type: "document_tag",
                  entity_id: `${created.id}:${tag.id}`,
                  action: "add",
                  payload_status: "available",
                  payload: {
                    document_id: created.id,
                    tag_id: tag.id,
                    space_id: "local-space",
                  },
                },
              ],
            }),
        } as Response;
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          files: [
            {
              id: "drive-provider-sync-manifest",
              name: "provider-sync-manifest.json",
              modifiedTime: "2026-07-10T12:00:00.000Z",
            },
          ],
        }),
      } as Response;
    }) as typeof fetch;

    const result = await dataClient.syncWithProvider("pull");

    expect(fetchCalls).toHaveLength(2);
    expect(result.conflicts).toBe(2);
    expect(await dataClient.getSyncState()).toBe("conflict");
    expect(result.remote_mutation_attempt?.import_dry_run).toMatchObject({
      operation_count: 2,
      would_apply_count: 0,
      conflict_count: 2,
      operations: [
        {
          operation_id: "remote-doc-update-conflict",
          status: "conflict",
          message: "A local queued operation targets the same entity",
        },
        {
          operation_id: "remote-document-tag-related-conflict",
          status: "conflict",
          message: "A local queued operation targets a related entity",
        },
      ],
    });
    expect(result.remote_mutation_attempt?.import_apply_plan).toMatchObject({
      selected_operation_count: 0,
      skipped_operation_count: 2,
      operations: [
        {
          operation_id: "remote-doc-update-conflict",
          apply_status: "skipped_conflict",
        },
        {
          operation_id: "remote-document-tag-related-conflict",
          apply_status: "skipped_conflict",
        },
      ],
    });
    await expect(dataClient.getDocumentContent("local-space", created.id)).resolves.toMatchObject({
      content: "local queued draft",
    });
  });

  it("queues, lists, and resolves document-level conflicts by id", async () => {
    const first = await dataClient.createDocument("local-space", {
      title: "Conflict One",
      content: "local a",
    });
    const second = await dataClient.createDocument("local-space", {
      title: "Conflict Two",
      content: "local b",
    });

    const firstConflict = await dataClient.simulateSyncConflictForDocument(first.id);
    const secondConflict = await dataClient.simulateSyncConflictForDocument(second.id);
    expect(firstConflict?.document_id).toBe(first.id);
    expect(secondConflict?.document_id).toBe(second.id);

    const conflicts = await dataClient.listConflicts();
    expect(conflicts).toHaveLength(2);
    expect(conflicts.map((c) => c.document_id)).toEqual(
      expect.arrayContaining([first.id, second.id]),
    );
    expect(await dataClient.getSyncState()).toBe("conflict");

    await dataClient.resolveConflict(firstConflict!.id, "keep-local");
    expect((await dataClient.listConflicts()).map((c) => c.id)).toEqual([secondConflict!.id]);
    expect(await dataClient.getSyncState()).toBe("conflict");

    const secondBefore = await dataClient.getDocumentContent("local-space", second.id);
    await dataClient.resolveConflict(secondConflict!.id, "keep-remote");
    const secondAfter = await dataClient.getDocumentContent("local-space", second.id);
    expect(secondAfter.content).not.toBe(secondBefore.content);
    expect(secondAfter.content).toContain("[Remote change detected");
    expect(await dataClient.listConflicts()).toHaveLength(0);
    expect(await dataClient.getSyncState()).toBe("idle");
  });
});
