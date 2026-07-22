export interface VaultImportPreflightSummary {
  schemaVersion?: number;
  spaceCount: number;
  folderCount: number;
  documentCount: number;
  assetCount: number;
  templateCount: number;
  queuedOperationCount: number;
  syncState?: string;
}

export function summarizeVaultImportSnapshot(value: unknown): VaultImportPreflightSummary {
  if (!value || typeof value !== "object") {
    return {
      spaceCount: 0,
      folderCount: 0,
      documentCount: 0,
      assetCount: 0,
      templateCount: 0,
      queuedOperationCount: 0,
    };
  }
  const snapshot = value as Record<string, unknown>;
  const syncState =
    snapshot.sync_state && typeof snapshot.sync_state === "object"
      ? (snapshot.sync_state as Record<string, unknown>)
      : null;
  return {
    schemaVersion:
      typeof snapshot.schema_version === "number" ? snapshot.schema_version : undefined,
    spaceCount: countArray(snapshot.spaces),
    folderCount: countArray(snapshot.folders),
    documentCount: countArray(snapshot.documents),
    assetCount: countArray(snapshot.assets),
    templateCount: countArray(snapshot.templates),
    queuedOperationCount: syncState ? countArray(syncState.queued_operations) : 0,
    syncState: typeof syncState?.state === "string" ? syncState.state : undefined,
  };
}

export function formatVaultImportPreflight(summary: VaultImportPreflightSummary): string {
  const parts = [
    `${summary.spaceCount} space${summary.spaceCount === 1 ? "" : "s"}`,
    `${summary.documentCount} document${summary.documentCount === 1 ? "" : "s"}`,
    `${summary.folderCount} folder${summary.folderCount === 1 ? "" : "s"}`,
    `${summary.assetCount} asset${summary.assetCount === 1 ? "" : "s"}`,
    `${summary.templateCount} template${summary.templateCount === 1 ? "" : "s"}`,
  ];
  const syncDetail = summary.syncState
    ? `Sync state: ${summary.syncState}; ${summary.queuedOperationCount} queued operation${summary.queuedOperationCount === 1 ? "" : "s"}.`
    : `${summary.queuedOperationCount} queued operation${summary.queuedOperationCount === 1 ? "" : "s"}.`;
  return `Import snapshot contains ${parts.join(", ")}. ${syncDetail}`;
}

function countArray(value: unknown): number {
  return Array.isArray(value) ? value.length : 0;
}
