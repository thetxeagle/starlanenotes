export type ConflictResolutionStrategy = "merge" | "keep-local" | "keep-remote";

export type ConflictStatus = "pending" | "resolved";

export interface ConflictSnapshot {
  title: string;
  content: string;
  updated_at: string;
}

export interface DocumentConflictRecord {
  id: string;
  document_id: string;
  document_title: string;
  space_id: string;
  space_name: string;
  detected_at: string;
  summary: string;
  local_snapshot: ConflictSnapshot;
  remote_snapshot: ConflictSnapshot;
  status: ConflictStatus;
  resolution_strategy?: ConflictResolutionStrategy;
  resolved_at?: string;
}

const CONFLICT_STORAGE_KEY = "starlane_document_conflicts_v1";

function safeLocalStorage(): Storage | null {
  if (typeof window === "undefined") return null;
  return window.localStorage;
}

function makeId(prefix: string): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  return `${prefix}-${Math.random().toString(36).slice(2, 10)}`;
}

function loadAllRecords(): DocumentConflictRecord[] {
  const storage = safeLocalStorage();
  if (!storage) return [];

  const raw = storage.getItem(CONFLICT_STORAGE_KEY);
  if (!raw) return [];

  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(Boolean) as DocumentConflictRecord[];
  } catch {
    return [];
  }
}

function saveAllRecords(records: DocumentConflictRecord[]): void {
  const storage = safeLocalStorage();
  if (!storage) return;
  storage.setItem(CONFLICT_STORAGE_KEY, JSON.stringify(records));
}

export function listConflicts(includeResolved = false): DocumentConflictRecord[] {
  const records = loadAllRecords();
  return includeResolved ? records : records.filter((record) => record.status === "pending");
}

export function hasPendingConflicts(): boolean {
  return loadAllRecords().some((record) => record.status === "pending");
}

export function clearConflicts(): void {
  saveAllRecords([]);
}

export function queueConflict(
  conflict: Omit<DocumentConflictRecord, "id" | "status"> & { id?: string },
): DocumentConflictRecord {
  const records = loadAllRecords();
  const id = conflict.id ?? makeId("conflict");
  const next: DocumentConflictRecord = {
    ...conflict,
    id,
    status: "pending",
  };

  const existingIndex = records.findIndex((record) => record.id === id);
  const duplicateIndex = records.findIndex(
    (record) => record.document_id === conflict.document_id && record.status === "pending",
  );

  if (existingIndex >= 0) {
    records[existingIndex] = next;
  } else if (duplicateIndex >= 0) {
    records[duplicateIndex] = next;
  } else {
    records.push(next);
  }

  saveAllRecords(records);
  return next;
}

export function getConflict(conflictId: string): DocumentConflictRecord | null {
  return loadAllRecords().find((record) => record.id === conflictId) ?? null;
}

export function resolveConflictRecord(
  conflictId: string,
  strategy: ConflictResolutionStrategy,
): DocumentConflictRecord {
  const records = loadAllRecords();
  const index = records.findIndex((record) => record.id === conflictId);
  if (index < 0) {
    throw new Error("Conflict not found");
  }

  const existing = records[index];
  if (existing.status !== "pending") {
    throw new Error("Conflict already resolved");
  }

  const resolved: DocumentConflictRecord = {
    ...existing,
    status: "resolved",
    resolution_strategy: strategy,
    resolved_at: new Date().toISOString(),
  };

  records[index] = resolved;
  saveAllRecords(records);
  return resolved;
}

