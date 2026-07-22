import { describe, expect, it } from "vitest";
import {
  buildNativeLocalDatabaseReplayContract,
  NATIVE_LOCAL_DATABASE_COMMANDS,
  type NativeLocalDatabaseRecordUpsertRequest,
  preflightReplayOperationsForContract,
  sortReplayOperationsForContract,
} from "./nativeLocalDatabase";

describe("native local database contracts", () => {
  it("describes server replay as API transport over SQLite or PostgreSQL backends", () => {
    expect(buildNativeLocalDatabaseReplayContract()).toEqual({
      schema_version: 1,
      operation_order: "queued_at_ascending",
      idempotency_key: "operation.id",
      transport: "server-api",
      server_backends: ["sqlite", "postgresql"],
      required_payload_status: "available",
      conflict_policy: "server_validates_version_and_permissions",
      web_canonical_store: "server-api",
    });
  });

  it("sorts replay operations by queued timestamp and stable operation id", () => {
    expect(
      sortReplayOperationsForContract([
        { id: "syncop-c", queued_at: "2026-07-08T12:01:00.000Z" },
        { id: "syncop-b", queued_at: "2026-07-08T12:00:00.000Z" },
        { id: "syncop-a", queued_at: "2026-07-08T12:00:00.000Z" },
      ]).map((operation) => operation.id),
    ).toEqual(["syncop-a", "syncop-b", "syncop-c"]);
  });

  it("marks replay readiness blocked when operation payloads are missing", () => {
    expect(
      preflightReplayOperationsForContract([
        { id: "syncop-ready", payload_status: "available" },
        { id: "syncop-missing", payload_status: "missing" },
      ]),
    ).toEqual({
      ready: false,
      ready_operation_count: 1,
      blocked_operation_count: 1,
      blockers: ["Missing replay payloads: syncop-missing"],
      target_backends: ["sqlite", "postgresql"],
    });
  });

  it("describes direct native record mutation commands", () => {
    expect(NATIVE_LOCAL_DATABASE_COMMANDS.upsertRecord).toBe(
      "upsert_native_local_database_record",
    );
    expect(NATIVE_LOCAL_DATABASE_COMMANDS.deleteRecord).toBe(
      "delete_native_local_database_record",
    );

    const request: NativeLocalDatabaseRecordUpsertRequest = {
      provider: "google-drive",
      collection: "documents",
      id: "doc-1",
      payload: { id: "doc-1", title: "Direct" },
      rowOrder: 0,
    };
    expect(request.collection).toBe("documents");
  });
});
