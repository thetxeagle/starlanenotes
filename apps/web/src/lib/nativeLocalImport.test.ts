import { describe, expect, it } from "vitest";
import { checkedNativeLocalImportAcknowledgement } from "./nativeLocalImport";

const operationId = "11111111-1111-4111-8111-111111111111";
const root = "/original/vault";
const request = {
  operation_id: operationId,
  name: "same-name.txt",
  mime_type: "text/plain",
  byte_length: 3,
  space_id: "space",
  document_id: "page",
  draft_id: "draft",
  draft_created_at: "2026-10-18T01:00:00Z",
  original_binding: {
    context: {
      version: 1 as const,
      serverBase: "local-vault",
      accountId: "local-user",
      sessionId: operationId,
      cacheScope: "local",
      storageMode: "local" as const,
      provider: "none" as const,
    },
    root,
    revision: "before",
  },
};
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

function acknowledgement() {
  const vaultPath = `${root}/assets/local-imports/asset-import-${operationId}.bin`;
  const asset = {
    id: `asset-import-${operationId}`,
    name: request.name,
    mime_type: request.mime_type,
    size: request.byte_length,
    url: `asset://localhost/${encodeURIComponent(vaultPath)}`,
    vault_path: vaultPath,
    source: "tauri" as const,
    space_id: request.space_id,
    document_id: request.document_id,
    created_at: "2026-10-18T02:00:00Z",
  };
  const queued = {
    id: `syncop-import-${operationId}`,
    entity_type: "asset",
    entity_id: asset.id,
    action: "create",
    queued_at: asset.created_at,
    attempts: 0,
    payload: {
      ...asset,
      draft_id: request.draft_id,
      local_draft_source: {
        schema_version: 1,
        space_id: request.space_id,
        document_id: request.document_id,
        draft_id: request.draft_id,
        draft_created_at: request.draft_created_at,
      },
    },
  };
  const receipt = {
    format: 1,
    operation_id: operationId,
    request_sha256: "a".repeat(64),
    request: clone(request),
    creator: {},
    seal_json: JSON.stringify({ format: 1, operation_id: operationId, binary_sha256: "b".repeat(64), asset }),
    seal_file: {},
    creator_directory_stamp: {},
    canonical_revision: "after",
    canonical_committed: true,
    asset,
    queued_operation: queued,
  };
  return {
    format: 1,
    operation_id: operationId,
    canonical_committed: true,
    asset,
    canonical_snapshot: {
      schema_version: 1,
      spaces: [],
      folders: [],
      documents: [],
      drafts: [],
      assets: [asset],
      templates: [],
      sync_state: { state: "idle" as const, updated_at: asset.created_at, queued_operations: [queued] },
      canonical_revision: "after",
      mirror_pending: true,
    },
    receipt_json: JSON.stringify(receipt),
  };
}

describe("native local import acknowledgement", () => {
  it("accepts only the exact native asset, queue, receipt, and canonical snapshot", () => {
    const checked = checkedNativeLocalImportAcknowledgement(acknowledgement(), request);
    expect(checked.asset.id).toBe(`asset-import-${operationId}`);
    expect(checked.operation.id).toBe(`syncop-import-${operationId}`);
  });

  it.each(["asset-id", "asset-extra-authority", "queue-id", "queue-payload", "canonical-asset", "canonical-queue", "receipt-request", "receipt-extra-authority", "vault-path", "extra-authority"])(
    "rejects substituted %s evidence",
    fault => {
      const value = acknowledgement();
      const receipt = JSON.parse(value.receipt_json);
      if (fault === "asset-id") value.asset.id = "asset-foreign";
      if (fault === "asset-extra-authority") Object.assign(value.asset, { retry_authority: true });
      if (fault === "queue-id") receipt.queued_operation.id = "syncop-foreign";
      if (fault === "queue-payload") receipt.queued_operation.payload.draft_id = "draft-foreign";
      if (fault === "canonical-asset") value.canonical_snapshot.assets = [];
      if (fault === "canonical-queue") value.canonical_snapshot.sync_state.queued_operations = [];
      if (fault === "receipt-request") receipt.request.draft_created_at = "foreign";
      if (fault === "receipt-extra-authority") receipt.retry_authority = true;
      if (fault === "vault-path") value.asset.vault_path = "/borrowed/equal-bytes.bin";
      if (fault === "extra-authority") Object.assign(value, { retry_authority: true });
      value.receipt_json = JSON.stringify(receipt);
      expect(() => checkedNativeLocalImportAcknowledgement(value, request)).toThrow();
    },
  );

  it("binds a path import receipt to the inspected source digest", () => {
    const value = acknowledgement();
    const receipt = JSON.parse(value.receipt_json);
    const pathRequest = { ...request, path_source: { schema_version: 1 as const, source_path: "/selected/file", stamp: { length: 3 }, binary_sha256: "c".repeat(64) } };
    receipt.request = pathRequest;
    value.receipt_json = JSON.stringify(receipt);
    expect(() => checkedNativeLocalImportAcknowledgement(value, pathRequest)).toThrow("receipt differs");
  });
});
