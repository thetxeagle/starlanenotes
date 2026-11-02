import {
  isDraftSaveHeldError,
  type DraftRecord,
  type DraftSaveHoldReason,
  type DraftSaveSelection,
} from "./dataClient";
import {
  DraftBirthHeldError,
  selectedDraftFork,
  type DraftBirthOptions,
  type RetainedDraftBirthInspection,
} from "./draftBirth";
import type {
  DocumentDraftRecoveryScope,
  DocumentDraftRecoverySnapshot,
  StorageLike,
} from "./documentDraftRecovery";

export interface RecoveryPatch {
  title: string;
  content: string;
  icon?: string;
  cover_image?: string;
}
export interface DraftRecoveryIntent {
  scope: DocumentDraftRecoveryScope;
  checkpoint: DocumentDraftRecoverySnapshot;
  reviewedDraft: DraftRecord;
  published: boolean;
  selection: DraftSaveSelection;
  patch: RecoveryPatch;
  holdReason?: DraftSaveHoldReason;
}
export interface DraftRecoveryTransport {
  storage: StorageLike & Partial<Pick<Storage, "length" | "key">>;
  runExclusive?: <T>(key: string, run: () => Promise<T>) => Promise<T>;
  inspectBirth?: (
    space: string,
    document: string,
    operationId: string,
  ) => RetainedDraftBirthInspection | null;
  hasPendingTargetSave?: (target: DraftRecord, intent: DraftRecoveryIntent) => boolean;
  hasPendingOriginalSave?: (intent: DraftRecoveryIntent) => boolean;
  verifySelection: (selection: DraftSaveSelection, scope: DocumentDraftRecoveryScope) => void;
  createDraft: (
    space: string,
    document: string,
    options: DraftBirthOptions,
    onPreparedOperationId: (operationId: string) => void,
  ) => Promise<DraftRecord>;
  retryBirth: (space: string, document: string, operationId: string) => Promise<DraftRecord>;
  saveDraft: (
    space: string,
    document: string,
    draft: string,
    patch: RecoveryPatch,
    sourceDigest?: string,
  ) => Promise<DraftRecord>;
}
export interface DraftRecoveryCopyRecord {
  version: 1;
  intent: DraftRecoveryIntent;
  options: DraftBirthOptions;
  state:
    | "prepared"
    | "birth-pending"
    | "birth-held"
    | "target-confirmed"
    | "save-pending"
    | "save-held"
    | "save-rejected"
    | "confirmed"
    | "finished";
  operationId?: string;
  target?: DraftRecord;
  saved?: DraftRecord;
  error?: string;
  saveRejection?: "source_conflict" | "source_required";
  reviewedTarget?: DraftRecord;
}
export interface DraftRecoveryReplacementRecord {
  version: 1;
  kind: "replacement";
  intent: DraftRecoveryIntent;
  state: "prepared" | "save-pending" | "save-held" | "save-rejected" | "confirmed";
  saved?: DraftRecord;
  error?: string;
  saveRejection?: "source_conflict" | "source_required";
  previousArchiveKey?: string;
}
// Device journal bound only; exceeding it holds the proposal without a request.
const MAX_REPLACEMENT_JOURNAL_CHARS = 32 * 1024 * 1024;
const MAX_REPLACEMENT_DISCOVERY_KEYS = 10_000;
function replacementRaw(value: unknown): string {
  const raw = JSON.stringify(value, (_key, item: unknown) => {
    if (
      typeof item === "function" ||
      typeof item === "symbol" ||
      (typeof item === "number" && !Number.isFinite(item))
    )
      throw new Error("Recovery replacement cannot retain non-JSON evidence.");
    return item;
  });
  if (typeof raw !== "string" || raw.length > MAX_REPLACEMENT_JOURNAL_CHARS)
    throw new Error(
      "Recovery replacement evidence exceeds this device journal bound; keep both sources for review.",
    );
  return raw;
}
const PREFIX = "starlane_document_draft_recovery_copy_v1:";
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const digest = (value: unknown): value is string =>
  typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
function key(intent: Pick<DraftRecoveryIntent, "scope" | "checkpoint">): string {
  return (
    PREFIX +
    [
      intent.scope.storageMode,
      intent.scope.serverTarget,
      intent.scope.authIdentity,
      intent.scope.spaceId,
      intent.scope.documentId,
      intent.scope.draftId,
      intent.checkpoint.snapshotId,
    ]
      .map(encodeURIComponent)
      .join(":")
  );
}
function identity(intent: DraftRecoveryIntent): string {
  const { holdReason: _hold, ...original } = intent;
  void _hold;
  return JSON.stringify(original);
}
function validate(intent: DraftRecoveryIntent): void {
  const { scope, reviewedDraft: draft, selection, checkpoint, patch } = intent;
  if (
    !Object.values(scope).every((value) => typeof value === "string" && value.length > 0) ||
    !checkpoint.snapshotId ||
    draft.id !== scope.draftId ||
    draft.space_id !== scope.spaceId ||
    draft.document_id !== scope.documentId ||
    selection.accountId !== scope.authIdentity ||
    selection.serverBase !== scope.serverTarget ||
    (scope.storageMode === "server") !== (selection.mode === "server") ||
    typeof patch.title !== "string" ||
    !patch.title.trim() ||
    typeof patch.content !== "string" ||
    (patch.icon !== undefined && typeof patch.icon !== "string") ||
    (patch.cover_image !== undefined && typeof patch.cover_image !== "string")
  ) {
    throw new Error("Recovery selection is unproven; retain the original checkpoint for review.");
  }
}
function confirmed(
  intent: DraftRecoveryIntent,
  draft: DraftRecord,
  target?: DraftRecord,
  copy = false,
): DraftRecord {
  if (
    !draft ||
    !draft.id ||
    draft.space_id !== intent.scope.spaceId ||
    draft.document_id !== intent.scope.documentId ||
    (intent.scope.storageMode === "server" && !digest(draft.source_digest)) ||
    (target && draft.id !== target.id) ||
    (copy &&
      (draft.id === intent.scope.draftId ||
        draft.owner_id !== intent.scope.authIdentity ||
        draft.access_role !== "owner" ||
        !draft.can_publish))
  ) {
    throw new Error(
      "Recovery acknowledgement is unproven; retain the checkpoint and operation evidence.",
    );
  }
  return clone(draft);
}
function confirmedSave(
  intent: DraftRecoveryIntent,
  draft: DraftRecord,
  target: DraftRecord,
  copy: boolean,
): DraftRecord {
  confirmed(intent, draft, target, copy);
  if (
    draft.title !== intent.patch.title.trim() ||
    draft.content !== intent.patch.content ||
    ["icon", "cover_image"].some((field) => {
      const k = field as "icon" | "cover_image",
        value = intent.patch[k];
      return value !== undefined && (draft[k] ?? null) !== (value.trim() ? value : null);
    })
  )
    throw new Error("Recovery save fields are unproven; keep both copies for review.");
  return clone(draft);
}

function saveFailure(
  error: unknown,
): Pick<DraftRecoveryCopyRecord, "error" | "saveRejection"> & {
  state: "save-held" | "save-rejected";
} {
  const reason = isDraftSaveHeldError(error) ? error.reason : undefined;
  return {
    state:
      reason === "source_conflict" || reason === "source_required" ? "save-rejected" : "save-held",
    saveRejection:
      reason === "source_conflict" || reason === "source_required" ? reason : undefined,
    error: error instanceof Error ? error.message : "Save outcome unavailable",
  };
}

/** Explicit recovery only. Never publishes, clears checkpoints, or changes the original queue. */
export function createDocumentDraftRecoveryActions(transport: DraftRecoveryTransport) {
  let busy = false;
  const verify = (intent: DraftRecoveryIntent) => {
    validate(intent);
    transport.verifySelection(intent.selection, intent.scope);
  };
  const read = (
    intent: DraftRecoveryIntent,
    inspection = false,
  ): DraftRecoveryCopyRecord | null => {
    const raw = transport.storage.getItem(key(intent));
    if (!raw) return null;
    let record: DraftRecoveryCopyRecord;
    try {
      record = JSON.parse(raw) as DraftRecoveryCopyRecord;
    } catch {
      throw new Error("Recovery copy journal is corrupt; retain it for review.");
    }
    if (
      !record ||
      record.version !== 1 ||
      !record.intent ||
      (inspection
        ? JSON.stringify({
            scope: record.intent.scope,
            checkpoint: record.intent.checkpoint,
            selection: record.intent.selection,
          }) !==
          JSON.stringify({
            scope: intent.scope,
            checkpoint: intent.checkpoint,
            selection: intent.selection,
          })
        : identity(record.intent) !== identity(intent)) ||
      ![
        "prepared",
        "birth-pending",
        "birth-held",
        "target-confirmed",
        "save-pending",
        "save-held",
        "save-rejected",
        "confirmed",
        "finished",
      ].includes(record.state)
    ) {
      throw new Error("Recovery copy journal differs from this intent; retain both for review.");
    }
    validate(record.intent);
    const originalOptions: DraftBirthOptions = {
      force_new: true,
      ...(!record.intent.published && record.intent.scope.storageMode === "server"
        ? selectedDraftFork(record.intent.reviewedDraft)
        : {}),
    };
    if (JSON.stringify(record.options) !== JSON.stringify(originalOptions))
      throw new Error("Recovery birth options changed; retain the original journal for review.");
    return clone(record);
  };
  const write = (
    intent: DraftRecoveryIntent,
    record: DraftRecoveryCopyRecord,
    previous: DraftRecoveryCopyRecord | null,
  ) => {
    const expected = previous ? JSON.stringify(previous) : null;
    if (transport.storage.getItem(key(intent)) !== expected)
      throw new Error("Recovery copy changed during the operation; keep its evidence for review.");
    const raw = JSON.stringify(record);
    transport.storage.setItem(key(intent), raw);
    if (transport.storage.getItem(key(intent)) !== raw)
      throw new Error("Could not retain recovery copy evidence; stop and review this device.");
    return clone(record);
  };
  const exclusively = async <T>(
    input: DraftRecoveryIntent,
    run: (intent: DraftRecoveryIntent) => Promise<T>,
  ): Promise<T> => {
    const intent = clone(input);
    if (busy) throw new Error("A recovery action is already pending.");
    busy = true;
    try {
      const lockName = `starlane:recovery:${key(intent)}`;
      if (transport.runExclusive) return await transport.runExclusive(lockName, () => run(intent));
      if (typeof navigator === "undefined" || !navigator.locks)
        throw new Error(
          "Recovery requires Web Locks; keep this checkpoint and use a supported HTTPS browser.",
        );
      return await navigator.locks.request(lockName, () => run(intent));
    } finally {
      busy = false;
    }
  };
  const replacementKey = (intent: DraftRecoveryIntent) => `${key(intent)}:replacement`;
  const validateReplacementIntent = (intent: DraftRecoveryIntent) => {
    validate(intent);
    confirmed(intent, intent.reviewedDraft);
    if (
      [
        "owner_id",
        "owner_name",
        "document_title",
        "title",
        "content",
        "base_updated_at",
        "created_at",
        "updated_at",
      ].some((field) => typeof intent.reviewedDraft[field as keyof DraftRecord] !== "string")
    )
      throw new Error("Recovery replacement reviewed source is malformed; keep both sources.");
  };
  const confirmedReplacementSave = (
    intent: DraftRecoveryIntent,
    saved: DraftRecord,
  ): DraftRecord => {
    const result = confirmedSave(intent, saved, intent.reviewedDraft, false);
    if (
      ["owner_id", "created_at", "base_updated_at"].some(
        (field) =>
          result[field as keyof DraftRecord] !== intent.reviewedDraft[field as keyof DraftRecord],
      )
    )
      throw new Error(
        "Recovery replacement acknowledgement changed original draft provenance; keep both sources.",
      );
    return result;
  };
  const readReplacement = (
    intent: DraftRecoveryIntent,
    inspection = false,
    archiveKey?: string,
  ): DraftRecoveryReplacementRecord | null => {
    if (archiveKey && !archiveKey.startsWith(`${key(intent)}:archive:`))
      throw new Error("Recovery replacement archive is outside this selection.");
    const raw = transport.storage.getItem(archiveKey ?? replacementKey(intent));
    if (raw === null) return null;
    if (!raw || raw.length > MAX_REPLACEMENT_JOURNAL_CHARS)
      throw new Error("Recovery replacement journal is corrupt or oversized; keep its evidence.");
    let record: DraftRecoveryReplacementRecord;
    try {
      record = JSON.parse(raw) as DraftRecoveryReplacementRecord;
    } catch {
      throw new Error("Recovery replacement journal is corrupt; keep its evidence.");
    }
    if (
      !record ||
      record.version !== 1 ||
      record.kind !== "replacement" ||
      !record.intent ||
      !["prepared", "save-pending", "save-held", "save-rejected", "confirmed"].includes(
        record.state,
      ) ||
      (inspection
        ? JSON.stringify([
            record.intent.scope,
            record.intent.checkpoint,
            record.intent.selection,
          ]) !== JSON.stringify([intent.scope, intent.checkpoint, intent.selection])
        : identity(record.intent) !== identity(intent))
    )
      throw new Error(
        "Recovery replacement journal differs from this intent; retain both sources.",
      );
    validateReplacementIntent(record.intent);
    if (record.error !== undefined && typeof record.error !== "string")
      throw new Error("Recovery replacement error evidence is malformed.");
    if (
      record.previousArchiveKey !== undefined &&
      (typeof record.previousArchiveKey !== "string" ||
        !record.previousArchiveKey.startsWith(`${key(intent)}:archive:`))
    )
      throw new Error("Recovery replacement archive identity is malformed.");
    if (
      record.state === "save-rejected" &&
      !["source_conflict", "source_required"].includes(record.saveRejection ?? "")
    )
      throw new Error("Recovery replacement rejection is unproven.");
    if (record.state === "confirmed") {
      if (!record.saved) throw new Error("Recovery replacement acknowledgement is incomplete.");
      confirmedReplacementSave(record.intent, record.saved);
    } else if (record.saved !== undefined)
      throw new Error("Recovery replacement journal has an unconfirmed acknowledgement.");
    return clone(record);
  };
  const replacementStorage = () => {
    const storage = transport.storage;
    if (
      typeof storage.key !== "function" ||
      !Number.isSafeInteger(storage.length) ||
      storage.length! < 0
    )
      throw new Error(
        "Retained replacement history requires enumerable browser storage; keep both sources.",
      );
    if (storage.length! > MAX_REPLACEMENT_DISCOVERY_KEYS)
      throw new Error(
        "Retained replacement history exceeds the bounded device inventory; preserve browser data for review.",
      );
    const first = storage.key(0);
    if (first !== null && typeof first !== "string")
      throw new Error(
        "Retained replacement history enumeration is unavailable; keep both sources.",
      );
    return storage as StorageLike & Pick<Storage, "length" | "key">;
  };
  const writeReplacement = (
    intent: DraftRecoveryIntent,
    record: DraftRecoveryReplacementRecord,
    previous: DraftRecoveryReplacementRecord | null,
  ) => {
    const raw = replacementRaw(record);
    if (
      transport.storage.getItem(replacementKey(intent)) !==
      (previous ? replacementRaw(previous) : null)
    )
      throw new Error("Recovery replacement changed during this operation; keep its evidence.");
    transport.storage.setItem(replacementKey(intent), raw);
    if (transport.storage.getItem(replacementKey(intent)) !== raw)
      throw new Error("Could not retain both replacement sources; stop and review this device.");
    return clone(record);
  };
  const performCopy = async (intent: DraftRecoveryIntent, retry: boolean) => {
    verify(intent);
    let record = read(intent);
    if (record?.state === "confirmed" || record?.state === "finished") {
      if (!record.saved || !record.target)
        throw new Error("Confirmed recovery evidence is incomplete.");
      return confirmedSave(intent, record.saved, record.target, true);
    }
    if (
      record &&
      (["save-pending", "save-held", "save-rejected"].includes(record.state) ||
        (record.state === "birth-pending" && (!retry || !record.operationId)))
    ) {
      throw new Error("Recovery outcome needs explicit review; no second request was dispatched.");
    }
    if (!record) {
      if (retry) throw new Error("No original recovery copy request exists to retry.");
      const options: DraftBirthOptions = {
        force_new: true,
        ...(!intent.published && intent.scope.storageMode === "server"
          ? selectedDraftFork(intent.reviewedDraft)
          : {}),
      };
      record = write(intent, { version: 1, intent, options, state: "prepared" }, null);
    }
    if (!record.target) {
      if (record.state === "birth-held" && (!retry || !record.operationId))
        throw new Error("Retry the original retained birth request explicitly.");
      const operationId = record.operationId;
      record = write(intent, { ...record, state: "birth-pending" }, record);
      let target: DraftRecord;
      try {
        target = operationId
          ? await transport.retryBirth(intent.scope.spaceId, intent.scope.documentId, operationId)
          : await transport.createDraft(
              intent.scope.spaceId,
              intent.scope.documentId,
              clone(record.options),
              (preparedOperationId) => {
                record = write(intent, { ...record!, operationId: preparedOperationId }, record);
              },
            );
      } catch (error) {
        // Persist the original birth identity before a context check can reject it.
        record = write(
          intent,
          {
            ...record,
            state: "birth-held",
            ...(error instanceof DraftBirthHeldError && record.operationId === error.operationId
              ? { operationId: error.operationId }
              : {}),
            error: error instanceof Error ? error.message : "Birth outcome unavailable",
          },
          record,
        );
        transport.verifySelection(intent.selection, intent.scope);
        throw error;
      }
      // Preserve an acknowledged target even if the original selection changed while awaiting.
      record = write(
        intent,
        { ...record, target: clone(target), state: "target-confirmed" },
        record,
      );
      verify(intent);
      confirmed(intent, target, undefined, true);
    }
    verify(intent);
    const target = confirmed(intent, record.target!, undefined, true);
    record = write(intent, { ...record, state: "save-pending" }, record);
    let saved: DraftRecord;
    try {
      saved = await transport.saveDraft(
        intent.scope.spaceId,
        intent.scope.documentId,
        target.id,
        clone(intent.patch),
        target.source_digest,
      );
      saved = confirmedSave(intent, saved, target, true);
    } catch (error) {
      write(
        intent,
        {
          ...record,
          ...saveFailure(error),
          error: error instanceof Error ? error.message : "Save outcome unavailable",
        },
        record,
      );
      transport.verifySelection(intent.selection, intent.scope);
      throw error;
    }
    // Confirmed evidence must survive late context drift and UI activation failure.
    write(intent, { ...record, saved, state: "confirmed" }, record);
    verify(intent);
    return saved;
  };
  const copy = (input: DraftRecoveryIntent, retry: boolean) =>
    exclusively(input, (intent) => performCopy(intent, retry));
  const archive = (
    intent: DraftRecoveryIntent,
    record: DraftRecoveryCopyRecord | DraftRecoveryReplacementRecord,
  ) => {
    const archiveKey = `${key(intent)}:archive:${crypto.randomUUID()}`;
    const raw = JSON.stringify(record);
    if (transport.storage.getItem(archiveKey) !== null)
      throw new Error("Recovery archive already exists; no evidence was overwritten.");
    transport.storage.setItem(archiveKey, raw);
    if (transport.storage.getItem(archiveKey) !== raw)
      throw new Error("Could not retain previous recovery evidence; no request was dispatched.");
    return archiveKey;
  };
  const restartable = (record: DraftRecoveryCopyRecord | null) => {
    if (
      !record ||
      record.state !== "birth-held" ||
      record.target ||
      !record.operationId ||
      record.intent.scope.storageMode !== "server"
    )
      return false;
    const proof = transport.inspectBirth?.(
      record.intent.scope.spaceId,
      record.intent.scope.documentId,
      record.operationId,
    );
    return (
      !!proof &&
      proof.operationId === record.operationId &&
      proof.state === "rejected" &&
      JSON.stringify(proof.options) === JSON.stringify(record.options)
    );
  };
  return {
    get busy() {
      return busy;
    },
    listReplacements(scope: DocumentDraftRecoveryScope, selection: DraftSaveSelection) {
      transport.verifySelection(selection, scope);
      const storage = replacementStorage();
      const prefix =
        PREFIX +
        [
          scope.storageMode,
          scope.serverTarget,
          scope.authIdentity,
          scope.spaceId,
          scope.documentId,
          scope.draftId,
        ]
          .map(encodeURIComponent)
          .join(":") +
        ":";
      const keys: string[] = [];
      const length = storage.length;
      for (let i = 0; i < length; i++) {
        const candidate = storage.key(i);
        if (candidate?.startsWith(prefix) && candidate.endsWith(":replacement"))
          keys.push(candidate);
      }
      const records: DraftRecoveryReplacementRecord[] = [];
      for (const candidate of keys.sort()) {
        const raw = storage.getItem(candidate);
        if (raw === null) continue;
        if (raw.length > MAX_REPLACEMENT_JOURNAL_CHARS)
          throw new Error("Retained replacement history is oversized; preserve browser data.");
        let value: DraftRecoveryReplacementRecord;
        try {
          value = JSON.parse(raw) as DraftRecoveryReplacementRecord;
        } catch {
          throw new Error("Retained replacement history is corrupt; preserve browser data.");
        }
        if (
          !value?.intent ||
          candidate !== replacementKey(value.intent) ||
          JSON.stringify(value.intent.scope) !== JSON.stringify(scope)
        )
          throw new Error(
            "Retained replacement history identity is unproven; preserve browser data.",
          );
        if (JSON.stringify(value.intent.selection) !== JSON.stringify(selection)) continue;
        const record = readReplacement(value.intent, true);
        if (record) records.push(record);
      }
      transport.verifySelection(selection, scope);
      return records;
    },
    inspectReplacement(input: DraftRecoveryIntent, archiveKey?: string) {
      verify(input);
      return readReplacement(clone(input), true, archiveKey);
    },
    inspectCopy(input: DraftRecoveryIntent) {
      validate(input);
      return read(clone(input), true);
    },
    canRestartCopy(input: DraftRecoveryIntent) {
      verify(input);
      return restartable(read(input, true));
    },
    restartCopy: (input: DraftRecoveryIntent) =>
      exclusively(input, async (intent) => {
        verify(intent);
        const previous = read(intent, true);
        if (!restartable(previous))
          throw new Error(
            "Only an exactly proven rejected birth can be restarted after review; unknown requests remain held.",
          );
        const options: DraftBirthOptions = {
          force_new: true,
          ...(!intent.published ? selectedDraftFork(intent.reviewedDraft) : {}),
        };
        archive(intent, previous!);
        verify(intent);
        write(intent, { version: 1, intent, options, state: "prepared" }, previous);
        return performCopy(intent, false);
      }),
    replaceCopyTarget: (input: DraftRecoveryIntent, reviewedInput: DraftRecord) => {
      const reviewed = clone(reviewedInput);
      return exclusively(input, async (intent) => {
        verify(intent);
        let record = read(intent);
        if (
          intent.scope.storageMode !== "server" ||
          record?.state !== "save-rejected" ||
          !record.target ||
          !["source_conflict", "source_required"].includes(record.saveRejection ?? "")
        )
          throw new Error(
            "Only a proven rejected copy save can be replaced after review; unknown outcomes remain held.",
          );
        const target = confirmed(intent, reviewed, record.target, true);
        if (!transport.hasPendingTargetSave)
          throw new Error("Review retained target saves before replacing this copy.");
        if (transport.hasPendingTargetSave(target, intent))
          throw new Error("Review the queued or unknown target save before replacing this copy.");
        archive(intent, record);
        verify(intent);
        record = write(
          intent,
          { ...record, reviewedTarget: target, state: "save-pending" },
          record,
        );
        let saved: DraftRecord;
        try {
          saved = confirmedSave(
            intent,
            await transport.saveDraft(
              intent.scope.spaceId,
              intent.scope.documentId,
              target.id,
              clone(intent.patch),
              target.source_digest,
            ),
            target,
            true,
          );
        } catch (error) {
          write(intent, { ...record, ...saveFailure(error) }, record);
          verify(intent);
          throw error;
        }
        write(intent, { ...record, saved, state: "confirmed" }, record);
        verify(intent);
        return saved;
      });
    },
    copy: (intent: DraftRecoveryIntent) => copy(intent, false),
    retryCopy: (intent: DraftRecoveryIntent) => copy(intent, true),
    replace: async (input: DraftRecoveryIntent) => {
      // Validate serialization before the shared cloning boundary can omit evidence.
      replacementRaw(input);
      return exclusively(input, async (intent) => {
        verify(intent);
        validateReplacementIntent(intent);
        replacementStorage();
        if (intent.scope.storageMode === "local")
          throw new Error(
            "Use a separate recovery copy for local drafts; reviewed replacement requires server source protection.",
          );
        if (intent.holdReason === "queued" || intent.holdReason === "outcome_unknown")
          throw new Error(
            "Review the original queued or unknown save before replacing this draft.",
          );
        const target = confirmed(intent, intent.reviewedDraft);
        if (target.access_role === "viewer")
          throw new Error("Review an editable draft before replacing its content.");
        if (transport.hasPendingOriginalSave?.(intent))
          throw new Error("Review the retained original save before replacing this draft.");
        let record = readReplacement(intent, true);
        if (record && identity(record.intent) !== identity(intent)) {
          if (
            record.state !== "save-rejected" ||
            record.intent.reviewedDraft.source_digest === target.source_digest
          )
            throw new Error(
              "Recovery replacement outcome needs explicit review; no second request was dispatched.",
            );
          const previousArchiveKey = archive(intent, record);
          verify(intent);
          record = writeReplacement(
            intent,
            { version: 1, kind: "replacement", intent, state: "prepared", previousArchiveKey },
            record,
          );
        }
        if (!record)
          record = writeReplacement(
            intent,
            { version: 1, kind: "replacement", intent, state: "prepared" },
            null,
          );
        if (record.state === "confirmed") return confirmedReplacementSave(intent, record.saved!);
        if (record.state !== "prepared")
          throw new Error(
            "Recovery replacement outcome needs explicit review; no second request was dispatched.",
          );
        verify(intent);
        record = writeReplacement(intent, { ...record, state: "save-pending" }, record);
        verify(intent);
        let saved: DraftRecord;
        try {
          saved = confirmedReplacementSave(
            intent,
            await transport.saveDraft(
              intent.scope.spaceId,
              intent.scope.documentId,
              target.id,
              clone(intent.patch),
              target.source_digest,
            ),
          );
        } catch (error) {
          writeReplacement(intent, { ...record, ...saveFailure(error) }, record);
          verify(intent);
          throw error;
        }
        writeReplacement(intent, { ...record, state: "confirmed", saved }, record);
        verify(intent);
        return saved;
      });
    },
    finishCopy(input: DraftRecoveryIntent, saved: DraftRecord) {
      if (busy) throw new Error("A recovery action is already pending.");
      const intent = clone(input);
      verify(intent);
      const record = read(intent);
      if (
        !record?.saved ||
        !record.target ||
        !["confirmed", "finished"].includes(record.state) ||
        JSON.stringify(record.saved) !== JSON.stringify(saved)
      )
        throw new Error("Only the exact confirmed recovery copy can be finished.");
      confirmedSave(intent, saved, record.target, true);
      write(intent, { ...record, state: "finished" }, record);
    },
  };
}
