import { api, ApiError, captureOfflineServerContext, getApiBase } from "./api";
import {
  isOfflineReplayOrigin,
  sameOfflineReplayOrigin,
  verifyOfflineReplayContext,
  type OfflineReplayContext,
  type OfflineReplayOrigin,
} from "./offlineReplayOrigin";
import { stripTemplateGuidance } from "./templatePublication";
import type { DraftRecord } from "./dataClient";

export interface DraftPublicationSource {
  schema_version: 1;
  space_id: string;
  document_id: string;
  draft_id: string;
  draft_source_digest: string;
  page_source_digest: string;
  page_published: boolean;
  page_updated_at: string;
  draft: { title: string; content: string; icon: string | null; cover_image: string | null };
}
export interface DraftPublicationRequest {
  schema_version: 1;
  operation_id: string;
  draft_source_digest: string;
  page_source_digest: string;
}
export interface DraftPublicationIntent {
  schema_version: 1;
  operation_id: string;
  space_id: string;
  document_id: string;
  draft_id: string;
  origin: OfflineReplayOrigin;
  request: DraftPublicationRequest;
  source: DraftPublicationSource;
  expectedPublication: DraftPublicationSource["draft"];
}
export interface DraftPublicationReceipt extends DraftPublicationRequest {
  space_id: string;
  document_id: string;
  draft_id: string;
  published_source_digest: string;
  published_at: string;
  publication: DraftPublicationSource["draft"];
  disposition: "applied" | "historical";
}
export interface DraftPublicationRecord {
  schema_version: 1;
  intent: DraftPublicationIntent;
  state: "prepared" | "dispatched" | "unknown" | "rejected" | "confirmed" | "cancelled";
  receipt?: DraftPublicationReceipt;
  error?: string;
}
export class DraftPublicationHeldError extends Error {
  constructor(
    public operationId: string,
    message: string,
  ) {
    super(message);
    this.name = "DraftPublicationHeldError";
  }
}
interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  length: number;
  key(index: number): string | null;
}
export interface DraftPublicationTransport {
  storage: StorageLike;
  capture: () => OfflineReplayContext;
  verify: (context: OfflineReplayContext) => void;
  fetch: typeof fetch;
  runExclusive: <T>(name: string, run: () => Promise<T>) => Promise<T>;
  online: () => boolean;
  operationId: () => string;
}
const PREFIX = "starlane_draft_publication_v1:";
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const canonical = (value: unknown): unknown => {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonical((value as Record<string, unknown>)[key])]),
    );
  return value;
};
const equal = (left: unknown, right: unknown) =>
  JSON.stringify(canonical(left)) === JSON.stringify(canonical(right));
const digest = (value: unknown): value is string =>
  typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
const uuid = (value: unknown): value is string =>
  typeof value === "string" &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
const text = (value: unknown): value is string =>
  typeof value === "string" && value.trim().length > 0;
const fields = (value: unknown): value is DraftPublicationSource["draft"] => {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const row = value as DraftPublicationSource["draft"];
  return (
    text(row.title) &&
    typeof row.content === "string" &&
    (row.icon === null || typeof row.icon === "string") &&
    (row.cover_image === null || typeof row.cover_image === "string")
  );
};
function validateSource(
  value: unknown,
  space: string,
  page: string,
  draft: string,
): DraftPublicationSource {
  const source = value as DraftPublicationSource;
  if (
    !source ||
    source.schema_version !== 1 ||
    source.space_id !== space ||
    source.document_id !== page ||
    source.draft_id !== draft ||
    !digest(source.draft_source_digest) ||
    !digest(source.page_source_digest) ||
    typeof source.page_published !== "boolean" ||
    !text(source.page_updated_at) ||
    !fields(source.draft)
  )
    throw new Error("Publication source is unproven; review the original saved draft.");
  return clone(source);
}
function validateIntent(intent: DraftPublicationIntent): void {
  if (
    !intent ||
    intent.schema_version !== 1 ||
    !uuid(intent.operation_id) ||
    !text(intent.space_id) ||
    !text(intent.document_id) ||
    !text(intent.draft_id) ||
    !isOfflineReplayOrigin(intent.origin) ||
    intent.origin.storageMode !== "server" ||
    !intent.request ||
    intent.request.schema_version !== 1 ||
    intent.request.operation_id !== intent.operation_id
  )
    throw new Error("Publication journal identity is unproven; retain it for review.");
  const source = validateSource(
    intent.source,
    intent.space_id,
    intent.document_id,
    intent.draft_id,
  );
  if (
    intent.request.draft_source_digest !== source.draft_source_digest ||
    intent.request.page_source_digest !== source.page_source_digest ||
    !equal(intent.expectedPublication, {
      ...source.draft,
      content: stripTemplateGuidance(source.draft.content),
    })
  )
    throw new Error("Publication journal source changed; retain the original operation.");
}
function receiptFor(value: unknown, intent: DraftPublicationIntent): DraftPublicationReceipt {
  const row = value as DraftPublicationReceipt;
  if (
    !row ||
    row.schema_version !== 1 ||
    row.operation_id !== intent.operation_id ||
    row.space_id !== intent.space_id ||
    row.document_id !== intent.document_id ||
    row.draft_id !== intent.draft_id ||
    row.draft_source_digest !== intent.request.draft_source_digest ||
    row.page_source_digest !== intent.request.page_source_digest ||
    !digest(row.published_source_digest) ||
    !text(row.published_at) ||
    !["applied", "historical"].includes(row.disposition) ||
    !fields(row.publication) ||
    !equal(row.publication, intent.expectedPublication)
  )
    throw new Error(
      "Publication receipt is unproven; retain the original request for explicit retry.",
    );
  return clone(row);
}
const lockKey = (origin: OfflineReplayOrigin, space: string, page: string) =>
  `starlane:publication:${JSON.stringify([origin.serverBase, origin.accountId, space, page])}`;
const route = (space: string, page: string, draft: string) =>
  `/spaces/${encodeURIComponent(space)}/documents/${encodeURIComponent(page)}/drafts/${encodeURIComponent(draft)}`;

/** Publication is explicit and durable; no generic offline queue or new-source retry. */
export function createDocumentDraftPublicationService(transport: DraftPublicationTransport) {
  const records = (): DraftPublicationRecord[] => {
    const keys: string[] = [];
    for (let i = 0; i < transport.storage.length; i += 1) {
      const key = transport.storage.key(i);
      if (key?.startsWith(PREFIX)) keys.push(key);
    }
    return keys.map((key) => {
      let record: DraftPublicationRecord;
      try {
        record = JSON.parse(transport.storage.getItem(key)!);
        validateIntent(record.intent);
      } catch {
        throw new Error("Stored publication request is corrupt; retain it for review.");
      }
      if (
        record.schema_version !== 1 ||
        key !== PREFIX + record.intent.operation_id ||
        !["prepared", "dispatched", "unknown", "rejected", "confirmed", "cancelled"].includes(
          record.state,
        )
      )
        throw new Error("Stored publication evidence is unproven; retain it for review.");
      if (record.receipt) receiptFor(record.receipt, record.intent);
      if (record.state === "confirmed" && !record.receipt)
        throw new Error("Confirmed publication receipt is missing; retain its evidence.");
      return clone(record);
    });
  };
  const read = (id: string) => {
    const record = records().find((row) => row.intent.operation_id === id);
    if (!record)
      throw new Error(
        "Original publication request is unavailable; retain this workspace for review.",
      );
    return record;
  };
  const write = (next: DraftPublicationRecord, previous: DraftPublicationRecord | null) => {
    const key = PREFIX + next.intent.operation_id;
    if (transport.storage.getItem(key) !== (previous ? JSON.stringify(previous) : null))
      throw new Error("Publication evidence changed; no replacement request was dispatched.");
    const raw = JSON.stringify(next);
    transport.storage.setItem(key, raw);
    if (transport.storage.getItem(key) !== raw)
      throw new Error("Could not retain publication evidence; review the original request.");
    return clone(next);
  };
  const current = (origin: OfflineReplayOrigin) => {
    const context = transport.capture();
    if (!sameOfflineReplayOrigin(context.origin, origin))
      throw new Error(
        "Publication belongs to its original account, session and server; retain it for review.",
      );
    transport.verify(context);
    return context;
  };
  const request = async (
    context: OfflineReplayContext,
    path: string,
    method: "GET" | "POST",
    body?: DraftPublicationRequest,
  ): Promise<unknown> => {
    transport.verify(context);
    const response = await transport.fetch(`${context.origin.serverBase}${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${context.bearer}`,
        ...(body ? { "Content-Type": "application/json" } : {}),
      },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    transport.verify(context);
    const raw = await response.text();
    transport.verify(context);
    if (response.status !== 200) {
      if (!response.ok) {
        let error: { message?: string; error?: string } = {};
        try {
          error = JSON.parse(raw);
        } catch {
          /* Status remains authoritative. */
        }
        throw new ApiError(
          typeof error.message === "string"
            ? error.message
            : `Publication returned ${response.status}`,
          response.status,
          typeof error.error === "string" ? error.error : undefined,
        );
      }
      throw new Error(`Publication response ${response.status} is not a confirmed receipt.`);
    }
    const decoded: unknown = JSON.parse(raw);
    transport.verify(context);
    return decoded;
  };
  const dispatch = (input: DraftPublicationIntent, retry: boolean) => {
    const intent = clone(input);
    validateIntent(intent);
    return transport.runExclusive(
      lockKey(intent.origin, intent.space_id, intent.document_id),
      async () => {
        const context = current(intent.origin);
        let record = read(intent.operation_id);
        if (!equal(record.intent, intent))
          throw new Error("Publication intent changed; keep the original request.");
        if (record.state === "cancelled" || record.state === "rejected")
          throw new DraftPublicationHeldError(
            intent.operation_id,
            "This publication was cancelled or rejected. Review the current saved source before preparing a new request.",
          );
        if (!retry && record.state !== "prepared")
          throw new DraftPublicationHeldError(
            intent.operation_id,
            "An original publication outcome is pending; retry that exact request explicitly.",
          );
        if (!transport.online())
          throw new DraftPublicationHeldError(
            intent.operation_id,
            "Publication was not dispatched. Reconnect and retry the original request explicitly.",
          );
        const previouslyDispatched = record.state !== "prepared";
        record = write({ ...record, state: "dispatched" }, record);
        try {
          const response = await request(
            context,
            `${route(intent.space_id, intent.document_id, intent.draft_id)}/publish`,
            "POST",
            intent.request,
          );
          const receipt = receiptFor(response, intent);
          record = write({ ...record, state: "confirmed", receipt, error: undefined }, record);
          transport.verify(context);
          return receipt;
        } catch (error) {
          const rejection =
            !previouslyDispatched &&
            error instanceof ApiError &&
            ([401, 403, 404].includes(error.status) ||
              ([400, 409, 422].includes(error.status) &&
                /^(source_required|source_conflict):/.test(error.message)));
          const message =
            error instanceof Error ? error.message : "Publication outcome unavailable";
          try {
            write(
              {
                ...record,
                state: record.receipt ? "confirmed" : rejection ? "rejected" : "unknown",
                error: message,
              },
              record,
            );
          } catch {
            throw new DraftPublicationHeldError(
              intent.operation_id,
              "Publication evidence could not be updated. Keep the original operation and inspect its receipt; never create a replacement request.",
            );
          }
          throw new DraftPublicationHeldError(
            intent.operation_id,
            `${message}${rejection ? "; rejected without confirmed publication; review a new source" : "; outcome unknown; retry only the original publication request"}`,
          );
        }
      },
    );
  };
  return {
    async prepare(
      space: string,
      page: string,
      savedInput: DraftRecord,
      originalOrigin: OfflineReplayOrigin,
    ): Promise<DraftPublicationIntent> {
      const saved = clone(savedInput);
      const origin = clone(originalOrigin);
      return transport.runExclusive(lockKey(origin, space, page), async () => {
        const context = current(origin);
        if (
          !saved ||
          !text(saved.id) ||
          saved.space_id !== space ||
          saved.document_id !== page ||
          saved.owner_id !== origin.accountId ||
          saved.access_role !== "owner" ||
          !saved.can_publish ||
          !digest(saved.source_digest)
        )
          throw new Error("Select an exact confirmed owned draft before reviewing publication.");
        const existing = records().find(
          (row) =>
            row.intent.origin.serverBase === origin.serverBase &&
            row.intent.origin.accountId === origin.accountId &&
            row.intent.space_id === space &&
            row.intent.document_id === page &&
            ["prepared", "dispatched", "unknown"].includes(row.state),
        );
        if (existing)
          throw new DraftPublicationHeldError(
            existing.intent.operation_id,
            "An original publication request already exists. Reopen its review or retry it explicitly without saving a newer source.",
          );
        const source = validateSource(
          await request(context, `${route(space, page, saved.id)}/publish-source`, "GET"),
          space,
          page,
          saved.id,
        );
        if (
          source.draft_source_digest !== saved.source_digest ||
          !equal(source.draft, {
            title: saved.title,
            content: saved.content,
            icon: saved.icon ?? null,
            cover_image: saved.cover_image ?? null,
          })
        )
          throw new Error(
            "Saved draft changed before publication review; no publish request was prepared.",
          );
        const operation_id = transport.operationId();
        const intent: DraftPublicationIntent = {
          schema_version: 1,
          operation_id,
          space_id: space,
          document_id: page,
          draft_id: saved.id,
          origin,
          source,
          request: {
            schema_version: 1,
            operation_id,
            draft_source_digest: source.draft_source_digest,
            page_source_digest: source.page_source_digest,
          },
          expectedPublication: {
            ...source.draft,
            content: stripTemplateGuidance(source.draft.content),
          },
        };
        validateIntent(intent);
        transport.verify(context);
        write({ schema_version: 1, intent, state: "prepared" }, null);
        return clone(intent);
      });
    },
    publish: (intent: DraftPublicationIntent) => dispatch(intent, false),
    retry(operationId: string) {
      return dispatch(read(operationId).intent, true);
    },
    inspect(space: string, page: string) {
      const context = transport.capture();
      transport.verify(context);
      return records().filter(
        (row) =>
          row.intent.space_id === space &&
          row.intent.document_id === page &&
          row.intent.origin.serverBase === context.origin.serverBase &&
          row.intent.origin.accountId === context.origin.accountId,
      );
    },
    async cancelPreparation(operationId: string): Promise<void> {
      const original = read(operationId);
      await transport.runExclusive(
        lockKey(original.intent.origin, original.intent.space_id, original.intent.document_id),
        async () => {
          current(original.intent.origin);
          const record = read(operationId);
          if (!equal(record.intent, original.intent))
            throw new Error(
              "Prepared publication changed before cancellation; retain both sources for review.",
            );
          if (record.state !== "prepared")
            throw new DraftPublicationHeldError(
              operationId,
              "Only an undispatched prepared publication can be cancelled. Retain the original outcome evidence.",
            );
          write({ ...record, state: "cancelled" }, record);
        },
      );
    },
  };
}
function liveService() {
  return createDocumentDraftPublicationService({
    storage: localStorage,
    capture: captureOfflineServerContext,
    verify: (context) => verifyOfflineReplayContext(context, getApiBase(), api.getToken()),
    fetch: (...args) => fetch(...args),
    online: () => typeof navigator === "undefined" || navigator.onLine !== false,
    operationId: () => crypto.randomUUID(),
    runExclusive: async (name, run) => {
      if (typeof navigator === "undefined" || !navigator.locks)
        throw new Error(
          "Publication requires Web Locks; keep the original request and use a supported HTTPS browser.",
        );
      return navigator.locks.request(name, run);
    },
  });
}
export const prepareDocumentDraftPublication = (
  space: string,
  page: string,
  saved: DraftRecord,
  origin: OfflineReplayOrigin,
) => liveService().prepare(space, page, saved, origin);
export const publishPreparedDocumentDraft = (intent: DraftPublicationIntent) =>
  liveService().publish(intent);
export const retryDocumentDraftPublication = (operationId: string) =>
  liveService().retry(operationId);
export const inspectDocumentDraftPublications = (space: string, page: string) =>
  liveService().inspect(space, page);
export const cancelPreparedDocumentDraftPublication = (operationId: string) =>
  liveService().cancelPreparation(operationId);

export interface DraftPublicationMetadata {
  title: string;
  icon?: string;
  cover_image?: string;
}

export interface DraftPublicationResponse {
  document_id?: string;
  published_at?: string;
  updated_at?: string;
}

type PublishedDocumentFields = {
  id: string;
  title: string;
  icon?: string;
  cover_image?: string;
  updated_at?: string;
  published_at?: string;
  last_updated_by?: string;
};

/** Local-only compatibility merge; server publication requires an original receipt and fresh reload. */
export function mergeDraftPublication<TDocument extends PublishedDocumentFields>(
  current: TDocument,
  draft: DraftPublicationMetadata,
  publication: DraftPublicationResponse,
  publisherId: string,
): TDocument {
  return {
    ...current,
    // The loaded record remains the identity source; minimal responses must
    // never replace it with an absent or mismatched document identifier.
    id: current.id,
    title: draft.title,
    icon: draft.icon,
    cover_image: draft.cover_image,
    published_at: publication.published_at ?? current.published_at,
    updated_at: publication.updated_at ?? publication.published_at ?? current.updated_at,
    last_updated_by: publisherId || current.last_updated_by,
  } as TDocument;
}
