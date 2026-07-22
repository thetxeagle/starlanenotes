import type { SyncProviderKind } from "@/lib/storageProviders";
import {
  NATIVE_LOCAL_DATABASE_COMMANDS,
  type NativeLocalDatabaseDescriptor,
  type NativeLocalDatabaseRecordCollection,
} from "@/lib/nativeLocalDatabase";

export type LocalVaultSyncState = "idle" | "syncing" | "conflict" | "error";
export const LOCAL_VAULT_SCHEMA_VERSION = 1;
const LOCAL_VAULT_DB_NAME = "starlane-local-vault";
const LOCAL_VAULT_DB_STORE = "snapshots";

export interface VaultSpace {
  id: string;
  name: string;
  slug?: string;
  owner_username?: string;
  owner_display_name?: string;
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface VaultFolder {
  id: string;
  space_id: string;
  name: string;
  parent_id?: string;
}

export interface VaultDocument {
  id: string;
  space_id: string;
  title: string;
  content: string;
  icon?: string;
  cover_image?: string;
  locked?: boolean;
  folder_id?: string;
  parent_id?: string;
  created_by?: string;
  last_updated_by?: string;
  created_at?: string;
  updated_at: string;
}

export interface VaultAsset {
  id: string;
  name: string;
  mime_type?: string;
  size: number;
  url: string;
  vault_path?: string;
  source: "browser" | "tauri";
  /** Optional ownership added after the initial vault schema; absent on legacy assets. */
  space_id?: string;
  document_id?: string;
  created_at: string;
}

export interface VaultTemplate {
  id: string;
  space_id: string;
  name: string;
  description?: string;
  content: string;
  properties?: unknown;
  tags: string[];
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface VaultWorkflowTemplateStage {
  id: string;
  name: string;
  sort_order: number;
}

export interface VaultWorkflowTemplateApprover {
  id: string;
  approver_type: "user" | "role";
  approver_id: string;
  stage_id?: string;
}

export interface VaultWorkflowTemplate {
  id: string;
  space_id: string;
  name: string;
  description?: string;
  stages: VaultWorkflowTemplateStage[];
  approvers: VaultWorkflowTemplateApprover[];
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface VaultDocumentWorkflow {
  id: string;
  document_id: string;
  workflow_template_id: string;
  current_stage_id: string;
  status: "active" | "completed" | "cancelled";
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface VaultDocumentWorkflowEvent {
  id: string;
  document_workflow_id: string;
  document_id: string;
  from_stage_id?: string;
  to_stage_id: string;
  action: "transition" | "enable" | "disable" | "delegate" | "escalate";
  note?: string;
  created_by?: string;
  created_at: string;
}

export interface VaultPropertySchema {
  id: string;
  space_id: string;
  name: string;
  property_type: string;
  options?: unknown;
  sort_order: number;
}

export interface VaultPropertyValue {
  id: string;
  document_id: string;
  property_id: string;
  value: string | null;
  created_at?: string;
  updated_at?: string;
}

export interface VaultTag {
  id: string;
  space_id: string;
  name: string;
  color: string;
  created_at?: string;
}

export interface VaultDocTag {
  document_id: string;
  tag_id: string;
}

export interface VaultSearchIndexEntry {
  document_id: string;
  space_id: string;
  title: string;
  content: string;
  search_text: string;
  updated_at: string;
}

export interface VaultQueuedSyncOperation {
  id: string;
  entity_type: string;
  entity_id: string;
  action: string;
  queued_at: string;
  payload?: unknown;
  attempts?: number;
  last_error?: string;
}

export interface VaultQueuedSyncOperationUpdate {
  id: string;
  attempts?: number;
  last_error?: string;
}

export interface VaultSyncMetadata {
  state: LocalVaultSyncState;
  updated_at: string;
  queued_operations: VaultQueuedSyncOperation[];
}

export interface LocalVaultSnapshot {
  schema_version: number;
  spaces: VaultSpace[];
  folders: VaultFolder[];
  documents: VaultDocument[];
  assets: VaultAsset[];
  templates: VaultTemplate[];
  workflow_templates?: VaultWorkflowTemplate[];
  document_workflows?: VaultDocumentWorkflow[];
  document_workflow_events?: VaultDocumentWorkflowEvent[];
  property_schemas?: VaultPropertySchema[];
  property_values?: VaultPropertyValue[];
  tags?: VaultTag[];
  doc_tags?: VaultDocTag[];
  search_index?: VaultSearchIndexEntry[];
  sync_state?: VaultSyncMetadata;
}

export interface LocalVaultAdapter {
  provider: SyncProviderKind;
  init(): Promise<void>;
  loadSnapshot(): Promise<LocalVaultSnapshot>;
  saveSnapshot(snapshot: LocalVaultSnapshot): Promise<void>;
  reset(): Promise<void>;
  setSyncState(state: LocalVaultSyncState): Promise<void>;
  enqueueSyncOperation(
    operation: Omit<VaultQueuedSyncOperation, "id" | "queued_at"> &
      Partial<Pick<VaultQueuedSyncOperation, "id" | "queued_at">>,
  ): Promise<VaultQueuedSyncOperation>;
  listQueuedSyncOperations(): Promise<VaultQueuedSyncOperation[]>;
  updateQueuedSyncOperations(updates: VaultQueuedSyncOperationUpdate[]): Promise<void>;
  clearQueuedSyncOperations(operationIds?: string[]): Promise<void>;
  listSpaces(): Promise<VaultSpace[]>;
  getDocument(documentId: string): Promise<VaultDocument | null>;
  saveDocument(document: VaultDocument): Promise<void>;
  deleteDocument(documentId: string): Promise<void>;
  saveAsset(asset: VaultAsset): Promise<void>;
  deleteAsset(assetId: string): Promise<void>;
  listAssets(): Promise<VaultAsset[]>;
  saveTemplate(template: VaultTemplate): Promise<void>;
  deleteTemplate(templateId: string): Promise<void>;
  listTemplates(): Promise<VaultTemplate[]>;
  saveWorkflowTemplate(template: VaultWorkflowTemplate): Promise<void>;
  deleteWorkflowTemplate(templateId: string): Promise<void>;
  listWorkflowTemplates(): Promise<VaultWorkflowTemplate[]>;
  saveDocumentWorkflow(workflow: VaultDocumentWorkflow): Promise<void>;
  deleteDocumentWorkflow(documentId: string): Promise<void>;
  getDocumentWorkflow(documentId: string): Promise<VaultDocumentWorkflow | null>;
  getSyncState(): Promise<LocalVaultSyncState>;
}

function getStorageKey(provider: SyncProviderKind): string {
  return `starlane_local_vault_v${LOCAL_VAULT_SCHEMA_VERSION}_${provider}`;
}

function canUseLocalStorage(): boolean {
  return typeof window !== "undefined" && !!window.localStorage;
}

function createEmptySnapshot(): LocalVaultSnapshot {
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

function normalizeSnapshot(
  snapshot: Partial<LocalVaultSnapshot> | null | undefined,
): LocalVaultSnapshot {
  if (!snapshot || snapshot.schema_version !== LOCAL_VAULT_SCHEMA_VERSION) {
    return createEmptySnapshot();
  }

  return {
    schema_version: LOCAL_VAULT_SCHEMA_VERSION,
    spaces: Array.isArray(snapshot.spaces) ? snapshot.spaces : [],
    folders: Array.isArray(snapshot.folders) ? snapshot.folders : [],
    documents: Array.isArray(snapshot.documents) ? snapshot.documents : [],
    assets: Array.isArray(snapshot.assets) ? snapshot.assets : [],
    templates: Array.isArray(snapshot.templates) ? snapshot.templates : [],
    workflow_templates: Array.isArray(snapshot.workflow_templates)
      ? snapshot.workflow_templates
      : [],
    document_workflows: Array.isArray(snapshot.document_workflows)
      ? snapshot.document_workflows
      : [],
    document_workflow_events: Array.isArray(snapshot.document_workflow_events)
      ? snapshot.document_workflow_events
      : [],
    property_schemas: Array.isArray(snapshot.property_schemas) ? snapshot.property_schemas : [],
    property_values: Array.isArray(snapshot.property_values) ? snapshot.property_values : [],
    tags: Array.isArray(snapshot.tags) ? snapshot.tags : [],
    doc_tags: Array.isArray(snapshot.doc_tags) ? snapshot.doc_tags : [],
    search_index: Array.isArray(snapshot.search_index) ? snapshot.search_index : [],
    sync_state:
      snapshot.sync_state && isLocalVaultSyncState(snapshot.sync_state.state)
        ? {
            state: snapshot.sync_state.state,
            updated_at: snapshot.sync_state.updated_at ?? new Date().toISOString(),
            queued_operations: normalizeQueuedSyncOperations(
              snapshot.sync_state.queued_operations,
            ),
          }
        : {
            state: "idle",
            updated_at: new Date().toISOString(),
            queued_operations: [],
          },
  };
}

function isLocalVaultSyncState(value: unknown): value is LocalVaultSyncState {
  return value === "idle" || value === "syncing" || value === "conflict" || value === "error";
}

function normalizeQueuedSyncOperations(value: unknown): VaultQueuedSyncOperation[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((operation): VaultQueuedSyncOperation | null => {
      if (!operation || typeof operation !== "object") return null;
      const candidate = operation as Partial<VaultQueuedSyncOperation>;
      if (
        typeof candidate.entity_type !== "string" ||
        typeof candidate.entity_id !== "string" ||
        typeof candidate.action !== "string"
      ) {
        return null;
      }
      return {
        id: typeof candidate.id === "string" ? candidate.id : makeSyncOperationId(),
        entity_type: candidate.entity_type,
        entity_id: candidate.entity_id,
        action: candidate.action,
        queued_at:
          typeof candidate.queued_at === "string"
            ? candidate.queued_at
            : new Date().toISOString(),
        payload: "payload" in candidate ? candidate.payload : undefined,
        attempts: typeof candidate.attempts === "number" ? candidate.attempts : 0,
        last_error: typeof candidate.last_error === "string" ? candidate.last_error : undefined,
      };
    })
    .filter((operation): operation is VaultQueuedSyncOperation => operation !== null);
}

function makeSyncOperationId(): string {
  return `syncop-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

function hasSnapshotContent(snapshot: LocalVaultSnapshot): boolean {
  return (
    snapshot.spaces.length > 0 ||
    snapshot.folders.length > 0 ||
    snapshot.documents.length > 0 ||
    snapshot.assets.length > 0 ||
    snapshot.templates.length > 0 ||
    (snapshot.workflow_templates?.length ?? 0) > 0 ||
    (snapshot.document_workflows?.length ?? 0) > 0 ||
    (snapshot.document_workflow_events?.length ?? 0) > 0 ||
    (snapshot.property_schemas?.length ?? 0) > 0 ||
    (snapshot.property_values?.length ?? 0) > 0 ||
    (snapshot.tags?.length ?? 0) > 0 ||
    (snapshot.doc_tags?.length ?? 0) > 0
  );
}

export interface LocalVaultStructureCheckResult {
  ok: boolean;
  repairs: string[];
  blockers: string[];
  snapshot: LocalVaultSnapshot;
}

export interface LocalVaultStartupCheckResult {
  provider: SyncProviderKind;
  status: "ok" | "repaired";
  repairs: string[];
  counts: {
    spaces: number;
    folders: number;
    documents: number;
    templates: number;
  };
}

export function repairLocalVaultSnapshotStructure(
  snapshot: LocalVaultSnapshot,
): LocalVaultStructureCheckResult {
  const repairs: string[] = [];
  const blockers: string[] = [];
  const spaceIds = new Set(snapshot.spaces.map((space) => space.id).filter(Boolean));

  const folders = snapshot.folders.map((folder) => ({ ...folder }));
  const folderById = new Map(folders.map((folder) => [folder.id, folder]));

  for (const folder of folders) {
    if (!spaceIds.has(folder.space_id)) {
      blockers.push(`Folder ${folder.id} references missing space ${folder.space_id}`);
      continue;
    }
    if (!folder.parent_id) continue;
    const parent = folderById.get(folder.parent_id);
    if (!parent || parent.space_id !== folder.space_id || folder.parent_id === folder.id) {
      repairs.push(`Detached folder ${folder.id} from invalid parent`);
      folder.parent_id = undefined;
      continue;
    }
    const seen = new Set<string>([folder.id]);
    let ancestorId = parent.parent_id;
    while (ancestorId) {
      if (seen.has(ancestorId)) {
        repairs.push(`Detached folder ${folder.id} from cyclic parent chain`);
        folder.parent_id = undefined;
        break;
      }
      seen.add(ancestorId);
      ancestorId = folderById.get(ancestorId)?.parent_id;
    }
  }

  const normalizedFolderById = new Map(folders.map((folder) => [folder.id, folder]));
  const documents = snapshot.documents.map((document) => ({ ...document }));
  const documentById = new Map(documents.map((document) => [document.id, document]));

  const getDocumentDepth = (documentId: string, seen = new Set<string>()): number => {
    if (seen.has(documentId)) return Number.POSITIVE_INFINITY;
    seen.add(documentId);
    const document = documentById.get(documentId);
    if (!document?.parent_id) return 1;
    return getDocumentDepth(document.parent_id, seen) + 1;
  };

  for (const document of documents) {
    if (!spaceIds.has(document.space_id)) {
      blockers.push(`Document ${document.id} references missing space ${document.space_id}`);
      continue;
    }
    if (document.folder_id) {
      const folder = normalizedFolderById.get(document.folder_id);
      if (!folder || folder.space_id !== document.space_id) {
        repairs.push(`Detached document ${document.id} from invalid folder`);
        document.folder_id = undefined;
      }
    }
    if (!document.parent_id) continue;
    const parent = documentById.get(document.parent_id);
    if (
      !parent ||
      parent.space_id !== document.space_id ||
      parent.folder_id !== document.folder_id ||
      document.parent_id === document.id
    ) {
      repairs.push(`Detached document ${document.id} from invalid parent page`);
      document.parent_id = undefined;
      continue;
    }
    if (getDocumentDepth(parent.id) >= 3) {
      repairs.push(`Detached document ${document.id} from over-depth parent page`);
      document.parent_id = undefined;
    }
  }

  return {
    ok: blockers.length === 0 && repairs.length === 0,
    repairs,
    blockers,
    snapshot: {
      ...snapshot,
      folders,
      documents,
    },
  };
}

export async function runLocalVaultStartupCheck(
  provider: SyncProviderKind,
): Promise<LocalVaultStartupCheckResult> {
  const vault = createLocalVaultAdapter(provider);
  await vault.init();
  const snapshot = await vault.loadSnapshot();
  const checked = repairLocalVaultSnapshotStructure(snapshot);

  if (checked.blockers.length > 0) {
    throw new Error(checked.blockers[0]);
  }

  if (checked.repairs.length > 0) {
    await vault.saveSnapshot(checked.snapshot);
  }

  return {
    provider,
    status: checked.repairs.length > 0 ? "repaired" : "ok",
    repairs: checked.repairs,
    counts: {
      spaces: checked.snapshot.spaces.length,
      folders: checked.snapshot.folders.length,
      documents: checked.snapshot.documents.length,
      templates: checked.snapshot.templates.length,
    },
  };
}

function canUseTauriNativeVault(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

interface NativeRecordMutation {
  type: "upsert" | "delete";
  collection: NativeLocalDatabaseRecordCollection;
  id: string;
  payload?: unknown;
}

interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: TauriCoreModule })
    .__TAURI_INTERNALS__;
  if (internals?.invoke) {
    return internals.invoke<T>(cmd, args);
  }

  const coreModulePath = "@tauri-apps/api/core";
  const coreMod = (await import(/* @vite-ignore */ coreModulePath)) as unknown as TauriCoreModule;
  return coreMod.invoke<T>(cmd, args);
}

class LocalStorageVaultAdapter implements LocalVaultAdapter {
  provider: SyncProviderKind;
  private storageKey: string;
  private spaces = new Map<string, VaultSpace>();
  private folders = new Map<string, VaultFolder>();
  private documents = new Map<string, VaultDocument>();
  private assets = new Map<string, VaultAsset>();
  private templates = new Map<string, VaultTemplate>();
  private workflowTemplates = new Map<string, VaultWorkflowTemplate>();
  private documentWorkflows = new Map<string, VaultDocumentWorkflow>();
  private documentWorkflowEvents = new Map<string, VaultDocumentWorkflowEvent>();
  private propertySchemas = new Map<string, VaultPropertySchema>();
  private propertyValues = new Map<string, VaultPropertyValue>();
  private tags = new Map<string, VaultTag>();
  private docTags = new Map<string, VaultDocTag>();
  private searchIndex = new Map<string, VaultSearchIndexEntry>();
  private syncState: LocalVaultSyncState = "idle";
  private queuedOperations: VaultQueuedSyncOperation[] = [];
  private initialized = false;

  constructor(provider: SyncProviderKind) {
    this.provider = provider;
    this.storageKey = getStorageKey(provider);
  }

  private readSnapshotFromStorage(): LocalVaultSnapshot {
    if (!canUseLocalStorage()) return createEmptySnapshot();
    const raw = window.localStorage.getItem(this.storageKey);
    if (!raw) return createEmptySnapshot();
    try {
      const parsed = JSON.parse(raw) as Partial<LocalVaultSnapshot>;
      return normalizeSnapshot(parsed);
    } catch {
      return createEmptySnapshot();
    }
  }

  private canUseIndexedDb(): boolean {
    return typeof window !== "undefined" && "indexedDB" in window;
  }

  private openIndexedDb(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const request = window.indexedDB.open(LOCAL_VAULT_DB_NAME, 1);
      request.onupgradeneeded = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains(LOCAL_VAULT_DB_STORE)) {
          db.createObjectStore(LOCAL_VAULT_DB_STORE);
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error ?? new Error("IndexedDB open failed"));
    });
  }

  private async readSnapshotFromIndexedDb(): Promise<LocalVaultSnapshot | null> {
    if (!this.canUseIndexedDb()) return null;
    try {
      const db = await this.openIndexedDb();
      return await new Promise((resolve, reject) => {
        const tx = db.transaction(LOCAL_VAULT_DB_STORE, "readonly");
        const store = tx.objectStore(LOCAL_VAULT_DB_STORE);
        const req = store.get(this.storageKey);
        req.onsuccess = () => {
          const value = req.result as Partial<LocalVaultSnapshot> | undefined;
          resolve(value ? normalizeSnapshot(value) : null);
        };
        req.onerror = () => reject(req.error ?? new Error("IndexedDB read failed"));
      });
    } catch {
      return null;
    }
  }

  private async writeSnapshotToIndexedDb(snapshot: LocalVaultSnapshot): Promise<void> {
    if (!this.canUseIndexedDb()) return;
    const db = await this.openIndexedDb();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(LOCAL_VAULT_DB_STORE, "readwrite");
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error ?? new Error("IndexedDB write failed"));
      tx.objectStore(LOCAL_VAULT_DB_STORE).put(snapshot, this.storageKey);
    });
  }

  private async deleteSnapshotFromIndexedDb(): Promise<void> {
    if (!this.canUseIndexedDb()) return;
    try {
      const db = await this.openIndexedDb();
      await new Promise<void>((resolve, reject) => {
        const tx = db.transaction(LOCAL_VAULT_DB_STORE, "readwrite");
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error ?? new Error("IndexedDB delete failed"));
        tx.objectStore(LOCAL_VAULT_DB_STORE).delete(this.storageKey);
      });
    } catch {
      // best effort
    }
  }

  private writeSnapshotToStorage(snapshot: LocalVaultSnapshot): void {
    if (!canUseLocalStorage()) return;
    window.localStorage.setItem(this.storageKey, JSON.stringify(snapshot));
  }

  private applySnapshot(snapshot: LocalVaultSnapshot): void {
    this.spaces = new Map(snapshot.spaces.map((space) => [space.id, space]));
    this.folders = new Map(snapshot.folders.map((folder) => [folder.id, folder]));
    this.documents = new Map(snapshot.documents.map((doc) => [doc.id, doc]));
    this.assets = new Map(snapshot.assets.map((asset) => [asset.id, asset]));
    this.templates = new Map(snapshot.templates.map((template) => [template.id, template]));
    this.workflowTemplates = new Map(
      (snapshot.workflow_templates ?? []).map((template) => [template.id, template]),
    );
    this.documentWorkflows = new Map(
      (snapshot.document_workflows ?? []).map((workflow) => [workflow.document_id, workflow]),
    );
    this.documentWorkflowEvents = new Map(
      (snapshot.document_workflow_events ?? []).map((event) => [event.id, event]),
    );
    this.propertySchemas = new Map(
      (snapshot.property_schemas ?? []).map((schema) => [schema.id, schema]),
    );
    this.propertyValues = new Map(
      (snapshot.property_values ?? []).map((value) => [value.id, value]),
    );
    this.tags = new Map((snapshot.tags ?? []).map((tag) => [tag.id, tag]));
    this.docTags = new Map(
      (snapshot.doc_tags ?? []).map((docTag) => [`${docTag.document_id}:${docTag.tag_id}`, docTag]),
    );
    this.searchIndex = new Map(
      (snapshot.search_index ?? []).map((entry) => [entry.document_id, entry]),
    );
    this.syncState = snapshot.sync_state?.state ?? "idle";
    this.queuedOperations = snapshot.sync_state?.queued_operations ?? [];
  }

  async init(): Promise<void> {
    if (this.initialized) return;
    const indexed = await this.readSnapshotFromIndexedDb();
    const snapshot = indexed ?? this.readSnapshotFromStorage();
    this.applySnapshot(snapshot);
    this.initialized = true;
  }

  async loadSnapshot(): Promise<LocalVaultSnapshot> {
    await this.init();
    return {
      schema_version: LOCAL_VAULT_SCHEMA_VERSION,
      spaces: Array.from(this.spaces.values()),
      folders: Array.from(this.folders.values()),
      documents: Array.from(this.documents.values()),
      assets: Array.from(this.assets.values()),
      templates: Array.from(this.templates.values()),
      workflow_templates: Array.from(this.workflowTemplates.values()),
      document_workflows: Array.from(this.documentWorkflows.values()),
      document_workflow_events: Array.from(this.documentWorkflowEvents.values()),
      property_schemas: Array.from(this.propertySchemas.values()),
      property_values: Array.from(this.propertyValues.values()),
      tags: Array.from(this.tags.values()),
      doc_tags: Array.from(this.docTags.values()),
      search_index: Array.from(this.searchIndex.values()),
      sync_state: {
        state: this.syncState,
        updated_at: new Date().toISOString(),
        queued_operations: this.queuedOperations,
      },
    };
  }

  async saveSnapshot(snapshot: LocalVaultSnapshot): Promise<void> {
    await this.init();
    this.syncState = "syncing";
    try {
      const normalized: LocalVaultSnapshot = {
        schema_version: LOCAL_VAULT_SCHEMA_VERSION,
        spaces: snapshot.spaces,
        folders: snapshot.folders,
        documents: snapshot.documents,
        assets: snapshot.assets,
        templates: snapshot.templates,
        workflow_templates: snapshot.workflow_templates ?? [],
        document_workflows: snapshot.document_workflows ?? [],
        document_workflow_events: snapshot.document_workflow_events ?? [],
        property_schemas: snapshot.property_schemas ?? [],
        property_values: snapshot.property_values ?? [],
        tags: snapshot.tags ?? [],
        doc_tags: snapshot.doc_tags ?? [],
        search_index: snapshot.search_index ?? [],
        sync_state: snapshot.sync_state ?? {
          state: "idle",
          updated_at: new Date().toISOString(),
          queued_operations: [],
        },
      };
      this.applySnapshot(normalized);
      await this.writeSnapshotToIndexedDb(normalized);
      this.writeSnapshotToStorage(normalized);
      this.syncState = normalized.sync_state?.state ?? "idle";
    } catch {
      this.syncState = "error";
      throw new Error("Failed to persist local vault snapshot");
    }
  }

  async reset(): Promise<void> {
    this.spaces.clear();
    this.folders.clear();
    this.documents.clear();
    this.assets.clear();
    this.templates.clear();
    this.workflowTemplates.clear();
    this.documentWorkflows.clear();
    this.documentWorkflowEvents.clear();
    this.propertySchemas.clear();
    this.propertyValues.clear();
    this.tags.clear();
    this.docTags.clear();
    this.searchIndex.clear();
    this.syncState = "idle";
    this.queuedOperations = [];
    this.initialized = false;
    await this.deleteSnapshotFromIndexedDb();
    if (canUseLocalStorage()) {
      window.localStorage.removeItem(this.storageKey);
    }
  }

  async listSpaces(): Promise<VaultSpace[]> {
    await this.init();
    return Array.from(this.spaces.values());
  }

  async getDocument(documentId: string): Promise<VaultDocument | null> {
    await this.init();
    return this.documents.get(documentId) ?? null;
  }

  async saveDocument(document: VaultDocument): Promise<void> {
    await this.init();
    this.documents.set(document.id, document);
    await this.saveSnapshot(await this.loadSnapshot());
  }

  async deleteDocument(documentId: string): Promise<void> {
    await this.init();
    this.documents.delete(documentId);
    await this.saveSnapshot(await this.loadSnapshot());
  }

  async saveAsset(asset: VaultAsset): Promise<void> {
    await this.init();
    this.assets.set(asset.id, asset);
    await this.saveSnapshot(await this.loadSnapshot());
  }

  async deleteAsset(assetId: string): Promise<void> {
    await this.init();
    this.assets.delete(assetId);
    await this.saveSnapshot(await this.loadSnapshot());
  }

  async listAssets(): Promise<VaultAsset[]> {
    await this.init();
    return Array.from(this.assets.values());
  }

  async saveTemplate(template: VaultTemplate): Promise<void> {
    await this.init();
    this.templates.set(template.id, template);
    await this.saveSnapshot(await this.loadSnapshot());
  }

  async deleteTemplate(templateId: string): Promise<void> {
    await this.init();
    this.templates.delete(templateId);
    await this.saveSnapshot(await this.loadSnapshot());
  }

  async listTemplates(): Promise<VaultTemplate[]> {
    await this.init();
    return Array.from(this.templates.values());
  }

  async saveWorkflowTemplate(template: VaultWorkflowTemplate): Promise<void> {
    await this.init();
    this.workflowTemplates.set(template.id, template);
    await this.saveSnapshot(await this.loadSnapshot());
  }

  async deleteWorkflowTemplate(templateId: string): Promise<void> {
    await this.init();
    this.workflowTemplates.delete(templateId);
    await this.saveSnapshot(await this.loadSnapshot());
  }

  async listWorkflowTemplates(): Promise<VaultWorkflowTemplate[]> {
    await this.init();
    return Array.from(this.workflowTemplates.values());
  }

  async saveDocumentWorkflow(workflow: VaultDocumentWorkflow): Promise<void> {
    await this.init();
    this.documentWorkflows.set(workflow.document_id, workflow);
    await this.saveSnapshot(await this.loadSnapshot());
  }

  async deleteDocumentWorkflow(documentId: string): Promise<void> {
    await this.init();
    this.documentWorkflows.delete(documentId);
    await this.saveSnapshot(await this.loadSnapshot());
  }

  async getDocumentWorkflow(documentId: string): Promise<VaultDocumentWorkflow | null> {
    await this.init();
    return this.documentWorkflows.get(documentId) ?? null;
  }

  async getSyncState(): Promise<LocalVaultSyncState> {
    await this.init();
    return this.syncState;
  }

  async setSyncState(state: LocalVaultSyncState): Promise<void> {
    await this.init();
    this.syncState = state;
    const snapshot = await this.loadSnapshot();
    snapshot.sync_state = {
      state,
      updated_at: new Date().toISOString(),
      queued_operations: this.queuedOperations,
    };
    await this.writeSnapshotToIndexedDb(snapshot);
    this.writeSnapshotToStorage(snapshot);
  }

  async enqueueSyncOperation(
    operation: Omit<VaultQueuedSyncOperation, "id" | "queued_at"> &
      Partial<Pick<VaultQueuedSyncOperation, "id" | "queued_at">>,
  ): Promise<VaultQueuedSyncOperation> {
    await this.init();
    const queued: VaultQueuedSyncOperation = {
      id: operation.id ?? makeSyncOperationId(),
      entity_type: operation.entity_type,
      entity_id: operation.entity_id,
      action: operation.action,
      queued_at: operation.queued_at ?? new Date().toISOString(),
      payload: operation.payload,
      attempts: operation.attempts ?? 0,
      last_error: operation.last_error,
    };
    this.queuedOperations = [...this.queuedOperations, queued];
    const snapshot = await this.loadSnapshot();
    await this.writeSnapshotToIndexedDb(snapshot);
    this.writeSnapshotToStorage(snapshot);
    return queued;
  }

  async listQueuedSyncOperations(): Promise<VaultQueuedSyncOperation[]> {
    await this.init();
    return [...this.queuedOperations];
  }

  async updateQueuedSyncOperations(updates: VaultQueuedSyncOperationUpdate[]): Promise<void> {
    await this.init();
    const updateById = new Map(updates.map((update) => [update.id, update]));
    this.queuedOperations = this.queuedOperations.map((operation) => {
      const update = updateById.get(operation.id);
      if (!update) return operation;
      return {
        ...operation,
        attempts: update.attempts ?? operation.attempts,
        last_error: update.last_error,
      };
    });
    const snapshot = await this.loadSnapshot();
    await this.writeSnapshotToIndexedDb(snapshot);
    this.writeSnapshotToStorage(snapshot);
  }

  async clearQueuedSyncOperations(operationIds?: string[]): Promise<void> {
    await this.init();
    if (operationIds && operationIds.length > 0) {
      const ids = new Set(operationIds);
      this.queuedOperations = this.queuedOperations.filter((operation) => !ids.has(operation.id));
    } else {
      this.queuedOperations = [];
    }
    const snapshot = await this.loadSnapshot();
    await this.writeSnapshotToIndexedDb(snapshot);
    this.writeSnapshotToStorage(snapshot);
  }
}

function recordMutationId(collection: NativeLocalDatabaseRecordCollection, value: unknown): string {
  const record = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  if (collection === "doc_tags") {
    return `${String(record.document_id ?? "document")}:${String(record.tag_id ?? "tag")}`;
  }
  if (collection === "search_index") {
    return String(record.document_id ?? "");
  }
  if (collection === "document_workflows") {
    return String(record.id ?? record.document_id ?? "");
  }
  if (collection === "property_values") {
    return String(record.id ?? `${String(record.document_id ?? "document")}:${String(record.property_id ?? "property")}`);
  }
  return String(record.id ?? "");
}

function snapshotMutationEntries(
  snapshot: LocalVaultSnapshot,
): Array<{
  collection: NativeLocalDatabaseRecordCollection;
  id: string;
  payload: unknown;
}> {
  const entries: Array<{
    collection: NativeLocalDatabaseRecordCollection;
    id: string;
    payload: unknown;
  }> = [];
  const append = <T>(
    collection: NativeLocalDatabaseRecordCollection,
    values: T[] | undefined,
  ) => {
    for (const value of values ?? []) {
      const id = recordMutationId(collection, value);
      if (id) entries.push({ collection, id, payload: value });
    }
  };

  append("spaces", snapshot.spaces);
  append("folders", snapshot.folders);
  append("documents", snapshot.documents);
  append("assets", snapshot.assets);
  append("templates", snapshot.templates);
  append("workflow_templates", snapshot.workflow_templates);
  append("document_workflows", snapshot.document_workflows);
  append("document_workflow_events", snapshot.document_workflow_events);
  append("property_schemas", snapshot.property_schemas);
  append("property_values", snapshot.property_values);
  append("tags", snapshot.tags);
  append("doc_tags", snapshot.doc_tags);
  append("search_index", snapshot.search_index);
  if (snapshot.sync_state) {
    entries.push({ collection: "sync_state", id: "sync_state", payload: snapshot.sync_state });
    append("queued_operations", snapshot.sync_state.queued_operations);
  }

  return entries;
}

function buildSnapshotRecordMutations(
  previous: LocalVaultSnapshot,
  next: LocalVaultSnapshot,
): NativeRecordMutation[] {
  const previousEntries = snapshotMutationEntries(previous);
  const nextEntries = snapshotMutationEntries(next);
  const previousByKey = new Map(
    previousEntries.map((entry) => [`${entry.collection}:${entry.id}`, entry]),
  );
  const nextByKey = new Map(nextEntries.map((entry) => [`${entry.collection}:${entry.id}`, entry]));
  const mutations: NativeRecordMutation[] = [];

  for (const [key, entry] of nextByKey) {
    const previousEntry = previousByKey.get(key);
    if (JSON.stringify(previousEntry?.payload) !== JSON.stringify(entry.payload)) {
      mutations.push({
        type: "upsert",
        collection: entry.collection,
        id: entry.id,
        payload: entry.payload,
      });
    }
  }

  for (const [key, entry] of previousByKey) {
    if (!nextByKey.has(key)) {
      mutations.push({ type: "delete", collection: entry.collection, id: entry.id });
    }
  }

  return mutations;
}

class TauriVaultAdapter implements LocalVaultAdapter {
  provider: SyncProviderKind;
  private fallback: LocalStorageVaultAdapter;
  private spaces = new Map<string, VaultSpace>();
  private folders = new Map<string, VaultFolder>();
  private documents = new Map<string, VaultDocument>();
  private assets = new Map<string, VaultAsset>();
  private templates = new Map<string, VaultTemplate>();
  private workflowTemplates = new Map<string, VaultWorkflowTemplate>();
  private documentWorkflows = new Map<string, VaultDocumentWorkflow>();
  private documentWorkflowEvents = new Map<string, VaultDocumentWorkflowEvent>();
  private propertySchemas = new Map<string, VaultPropertySchema>();
  private propertyValues = new Map<string, VaultPropertyValue>();
  private tags = new Map<string, VaultTag>();
  private docTags = new Map<string, VaultDocTag>();
  private searchIndex = new Map<string, VaultSearchIndexEntry>();
  private syncState: LocalVaultSyncState = "idle";
  private queuedOperations: VaultQueuedSyncOperation[] = [];
  private initialized = false;

  constructor(provider: SyncProviderKind) {
    this.provider = provider;
    this.fallback = new LocalStorageVaultAdapter(provider);
  }

  private applySnapshot(snapshot: LocalVaultSnapshot): void {
    this.spaces = new Map(snapshot.spaces.map((space) => [space.id, space]));
    this.folders = new Map(snapshot.folders.map((folder) => [folder.id, folder]));
    this.documents = new Map(snapshot.documents.map((doc) => [doc.id, doc]));
    this.assets = new Map(snapshot.assets.map((asset) => [asset.id, asset]));
    this.templates = new Map(snapshot.templates.map((template) => [template.id, template]));
    this.workflowTemplates = new Map(
      (snapshot.workflow_templates ?? []).map((template) => [template.id, template]),
    );
    this.documentWorkflows = new Map(
      (snapshot.document_workflows ?? []).map((workflow) => [workflow.document_id, workflow]),
    );
    this.documentWorkflowEvents = new Map(
      (snapshot.document_workflow_events ?? []).map((event) => [event.id, event]),
    );
    this.propertySchemas = new Map(
      (snapshot.property_schemas ?? []).map((schema) => [schema.id, schema]),
    );
    this.propertyValues = new Map(
      (snapshot.property_values ?? []).map((value) => [value.id, value]),
    );
    this.tags = new Map((snapshot.tags ?? []).map((tag) => [tag.id, tag]));
    this.docTags = new Map(
      (snapshot.doc_tags ?? []).map((docTag) => [`${docTag.document_id}:${docTag.tag_id}`, docTag]),
    );
    this.searchIndex = new Map(
      (snapshot.search_index ?? []).map((entry) => [entry.document_id, entry]),
    );
    this.syncState = snapshot.sync_state?.state ?? "idle";
    this.queuedOperations = snapshot.sync_state?.queued_operations ?? [];
  }

  private buildSnapshot(): LocalVaultSnapshot {
    return {
      schema_version: LOCAL_VAULT_SCHEMA_VERSION,
      spaces: Array.from(this.spaces.values()),
      folders: Array.from(this.folders.values()),
      documents: Array.from(this.documents.values()),
      assets: Array.from(this.assets.values()),
      templates: Array.from(this.templates.values()),
      workflow_templates: Array.from(this.workflowTemplates.values()),
      document_workflows: Array.from(this.documentWorkflows.values()),
      document_workflow_events: Array.from(this.documentWorkflowEvents.values()),
      property_schemas: Array.from(this.propertySchemas.values()),
      property_values: Array.from(this.propertyValues.values()),
      tags: Array.from(this.tags.values()),
      doc_tags: Array.from(this.docTags.values()),
      search_index: Array.from(this.searchIndex.values()),
      sync_state: {
        state: this.syncState,
        updated_at: new Date().toISOString(),
        queued_operations: this.queuedOperations,
      },
    };
  }

  private async readNativeSnapshot(): Promise<LocalVaultSnapshot | null> {
    const descriptor = await this.describeNativeLocalDatabase();
    if (descriptor?.supports_snapshot_bridge) {
      const snapshot = await invokeTauri<Partial<LocalVaultSnapshot> | null>(
        NATIVE_LOCAL_DATABASE_COMMANDS.readSnapshot,
        { provider: this.provider },
      );
      return snapshot ? normalizeSnapshot(snapshot) : null;
    }

    const snapshot = await invokeTauri<Partial<LocalVaultSnapshot> | null>("read_vault_snapshot", {
      provider: this.provider,
    });
    return snapshot ? normalizeSnapshot(snapshot) : null;
  }

  private async writeNativeSnapshot(snapshot: LocalVaultSnapshot): Promise<void> {
    const descriptor = await this.describeNativeLocalDatabase();
    if (descriptor?.supports_snapshot_bridge) {
      await invokeTauri<void>(NATIVE_LOCAL_DATABASE_COMMANDS.writeSnapshot, {
        provider: this.provider,
        snapshot,
      });
      return;
    }

    await invokeTauri<void>("write_vault_snapshot", {
      provider: this.provider,
      snapshot,
    });
  }

  private async upsertNativeRecord(
    collection: NativeLocalDatabaseRecordCollection,
    id: string,
    payload: unknown,
    rowOrder?: number,
  ): Promise<void> {
    await invokeTauri<void>(NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord, {
      provider: this.provider,
      collection,
      id,
      payload,
      rowOrder,
    });
  }

  private async deleteNativeRecord(
    collection: NativeLocalDatabaseRecordCollection,
    id: string,
  ): Promise<void> {
    await invokeTauri<void>(NATIVE_LOCAL_DATABASE_COMMANDS.deleteRecord, {
      provider: this.provider,
      collection,
      id,
    });
  }

  private async saveNativeRecordWithSnapshotMirror(
    collection: NativeLocalDatabaseRecordCollection,
    id: string,
    payload: unknown,
    snapshot: LocalVaultSnapshot,
    rowOrder?: number,
  ): Promise<void> {
    try {
      await this.upsertNativeRecord(collection, id, payload, rowOrder);
    } catch {
      await this.writeNativeSnapshot(snapshot);
      await this.fallback.saveSnapshot(snapshot);
      return;
    }
    await this.fallback.saveSnapshot(snapshot);
  }

  private async deleteNativeRecordWithSnapshotMirror(
    collection: NativeLocalDatabaseRecordCollection,
    id: string,
    snapshot: LocalVaultSnapshot,
  ): Promise<void> {
    try {
      await this.deleteNativeRecord(collection, id);
    } catch {
      await this.writeNativeSnapshot(snapshot);
      await this.fallback.saveSnapshot(snapshot);
      return;
    }
    await this.fallback.saveSnapshot(snapshot);
  }

  private async resetNativeStore(): Promise<void> {
    const descriptor = await this.describeNativeLocalDatabase();
    if (descriptor?.supports_snapshot_bridge) {
      await invokeTauri<void>(NATIVE_LOCAL_DATABASE_COMMANDS.reset, { provider: this.provider });
      return;
    }

    await invokeTauri<void>("reset_vault_snapshot", { provider: this.provider });
  }

  private async describeNativeLocalDatabase(): Promise<NativeLocalDatabaseDescriptor | null> {
    try {
      return await invokeTauri<NativeLocalDatabaseDescriptor>(
        NATIVE_LOCAL_DATABASE_COMMANDS.describe,
        { provider: this.provider },
      );
    } catch {
      return null;
    }
  }

  async init(): Promise<void> {
    if (this.initialized) return;

    try {
      const nativeSnapshot = await this.readNativeSnapshot();
      if (nativeSnapshot) {
        this.applySnapshot(nativeSnapshot);
      } else {
        await this.fallback.init();
        const fallbackSnapshot = await this.fallback.loadSnapshot();
        this.applySnapshot(fallbackSnapshot);
        if (hasSnapshotContent(fallbackSnapshot)) {
          await this.writeNativeSnapshot(fallbackSnapshot);
        }
      }
      this.initialized = true;
      return;
    } catch {
      await this.fallback.init();
      this.applySnapshot(await this.fallback.loadSnapshot());
      this.syncState = "error";
      this.initialized = true;
    }
  }

  async loadSnapshot(): Promise<LocalVaultSnapshot> {
    await this.init();
    return this.buildSnapshot();
  }

  async saveSnapshot(snapshot: LocalVaultSnapshot): Promise<void> {
    await this.init();
    this.syncState = "syncing";
    const previous = this.buildSnapshot();
    const normalized = normalizeSnapshot(snapshot);
    try {
      this.applySnapshot(normalized);
      const mutations = buildSnapshotRecordMutations(previous, normalized);
      let needsNativeSnapshotBridge = true;
      try {
        for (const mutation of mutations) {
          if (mutation.type === "upsert") {
            await this.upsertNativeRecord(mutation.collection, mutation.id, mutation.payload);
          } else {
            await this.deleteNativeRecord(mutation.collection, mutation.id);
          }
        }
        needsNativeSnapshotBridge = false;
      } catch {
        // Older native shells still persist through the full snapshot bridge below.
      }
      if (needsNativeSnapshotBridge) {
        await this.writeNativeSnapshot(normalized);
      }
      await this.fallback.saveSnapshot(normalized);
      this.syncState = normalized.sync_state?.state ?? "idle";
    } catch {
      this.syncState = "error";
      throw new Error("Failed to persist native vault snapshot");
    }
  }

  async reset(): Promise<void> {
    this.spaces.clear();
    this.folders.clear();
    this.documents.clear();
    this.assets.clear();
    this.templates.clear();
    this.workflowTemplates.clear();
    this.documentWorkflows.clear();
    this.documentWorkflowEvents.clear();
    this.propertySchemas.clear();
    this.propertyValues.clear();
    this.tags.clear();
    this.docTags.clear();
    this.searchIndex.clear();
    this.syncState = "idle";
    this.queuedOperations = [];
    this.initialized = false;
    await this.resetNativeStore();
    await this.fallback.reset();
  }

  async listSpaces(): Promise<VaultSpace[]> {
    await this.init();
    return Array.from(this.spaces.values());
  }

  async getDocument(documentId: string): Promise<VaultDocument | null> {
    await this.init();
    return this.documents.get(documentId) ?? null;
  }

  async saveDocument(document: VaultDocument): Promise<void> {
    await this.init();
    this.documents.set(document.id, document);
    await this.saveNativeRecordWithSnapshotMirror(
      "documents",
      document.id,
      document,
      this.buildSnapshot(),
    );
  }

  async deleteDocument(documentId: string): Promise<void> {
    await this.init();
    this.documents.delete(documentId);
    await this.deleteNativeRecordWithSnapshotMirror("documents", documentId, this.buildSnapshot());
  }

  async saveAsset(asset: VaultAsset): Promise<void> {
    await this.init();
    this.assets.set(asset.id, asset);
    await this.saveNativeRecordWithSnapshotMirror("assets", asset.id, asset, this.buildSnapshot());
  }

  async deleteAsset(assetId: string): Promise<void> {
    await this.init();
    this.assets.delete(assetId);
    await this.deleteNativeRecordWithSnapshotMirror("assets", assetId, this.buildSnapshot());
  }

  async listAssets(): Promise<VaultAsset[]> {
    await this.init();
    return Array.from(this.assets.values());
  }

  async saveTemplate(template: VaultTemplate): Promise<void> {
    await this.init();
    this.templates.set(template.id, template);
    await this.saveNativeRecordWithSnapshotMirror(
      "templates",
      template.id,
      template,
      this.buildSnapshot(),
    );
  }

  async deleteTemplate(templateId: string): Promise<void> {
    await this.init();
    this.templates.delete(templateId);
    await this.deleteNativeRecordWithSnapshotMirror("templates", templateId, this.buildSnapshot());
  }

  async listTemplates(): Promise<VaultTemplate[]> {
    await this.init();
    return Array.from(this.templates.values());
  }

  async saveWorkflowTemplate(template: VaultWorkflowTemplate): Promise<void> {
    await this.init();
    this.workflowTemplates.set(template.id, template);
    await this.saveNativeRecordWithSnapshotMirror(
      "workflow_templates",
      template.id,
      template,
      this.buildSnapshot(),
    );
  }

  async deleteWorkflowTemplate(templateId: string): Promise<void> {
    await this.init();
    this.workflowTemplates.delete(templateId);
    await this.deleteNativeRecordWithSnapshotMirror(
      "workflow_templates",
      templateId,
      this.buildSnapshot(),
    );
  }

  async listWorkflowTemplates(): Promise<VaultWorkflowTemplate[]> {
    await this.init();
    return Array.from(this.workflowTemplates.values());
  }

  async saveDocumentWorkflow(workflow: VaultDocumentWorkflow): Promise<void> {
    await this.init();
    this.documentWorkflows.set(workflow.document_id, workflow);
    await this.saveNativeRecordWithSnapshotMirror(
      "document_workflows",
      workflow.id ?? workflow.document_id,
      workflow,
      this.buildSnapshot(),
    );
  }

  async deleteDocumentWorkflow(documentId: string): Promise<void> {
    await this.init();
    const workflowId = this.documentWorkflows.get(documentId)?.id ?? documentId;
    this.documentWorkflows.delete(documentId);
    await this.deleteNativeRecordWithSnapshotMirror(
      "document_workflows",
      workflowId,
      this.buildSnapshot(),
    );
  }

  async getDocumentWorkflow(documentId: string): Promise<VaultDocumentWorkflow | null> {
    await this.init();
    return this.documentWorkflows.get(documentId) ?? null;
  }

  async getSyncState(): Promise<LocalVaultSyncState> {
    await this.init();
    return this.syncState;
  }

  async setSyncState(state: LocalVaultSyncState): Promise<void> {
    await this.init();
    this.syncState = state;
    const snapshot = this.buildSnapshot();
    await this.saveNativeRecordWithSnapshotMirror(
      "sync_state",
      this.provider,
      snapshot.sync_state,
      snapshot,
    );
  }

  async enqueueSyncOperation(
    operation: Omit<VaultQueuedSyncOperation, "id" | "queued_at"> &
      Partial<Pick<VaultQueuedSyncOperation, "id" | "queued_at">>,
  ): Promise<VaultQueuedSyncOperation> {
    await this.init();
    const queued: VaultQueuedSyncOperation = {
      id: operation.id ?? makeSyncOperationId(),
      entity_type: operation.entity_type,
      entity_id: operation.entity_id,
      action: operation.action,
      queued_at: operation.queued_at ?? new Date().toISOString(),
      payload: operation.payload,
      attempts: operation.attempts ?? 0,
      last_error: operation.last_error,
    };
    this.queuedOperations = [...this.queuedOperations, queued];
    const snapshot = this.buildSnapshot();
    await this.saveNativeRecordWithSnapshotMirror(
      "queued_operations",
      queued.id,
      queued,
      snapshot,
    );
    return queued;
  }

  async listQueuedSyncOperations(): Promise<VaultQueuedSyncOperation[]> {
    await this.init();
    return [...this.queuedOperations];
  }

  async updateQueuedSyncOperations(updates: VaultQueuedSyncOperationUpdate[]): Promise<void> {
    await this.init();
    const updateById = new Map(updates.map((update) => [update.id, update]));
    this.queuedOperations = this.queuedOperations.map((operation) => {
      const update = updateById.get(operation.id);
      if (!update) return operation;
      return {
        ...operation,
        attempts: update.attempts ?? operation.attempts,
        last_error: update.last_error,
      };
    });
    const snapshot = this.buildSnapshot();
    let needsNativeSnapshotBridge = true;
    try {
      await Promise.all(
        this.queuedOperations
          .filter((operation) => updateById.has(operation.id))
          .map((operation) =>
            this.upsertNativeRecord("queued_operations", operation.id, operation),
          ),
      );
      needsNativeSnapshotBridge = false;
    } catch {
      // Fall back to the full snapshot write below.
    }
    if (needsNativeSnapshotBridge) {
      await this.writeNativeSnapshot(snapshot);
    }
    await this.fallback.saveSnapshot(snapshot);
  }

  async clearQueuedSyncOperations(operationIds?: string[]): Promise<void> {
    await this.init();
    const operationIdsToDelete =
      operationIds && operationIds.length > 0
        ? operationIds
        : this.queuedOperations.map((operation) => operation.id);
    if (operationIds && operationIds.length > 0) {
      const ids = new Set(operationIds);
      this.queuedOperations = this.queuedOperations.filter((operation) => !ids.has(operation.id));
    } else {
      this.queuedOperations = [];
    }
    const snapshot = this.buildSnapshot();
    let needsNativeSnapshotBridge = true;
    try {
      await Promise.all([
        ...operationIdsToDelete.map((operationId) =>
          this.deleteNativeRecord("queued_operations", operationId),
        ),
        this.upsertNativeRecord("sync_state", this.provider, snapshot.sync_state),
      ]);
      needsNativeSnapshotBridge = false;
    } catch {
      // Fall back to the full snapshot write below.
    }
    if (needsNativeSnapshotBridge) {
      await this.writeNativeSnapshot(snapshot);
    }
    await this.fallback.saveSnapshot(snapshot);
  }
}

export function createLocalVaultAdapter(provider: SyncProviderKind): LocalVaultAdapter {
  if (canUseTauriNativeVault()) {
    return new TauriVaultAdapter(provider);
  }
  return new LocalStorageVaultAdapter(provider);
}
