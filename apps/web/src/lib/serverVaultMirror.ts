import {
  createLocalVaultAdapter,
  importNativeVaultAssetFromDataUrl,
  LOCAL_VAULT_SCHEMA_VERSION,
  type LocalVaultSnapshot,
  type VaultAsset,
  type VaultDocument,
  type VaultDocumentDraft,
  type VaultFolder,
  type VaultSpace,
} from "@/lib/localVault";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { createVaultFolder } from "@/lib/tauriVault";

interface ServerSpaceSummary {
  id: string;
  name: string;
  slug?: string;
  description?: string;
  owner_username?: string;
  owner_display_name?: string;
  created_by?: string;
  created_at?: string;
  updated_at?: string;
}

interface ServerFolderSummary {
  id: string;
  name: string;
  icon?: string;
  parent_id?: string | null;
}

interface ServerDocumentSummary {
  human_identifier?: string | null;
  maintenance_user_id?: string | null;
  maintenance_group_id?: string | null;
  id: string;
  title: string;
  icon?: string;
  cover_image?: string;
  locked?: boolean;
  folder_id?: string | null;
  parent_id?: string | null;
  created_by?: string;
  last_updated_by?: string;
  created_at?: string;
  updated_at?: string;
  published_at?: string;
}

interface ServerDocumentContent {
  id: string;
  title: string;
  content: string;
}

interface ServerDocumentDraftContent {
  id?: string;
  document_id?: string;
  space_id?: string;
  owner_id?: string;
  title?: string;
  content?: string;
  icon?: string;
  cover_image?: string;
  base_updated_at?: string;
  created_at?: string;
  updated_at?: string;
  previous_folder_id?: string;
  previous_parent_id?: string;
}

export interface ServerAttachmentMirrorInput {
  id: string;
  filename: string;
  original_name: string;
  mime_type: string;
  size_bytes: number;
  url: string;
  created_at: string;
}

interface PendingServerMirrorUpdate {
  path: string;
  value: unknown;
}

const SERVER_MIRROR_FLUSH_DELAY_MS = 50;
let pendingUpdates: PendingServerMirrorUpdate[] = [];
let flushTimer: number | undefined;
let flushChain: Promise<void> = Promise.resolve();

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function emptySnapshot(): LocalVaultSnapshot {
  return {
    schema_version: LOCAL_VAULT_SCHEMA_VERSION,
    spaces: [],
    folders: [],
    documents: [],
    assets: [],
    templates: [],
    workflow_templates: [],
    document_workflows: [],
    document_workflow_events: [],
    property_schemas: [],
    property_values: [],
    tags: [],
    doc_tags: [],
    search_index: [],
    sync_state: {
      state: "idle",
      updated_at: new Date().toISOString(),
      queued_operations: [],
    },
  };
}

function upsertById<T extends { id: string }>(items: T[], next: T): T[] {
  const index = items.findIndex((item) => item.id === next.id);
  if (index < 0) return [...items, next];
  return items.map((item, itemIndex) => (itemIndex === index ? next : item));
}

function serverSpaceToVaultSpace(
  space: ServerSpaceSummary,
  current: VaultSpace | undefined,
  now: string,
): VaultSpace {
  return {
    id: space.id,
    name: space.name,
    slug: space.slug,
    description: space.description,
    owner_username: space.owner_username,
    owner_display_name: space.owner_display_name,
    created_by: space.created_by,
    created_at: space.created_at ?? current?.created_at ?? now,
    updated_at: space.updated_at ?? current?.updated_at ?? now,
  };
}

function serverFolderToVaultFolder(
  spaceId: string,
  folder: ServerFolderSummary,
): VaultFolder {
  return {
    id: folder.id,
    space_id: spaceId,
    name: folder.name,
    icon: folder.icon,
    parent_id: folder.parent_id ?? undefined,
  };
}

function serverDocumentToVaultDocument(
  spaceId: string,
  document: ServerDocumentSummary,
  current: VaultDocument | undefined,
  now: string,
): VaultDocument {
  return {
    id: document.id,
    human_identifier: document.human_identifier !== undefined ? document.human_identifier : current?.human_identifier,
    maintenance_user_id: document.maintenance_user_id !== undefined ? document.maintenance_user_id : current?.maintenance_user_id,
    maintenance_group_id: document.maintenance_group_id !== undefined ? document.maintenance_group_id : current?.maintenance_group_id,
    space_id: spaceId,
    title: document.title,
    content: current?.content ?? "",
    icon: document.icon,
    cover_image: document.cover_image,
    locked: document.locked,
    folder_id: document.folder_id ?? undefined,
    parent_id: document.parent_id ?? undefined,
    created_by: document.created_by,
    last_updated_by: document.last_updated_by,
    created_at: document.created_at ?? current?.created_at ?? now,
    updated_at: document.updated_at ?? current?.updated_at ?? now,
    published_at: document.published_at,
  };
}

export function deriveServerVaultFolderName(serverUrl: string): string {
  try {
    const parsed = new URL(serverUrl);
    return parsed.port ? `${parsed.hostname}-${parsed.port}` : parsed.hostname;
  } catch {
    return "Starlane Notes Server";
  }
}

export function mergeServerResponseIntoVaultSnapshot(
  snapshot: LocalVaultSnapshot,
  path: string,
  value: unknown,
  now = new Date().toISOString(),
): LocalVaultSnapshot {
  let next = snapshot;

  if (path === "/spaces" && Array.isArray(value)) {
    const spaces = value.filter(
      (item): item is ServerSpaceSummary =>
        Boolean(item) &&
        typeof item === "object" &&
        typeof (item as ServerSpaceSummary).id === "string" &&
        typeof (item as ServerSpaceSummary).name === "string",
    );
    next = {
      ...next,
      spaces: spaces.reduce(
        (items, space) =>
          upsertById(
            items,
            serverSpaceToVaultSpace(
              space,
              items.find((item) => item.id === space.id),
              now,
            ),
          ),
        next.spaces,
      ),
    };
  }

  const foldersMatch = path.match(/^\/spaces\/([^/]+)\/folders(?:\?.*)?$/);
  if (foldersMatch && Array.isArray(value)) {
    const spaceId = foldersMatch[1];
    next = {
      ...next,
      folders: value.reduce<VaultFolder[]>((items, item) => {
        if (
          !item ||
          typeof item !== "object" ||
          typeof (item as ServerFolderSummary).id !== "string" ||
          typeof (item as ServerFolderSummary).name !== "string"
        ) {
          return items;
        }
        return upsertById(items, serverFolderToVaultFolder(spaceId, item as ServerFolderSummary));
      }, next.folders),
    };
  }

  const documentsMatch = path.match(/^\/spaces\/([^/]+)\/documents(?:\?.*)?$/);
  if (documentsMatch && Array.isArray(value)) {
    const spaceId = documentsMatch[1];
    next = {
      ...next,
      documents: value.reduce<VaultDocument[]>((items, item) => {
        if (
          !item ||
          typeof item !== "object" ||
          typeof (item as ServerDocumentSummary).id !== "string" ||
          typeof (item as ServerDocumentSummary).title !== "string"
        ) {
          return items;
        }
        const document = item as ServerDocumentSummary;
        return upsertById(
          items,
          serverDocumentToVaultDocument(
            spaceId,
            document,
            items.find((current) => current.id === document.id),
            now,
          ),
        );
      }, next.documents),
    };
  }

  if (
    documentsMatch &&
    value &&
    typeof value === "object" &&
    typeof (value as Partial<ServerDocumentSummary>).id === "string" &&
    typeof (value as Partial<ServerDocumentSummary>).title === "string"
  ) {
    const spaceId = documentsMatch[1];
    const document = value as ServerDocumentSummary;
    next = {
      ...next,
      documents: upsertById(
        next.documents,
        serverDocumentToVaultDocument(
          spaceId,
          document,
          next.documents.find((current) => current.id === document.id),
          now,
        ),
      ),
    };
  }

  const documentDeleteMatch = path.match(/^\/spaces\/([^/]+)\/documents\/([^/]+)$/);
  if (documentDeleteMatch && value === null) {
    const [, spaceId, documentId] = documentDeleteMatch;
    next = {
      ...next,
      documents: next.documents.filter(
        (document) => !(document.space_id === spaceId && document.id === documentId),
      ),
    };
  }

  const contentMatch = path.match(
    /^\/spaces\/([^/]+)\/documents\/([^/]+)\/content$/,
  );
  if (
    contentMatch &&
    value &&
    typeof value === "object" &&
    typeof (value as ServerDocumentContent).content === "string"
  ) {
    const [spaceId, documentId] = contentMatch.slice(1);
    const content = value as ServerDocumentContent;
    const current = next.documents.find((document) => document.id === documentId);
    next = {
      ...next,
      documents: upsertById(next.documents, {
        id: documentId,
        human_identifier: current?.human_identifier,
        maintenance_user_id: current?.maintenance_user_id,
        maintenance_group_id: current?.maintenance_group_id,
        space_id: spaceId,
        title: content.title || current?.title || "Untitled",
        content: content.content,
        icon: current?.icon,
        cover_image: current?.cover_image,
        locked: current?.locked,
        folder_id: current?.folder_id,
        parent_id: current?.parent_id,
        created_by: current?.created_by,
        last_updated_by: current?.last_updated_by,
        created_at: current?.created_at ?? now,
        updated_at: current?.updated_at ?? now,
        published_at: current?.published_at,
      }),
    };
  }

  const folderItemMatch = path.match(/^\/spaces\/([^/]+)\/folders$/);
  if (
    folderItemMatch &&
    value &&
    typeof value === "object" &&
    typeof (value as Partial<ServerFolderSummary>).id === "string" &&
    typeof (value as Partial<ServerFolderSummary>).name === "string"
  ) {
    next = {
      ...next,
      folders: upsertById(
        next.folders,
        serverFolderToVaultFolder(folderItemMatch[1], value as ServerFolderSummary),
      ),
    };
  }

  const folderDeleteMatch = path.match(/^\/spaces\/([^/]+)\/folders\/([^/]+)$/);
  if (folderDeleteMatch && value === null) {
    const [, spaceId, folderId] = folderDeleteMatch;
    next = {
      ...next,
      folders: next.folders.filter(
        (folder) => !(folder.space_id === spaceId && folder.id === folderId),
      ),
    };
  }

  const draftMatch = path.match(
    /^\/spaces\/([^/]+)\/documents\/([^/]+)\/drafts(?:\/([^/?]+))?$/,
  );
  if (draftMatch && value && typeof value === "object") {
    const [, spaceId, documentId, pathDraftId] = draftMatch;
    const draft = value as ServerDocumentDraftContent;
    const current = next.documents.find((document) => document.id === documentId);
    const draftId = draft.id ?? pathDraftId;
    const currentDraft = (next.drafts ?? []).find(
      (item) => item.id === draftId && item.document_id === documentId && item.space_id === spaceId,
    );
    const content = draft.content ?? currentDraft?.content;
    if (draftId && typeof content === "string") {
      next = {
        ...next,
        documents: upsertById(next.documents, {
          id: documentId,
          human_identifier: current?.human_identifier,
        maintenance_user_id: current?.maintenance_user_id,
        maintenance_group_id: current?.maintenance_group_id,
          space_id: spaceId,
          title: draft.title || currentDraft?.title || current?.title || "Untitled",
          content,
          icon: draft.icon ?? currentDraft?.icon ?? current?.icon,
          cover_image: draft.cover_image ?? currentDraft?.cover_image ?? current?.cover_image,
          locked: current?.locked,
          folder_id: current?.folder_id,
          parent_id: current?.parent_id,
          created_by: current?.created_by,
          last_updated_by: current?.last_updated_by,
          created_at: current?.created_at ?? now,
          updated_at: draft.updated_at ?? currentDraft?.updated_at ?? current?.updated_at ?? now,
          published_at: current?.published_at,
        }),
        drafts: upsertById(next.drafts ?? [], {
          id: draftId,
          document_id: draft.document_id ?? currentDraft?.document_id ?? documentId,
          space_id: draft.space_id ?? currentDraft?.space_id ?? spaceId,
          owner_id: draft.owner_id ?? currentDraft?.owner_id ?? "server-user",
          title: draft.title || currentDraft?.title || current?.title || "Untitled",
          content,
          icon: draft.icon ?? currentDraft?.icon,
          cover_image: draft.cover_image ?? currentDraft?.cover_image,
          base_updated_at:
            draft.base_updated_at ?? currentDraft?.base_updated_at ?? draft.updated_at ?? now,
          created_at: draft.created_at ?? currentDraft?.created_at ?? draft.updated_at ?? now,
          updated_at: draft.updated_at ?? now,
          previous_folder_id: draft.previous_folder_id ?? currentDraft?.previous_folder_id,
          previous_parent_id: draft.previous_parent_id ?? currentDraft?.previous_parent_id,
        } satisfies VaultDocumentDraft),
      };
    }
  }

  const draftPurgeMatch = path.match(/^\/spaces\/([^/]+)\/drafts$/);
  if (
    draftPurgeMatch &&
    value &&
    typeof value === "object" &&
    typeof (value as { deleted?: unknown }).deleted === "number"
  ) {
    next = {
      ...next,
      drafts: (next.drafts ?? []).filter((draft) => draft.space_id !== draftPurgeMatch[1]),
    };
  }

  const draftDeleteMatch = path.match(
    /^\/spaces\/([^/]+)\/documents\/([^/]+)\/drafts\/([^/?]+)$/,
  );
  if (draftDeleteMatch && value === null) {
    const [, spaceId, documentId, draftId] = draftDeleteMatch;
    next = {
      ...next,
      drafts: (next.drafts ?? []).filter(
        (draft) =>
          !(draft.space_id === spaceId && draft.document_id === documentId && draft.id === draftId),
      ),
    };
  }

  const draftPublishMatch = path.match(
    /^\/spaces\/([^/]+)\/documents\/([^/]+)\/drafts\/([^/?]+)\/publish$/,
  );
  if (draftPublishMatch) {
    const [, spaceId, documentId, draftId] = draftPublishMatch;
    next = {
      ...next,
      drafts: (next.drafts ?? []).filter(
        (draft) =>
          !(draft.space_id === spaceId && draft.document_id === documentId && draft.id === draftId),
      ),
    };
  }

  return next;
}

export function mergeServerAttachmentIntoVaultSnapshot(
  snapshot: LocalVaultSnapshot,
  spaceId: string,
  documentId: string,
  attachment: ServerAttachmentMirrorInput,
  dataUrl: string,
  persistedAsset?: VaultAsset | null,
): LocalVaultSnapshot {
  const asset: VaultAsset = {
    ...(persistedAsset ?? {}),
    id: persistedAsset?.id ?? attachment.id,
    name: persistedAsset?.name ?? (attachment.original_name || attachment.filename),
    mime_type: persistedAsset?.mime_type ?? attachment.mime_type,
    size: persistedAsset?.size ?? attachment.size_bytes,
    url: persistedAsset?.url ?? dataUrl,
    source: persistedAsset?.source ?? "browser",
    space_id: spaceId,
    document_id: documentId,
    created_at: persistedAsset?.created_at ?? attachment.created_at,
  };
  return { ...snapshot, assets: upsertById(snapshot.assets, asset) };
}

export async function ensureServerVaultFolder(serverUrl: string): Promise<void> {
  if (!isTauriRuntime()) return;
  await flushServerVaultMirror();
  await createVaultFolder(deriveServerVaultFolderName(serverUrl));
}

async function persistPendingUpdates(): Promise<void> {
  if (!isTauriRuntime() || getCurrentLaunchMode() !== "server") {
    pendingUpdates = [];
    return;
  }
  const updates = pendingUpdates;
  pendingUpdates = [];
  if (updates.length === 0) return;

  const adapter = createLocalVaultAdapter("none");
  await adapter.init();
  let snapshot = (await adapter.loadSnapshot()) ?? emptySnapshot();
  for (const update of updates) {
    snapshot = mergeServerResponseIntoVaultSnapshot(snapshot, update.path, update.value);
  }
  await adapter.saveSnapshot(snapshot);
}

function scheduleFlush(): void {
  if (typeof window === "undefined" || flushTimer !== undefined) return;
  flushTimer = window.setTimeout(() => {
    flushTimer = undefined;
    void flushServerVaultMirror();
  }, SERVER_MIRROR_FLUSH_DELAY_MS);
}

export function queueServerResponseForVaultMirror(
  serverBase: string,
  path: string,
  value: unknown,
): void {
  // Access inspection is a display-only projection, never a canonical vault record.
  if (/^\/spaces\/[^/]+\/documents\/[^/]+\/access(?:\/inspect)?$/.test(path)) return;
  // Manager projections and expiring confirmations are never workspace records.
  if (/^\/spaces\/[^/]+\/documents\/[^/]+\/restrictions\/(?:inspect|preview|apply)(?:\?[^#]*)?$/.test(path)) return;
  if (/^\/moves\/documents\/[^/]+\/preserved\/inspect(?:\?[^#]*)?$/.test(path) ||
      /^\/moves\/commits\/[^/]+\/preserved\/[^/]+\/inspect$/.test(path)) return;
  if (!isTauriRuntime() || getCurrentLaunchMode() !== "server") return;
  const configuredServer = window.localStorage.getItem("server_url")?.replace(/\/+$/, "");
  if (!configuredServer || `${configuredServer}/api/v1` !== serverBase.replace(/\/+$/, "")) {
    return;
  }
  pendingUpdates.push({ path, value });
  scheduleFlush();
}

/**
 * Applies an offline mutation to the durable native server vault immediately.
 * The queued request remains authoritative for eventual server replay, but the
 * phone must be able to reopen the edited workspace before that replay occurs.
 */
export function queueServerMutationForVaultMirror(
  serverBase: string,
  path: string,
  method: "POST" | "PUT" | "DELETE",
  body: unknown,
): void {
  if (!isTauriRuntime() || getCurrentLaunchMode() !== "server") return;
  const configuredServer = window.localStorage.getItem("server_url")?.replace(/\/+$/, "");
  if (!configuredServer || `${configuredServer}/api/v1` !== serverBase.replace(/\/+$/, "")) {
    return;
  }

  const documentCreateMatch = path.match(/^\/spaces\/([^/]+)\/documents$/);
  if (method === "POST" && documentCreateMatch && body && typeof body === "object") {
    const input = body as Record<string, unknown>;
    if (typeof input.source_id !== "string" || typeof input.title !== "string") return;
    queueServerResponseForVaultMirror(serverBase, path, {
      id: input.source_id,
      title: input.title,
      icon: typeof input.icon === "string" ? input.icon : undefined,
      cover_image: typeof input.cover_image === "string" ? input.cover_image : undefined,
      folder_id: typeof input.folder_id === "string" ? input.folder_id : undefined,
      parent_id: typeof input.parent_id === "string" ? input.parent_id : undefined,
      created_at: typeof input.created_at === "string" ? input.created_at : undefined,
      updated_at: typeof input.updated_at === "string" ? input.updated_at : undefined,
    });
    if (typeof input.content === "string") {
      queueServerResponseForVaultMirror(
        serverBase,
        `/spaces/${documentCreateMatch[1]}/documents/${input.source_id}/content`,
        { id: input.source_id, title: input.title, content: input.content },
      );
    }
    return;
  }

  const folderCreateMatch = path.match(/^\/spaces\/([^/]+)\/folders$/);
  if (method === "POST" && folderCreateMatch && body && typeof body === "object") {
    const input = body as Record<string, unknown>;
    if (typeof input.source_id !== "string" || typeof input.name !== "string") return;
    queueServerResponseForVaultMirror(serverBase, path, {
      id: input.source_id,
      name: input.name,
      icon: typeof input.icon === "string" ? input.icon : undefined,
      parent_id: typeof input.parent_id === "string" ? input.parent_id : null,
    });
    return;
  }

  const documentMatch = path.match(/^\/spaces\/([^/]+)\/documents\/([^/]+)$/);
  if (documentMatch && method === "DELETE") {
    queueServerResponseForVaultMirror(serverBase, path, null);
    return;
  }
  const folderMatch = path.match(/^\/spaces\/([^/]+)\/folders\/([^/]+)$/);
  if (folderMatch && method === "DELETE") {
    queueServerResponseForVaultMirror(serverBase, `/spaces/${folderMatch[1]}/folders`, null);
    queueServerResponseForVaultMirror(serverBase, path, null);
    return;
  }

  const draftMatch = path.match(
    /^\/spaces\/([^/]+)\/documents\/([^/]+)\/drafts\/([^/?]+)$/,
  );
  if (draftMatch && method === "DELETE") {
    queueServerResponseForVaultMirror(serverBase, path, null);
    return;
  }
  if (draftMatch && method === "PUT" && body && typeof body === "object") {
    queueServerResponseForVaultMirror(serverBase, path, body);
    return;
  }

  if (body && typeof body === "object") {
    if (path.match(/^\/spaces\/[^/]+\/documents\/[^/]+\/content$/)) {
      queueServerResponseForVaultMirror(serverBase, path, body);
      return;
    }
    if (documentMatch) {
      queueServerResponseForVaultMirror(serverBase, `/spaces/${documentMatch[1]}/documents`, {
        id: documentMatch[2],
        ...(body as Record<string, unknown>),
      });
      return;
    }
    if (folderMatch) {
      queueServerResponseForVaultMirror(serverBase, `/spaces/${folderMatch[1]}/folders`, {
        id: folderMatch[2],
        ...(body as Record<string, unknown>),
      });
    }
  }
}

export async function loadServerVaultSnapshot(): Promise<LocalVaultSnapshot | null> {
  if (!isTauriRuntime() || getCurrentLaunchMode() !== "server") return null;
  const adapter = createLocalVaultAdapter("none");
  await adapter.init();
  return adapter.loadSnapshot();
}

export async function flushServerVaultMirror(): Promise<void> {
  if (typeof window !== "undefined" && flushTimer !== undefined) {
    window.clearTimeout(flushTimer);
    flushTimer = undefined;
  }
  flushChain = flushChain.then(persistPendingUpdates, persistPendingUpdates);
  await flushChain;
}

export async function mirrorServerAttachmentToVault(
  spaceId: string,
  documentId: string,
  attachment: ServerAttachmentMirrorInput,
  dataUrl: string,
): Promise<void> {
  if (!isTauriRuntime() || getCurrentLaunchMode() !== "server") return;
  await flushServerVaultMirror();
  flushChain = flushChain.then(async () => {
    const adapter = createLocalVaultAdapter("none");
    await adapter.init();
    const snapshot = await adapter.loadSnapshot();
    let persistedAsset: VaultAsset | null = null;
    try {
      persistedAsset = await importNativeVaultAssetFromDataUrl({
        name: attachment.original_name || attachment.filename,
        mime_type: attachment.mime_type,
        data_url: dataUrl,
        space_id: spaceId,
        document_id: documentId,
      });
    } catch (error) {
      console.warn("Native vault attachment import failed; keeping a browser copy", error);
    }
    await adapter.saveSnapshot(
      mergeServerAttachmentIntoVaultSnapshot(
        snapshot,
        spaceId,
        documentId,
        attachment,
        persistedAsset?.url ?? dataUrl,
        persistedAsset,
      ),
    );
  });
  await flushChain;
}
