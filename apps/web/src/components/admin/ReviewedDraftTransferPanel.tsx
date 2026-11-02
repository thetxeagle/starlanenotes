import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Download, RefreshCw, ShieldCheck, Upload } from "lucide-react";
import { api } from "@/lib/api";
import {
  dataClient,
  type DocumentContentRecord,
  type DocumentRecord,
  type DraftRecord,
  type SpaceRecord,
} from "@/lib/dataClient";
import type { LocalVaultSnapshot, VaultDocumentDraft, VaultQueuedSyncOperation } from "@/lib/localVault";
import {
  confirmReviewedDraftTransfer,
  currentLocalTransferRootIdentity,
  listReviewedDraftTransfers,
  prepareReviewedDraftTransfer,
  reviewedDraftTransferDownload,
  type ReviewedDraftTransferRecord,
} from "@/lib/reviewedDraftTransfer";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import {
  DialogBody,
  DialogCloseAction,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";

type AuthAccount = { id: string; email?: string; display_name?: string };

function sourceFor(operation: VaultQueuedSyncOperation): Record<string, unknown> | null {
  const payload = operation.payload;
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return null;
  const source = (payload as Record<string, unknown>).local_draft_source;
  return source && typeof source === "object" && !Array.isArray(source) ? source as Record<string, unknown> : null;
}

function operationsForDraft(snapshot: LocalVaultSnapshot | null, draft: VaultDocumentDraft | undefined): VaultQueuedSyncOperation[] {
  if (!snapshot || !draft) return [];
  return (snapshot.sync_state?.queued_operations ?? []).filter((operation) => {
    const source = sourceFor(operation);
    return operation.entity_type === "asset" && operation.action === "create" && source?.schema_version === 1 &&
      source.space_id === draft.space_id && source.document_id === draft.document_id && source.draft_id === draft.id && source.draft_created_at === draft.created_at;
  });
}

function download(filename: string, value: unknown): void {
  const url = URL.createObjectURL(new Blob([JSON.stringify(value, null, 2)], { type: "application/json" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

export function ReviewedDraftTransferPanel({ disabled = false }: { disabled?: boolean }) {
  const reviewTriggerRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<LocalVaultSnapshot | null>(null);
  const [account, setAccount] = useState<AuthAccount | null>(null);
  const [spaces, setSpaces] = useState<SpaceRecord[]>([]);
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [targetDocument, setTargetDocument] = useState<DocumentRecord | null>(null);
  const [serverDrafts, setServerDrafts] = useState<DraftRecord[]>([]);
  const [sourceDraftId, setSourceDraftId] = useState("");
  const [targetSpaceId, setTargetSpaceId] = useState("");
  const [targetDocumentId, setTargetDocumentId] = useState("");
  const [targetDraftId, setTargetDraftId] = useState("");
  const [selectedOperations, setSelectedOperations] = useState<Set<string>>(() => new Set());
  const [records, setRecords] = useState<ReviewedDraftTransferRecord[]>([]);

  const refreshRecords = useCallback(async () => {
    setRecords(await listReviewedDraftTransfers());
  }, []);

  useEffect(() => {
    refreshRecords().catch(() => setRecords([]));
  }, [refreshRecords]);

  const eligibleDrafts = useMemo(() => snapshot?.drafts?.filter((draft) => operationsForDraft(snapshot, draft).length > 0) ?? [], [snapshot]);
  const sourceDraft = eligibleDrafts.find((draft) => draft.id === sourceDraftId);
  const sourceOperations = useMemo(() => operationsForDraft(snapshot, sourceDraft), [snapshot, sourceDraft]);

  const openReview = async () => {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const [nextSnapshot, nextAccount, nextSpaces] = await Promise.all([
        dataClient.exportLocalVaultSnapshot(),
        api.fetch<AuthAccount>("/auth/me"),
        api.fetch<SpaceRecord[]>("/spaces"),
      ]);
      setSnapshot(nextSnapshot);
      setAccount(nextAccount);
      setSpaces(nextSpaces);
      const firstDraft = nextSnapshot.drafts?.find((draft) => operationsForDraft(nextSnapshot, draft).length > 0);
      setSourceDraftId(firstDraft?.id ?? "");
      setSelectedOperations(new Set(firstDraft ? operationsForDraft(nextSnapshot, firstDraft).map((operation) => operation.id) : []));
      setTargetSpaceId(nextSpaces[0]?.id ?? "");
      setTargetDocumentId("");
      setTargetDraftId("");
      setOpen(true);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Transfer review could not be opened");
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!open || !targetSpaceId) { setDocuments([]); return; }
    api.fetch<DocumentRecord[]>(`/spaces/${encodeURIComponent(targetSpaceId)}/documents?include_nested=true`)
      .then((next) => { setDocuments(next); setTargetDocumentId(next[0]?.id ?? ""); })
      .catch((caught) => setError(caught instanceof Error ? caught.message : "Server pages could not be loaded"));
  }, [open, targetSpaceId]);

  useEffect(() => {
    if (!open || !targetSpaceId || !targetDocumentId) { setTargetDocument(null); setServerDrafts([]); return; }
    Promise.all([
      api.fetch<DocumentRecord>(`/spaces/${encodeURIComponent(targetSpaceId)}/documents/${encodeURIComponent(targetDocumentId)}`),
      api.fetch<DraftRecord[]>(`/spaces/${encodeURIComponent(targetSpaceId)}/drafts?document_id=${encodeURIComponent(targetDocumentId)}`),
    ]).then(async ([document, summaries]) => {
      setTargetDocument(document);
      if (document.published_at) { setServerDrafts([]); setTargetDraftId(""); return; }
      const exact = await Promise.all(summaries.map((draft) => api.fetch<DraftRecord>(`/spaces/${encodeURIComponent(targetSpaceId)}/documents/${encodeURIComponent(targetDocumentId)}/drafts/${encodeURIComponent(draft.id)}`)));
      setServerDrafts(exact);
      setTargetDraftId(exact[0]?.id ?? "");
    }).catch((caught) => setError(caught instanceof Error ? caught.message : "Exact server page source could not be loaded"));
  }, [open, targetSpaceId, targetDocumentId]);

  const chooseSourceDraft = (draftId: string) => {
    setSourceDraftId(draftId);
    const draft = eligibleDrafts.find((item) => item.id === draftId);
    setSelectedOperations(new Set(operationsForDraft(snapshot, draft).map((operation) => operation.id)));
  };

  const transfer = async () => {
    if (!sourceDraft || !account || !targetDocument) return;
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      let serverSource: Parameters<typeof prepareReviewedDraftTransfer>[0]["serverSource"];
      if (targetDocument.published_at) {
        const content = await api.fetch<DocumentContentRecord>(`/spaces/${encodeURIComponent(targetSpaceId)}/documents/${encodeURIComponent(targetDocumentId)}/content`);
        if (!targetDocument.updated_at || !content.published_source_digest) throw new Error("Published target source proof is unavailable");
        serverSource = { kind: "published_page", page_updated_at: targetDocument.updated_at, source_digest: content.published_source_digest };
      } else {
        const draft = serverDrafts.find((item) => item.id === targetDraftId);
        if (!draft?.source_digest) throw new Error("Select one exact editable server draft source");
        serverSource = { kind: "draft", draft_id: draft.id, draft_created_at: draft.created_at, source_digest: draft.source_digest };
      }
      const prepared = await prepareReviewedDraftTransfer({
        draft: sourceDraft,
        rootIdentity: await currentLocalTransferRootIdentity(),
        serverAccountId: account.id,
        serverSpaceId: targetSpaceId,
        serverSpaceName: spaces.find((space) => space.id === targetSpaceId)?.name ?? targetSpaceId,
        serverDocumentId: targetDocumentId,
        serverDocumentTitle: targetDocument.title,
        serverSource,
        selectedOperationIds: [...selectedOperations],
      });
      const confirmed = await confirmReviewedDraftTransfer(prepared);
      await refreshRecords();
      setOpen(false);
      setMessage(`Transfer confirmed to private server draft ${confirmed.receipt?.target_draft_id}. Original operations remain retained for reviewed replay.`);
    } catch (caught) {
      await refreshRecords().catch(() => undefined);
      setError(caught instanceof Error ? caught.message : "Transfer outcome is unknown; retry the retained original request");
    } finally {
      setBusy(false);
    }
  };

  const retry = async (record: ReviewedDraftTransferRecord) => {
    setBusy(true); setError(null); setMessage(null);
    try {
      const confirmed = await confirmReviewedDraftTransfer(record);
      await refreshRecords();
      setMessage(`Original transfer ${confirmed.operation_id} is confirmed.`);
    } catch (caught) {
      await refreshRecords().catch(() => undefined);
      setError(caught instanceof Error ? caught.message : "Original transfer retry remains unknown");
    } finally { setBusy(false); }
  };

  const replay = async (record: ReviewedDraftTransferRecord) => {
    if (!record.binding) return;
    setBusy(true); setError(null); setMessage(null);
    try {
      const ids = record.exact_manifest.operations.map((operation) => operation.id);
      const preflight = await dataClient.preflightQueuedSyncReplayManifest(ids, record.binding);
      if (!preflight.ready) throw new Error(`Reviewed replay blocked: ${preflight.blockers.join("; ")}`);
      const result = await dataClient.runQueuedSyncReplay(ids, record.binding);
      if (result.failed_operation_count || result.unsupported_operation_count) throw new Error("Reviewed replay completed partially; original operations remain queued");
      setMessage(`Reviewed replay applied ${result.applied_operation_count} and recognized ${result.skipped_operation_count} exact original operation(s).`);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Reviewed replay failed; original operations remain queued");
    } finally { setBusy(false); }
  };

  return (
    <div className="border-border mt-3 border-t pt-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-text text-xs font-medium">Reviewed Draft Transfers</p>
          <p className="text-text-muted mt-1 max-w-3xl text-xs">Create one private server draft from an exact local draft and bind only its reviewed asset operations. Unknown outcomes retain the original request and bytes for retry or download.</p>
        </div>
        <Button ref={reviewTriggerRef} size="sm" variant="secondary" onClick={() => void openReview()} disabled={disabled || busy}>
          <ShieldCheck size={14} /> Review Transfer
        </Button>
      </div>
      {message ? <p className="text-success mt-2 text-xs" role="status">{message}</p> : null}
      {error ? <p className="text-destructive mt-2 text-xs" role="alert">{error}</p> : null}
      {records.length ? <div className="mt-3 space-y-2">
        {records.slice(0, 5).map((record) => <div key={record.operation_id} className="border-border rounded-card border p-3 text-xs">
          <div className="flex flex-wrap items-start justify-between gap-2">
            <div>
              <p className="text-text font-medium">{record.request.reviewed_draft.title}</p>
              <p className="text-text-muted mt-1">{record.state} · {record.server_space_name} / {record.server_document_title} · {record.exact_manifest.operation_count} asset operation(s)</p>
              <p className="text-text-muted mt-1 break-all">Original {record.request.local_source.draft_id} → {record.receipt?.target_draft_id ?? "target outcome unknown"}</p>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" variant="ghost" onClick={() => download(`starlane-draft-transfer-${record.operation_id}.json`, reviewedDraftTransferDownload(record))}><Download size={14} /> Download</Button>
              {record.state !== "confirmed" ? <Button size="sm" variant="secondary" onClick={() => void retry(record)} disabled={busy}><RefreshCw size={14} /> Retry Original</Button> : null}
              {record.state === "confirmed" ? <Button size="sm" variant="secondary" onClick={() => void replay(record)} disabled={busy}><Upload size={14} /> Replay Assets</Button> : null}
            </div>
          </div>
        </div>)}
      </div> : <p className="text-text-muted mt-2 text-xs">No retained draft transfer reviews on this device.</p>}

      {open ? <DialogShell labelledBy="reviewed-draft-transfer-title" describedBy="reviewed-draft-transfer-description" maxWidth="xl" triggerRef={reviewTriggerRef} onClose={() => !busy && setOpen(false)}>
        <DialogHeader>
          <div><DialogTitle id="reviewed-draft-transfer-title">Review exact draft transfer</DialogTitle><DialogDescription id="reviewed-draft-transfer-description">Nothing is sent until this exact source, target, and operation set is durably retained.</DialogDescription></div>
          <DialogCloseAction onClick={() => setOpen(false)} disabled={busy} />
        </DialogHeader>
        <DialogBody className="space-y-4">
          <label className="block text-xs font-medium text-text">Original local draft<Select className="mt-1" value={sourceDraftId} onChange={(event) => chooseSourceDraft(event.target.value)}>{eligibleDrafts.map((draft) => <option key={draft.id} value={draft.id}>{draft.title} · {draft.id}</option>)}</Select></label>
          {sourceDraft ? <div className="border-border rounded-card border p-3 text-xs">
            <p className="text-text font-medium">Exact original snapshot</p>
            <dl className="text-text-muted mt-2 grid gap-1 sm:grid-cols-2"><div><dt>ID / created</dt><dd className="break-all text-text">{sourceDraft.id} / {sourceDraft.created_at}</dd></div><div><dt>Icon / cover</dt><dd className="break-all text-text">{sourceDraft.icon ?? "none"} / {sourceDraft.cover_image ?? "none"}</dd></div></dl>
            <pre className="border-border bg-bg text-text mt-3 max-h-40 overflow-auto whitespace-pre-wrap rounded-input border p-2 text-xs">{sourceDraft.content}</pre>
          </div> : <p className="text-destructive text-xs">No draft with exact queued asset provenance is available.</p>}
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="text-xs font-medium text-text">Destination server space<Select className="mt-1" value={targetSpaceId} onChange={(event) => setTargetSpaceId(event.target.value)}>{spaces.map((space) => <option key={space.id} value={space.id}>{space.name}</option>)}</Select></label>
            <label className="text-xs font-medium text-text">Destination page<Select className="mt-1" value={targetDocumentId} onChange={(event) => setTargetDocumentId(event.target.value)}>{documents.map((document) => <option key={document.id} value={document.id}>{document.title}</option>)}</Select></label>
          </div>
          {targetDocument && !targetDocument.published_at ? <label className="block text-xs font-medium text-text">Exact current server draft source<Select className="mt-1" value={targetDraftId} onChange={(event) => setTargetDraftId(event.target.value)}>{serverDrafts.map((draft) => <option key={draft.id} value={draft.id}>{draft.title} · {draft.id}</option>)}</Select></label> : null}
          <div className="border-border rounded-card border p-3">
            <p className="text-text text-xs font-medium">Exact original asset operations</p>
            <div className="mt-2 space-y-2">{sourceOperations.map((operation) => <label key={operation.id} className="text-text-muted flex items-start gap-2 text-xs"><input type="checkbox" checked={selectedOperations.has(operation.id)} onChange={(event) => setSelectedOperations((current) => { const next = new Set(current); if (event.target.checked) next.add(operation.id); else next.delete(operation.id); return next; })} /><span className="break-all"><span className="text-text">{operation.entity_id}</span> · {operation.id}</span></label>)}</div>
          </div>
          <p className="text-text-muted text-xs">Linked account: {account?.display_name ?? account?.email ?? account?.id ?? "unavailable"}. The target remains private; this action does not publish the page.</p>
        </DialogBody>
        <DialogFooter><Button variant="ghost" onClick={() => setOpen(false)} disabled={busy}>Cancel</Button><Button onClick={() => void transfer()} disabled={busy || !sourceDraft || !targetDocument || selectedOperations.size === 0 || (!targetDocument.published_at && !targetDraftId)}>Create Private Target</Button></DialogFooter>
      </DialogShell> : null}
    </div>
  );
}
