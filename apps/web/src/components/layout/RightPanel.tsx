import { useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { ChevronRight, Clock3, FileText, Link2, LoaderCircle, Sparkles, X } from "lucide-react";
import { PropertiesPanel } from "@/components/PropertiesPanel";
import { CommentsPanel } from "@/components/CommentsPanel";
import { DraftDiscussionPanel } from "@/components/DraftDiscussionPanel";
import { AiChatPanel } from "@/components/AiChatPanel";
import { Button, BUTTON_RESPONSIVE_HEIGHT_CLASS } from "@/components/ui/button";
import { AuthContext } from "@/hooks/useAuth";
import { SnapshotRecoveryConfirmation, type RecoveryComparison } from "@/components/SnapshotRecoveryConfirmation";
import type { SnapshotRecoveryIntent, SnapshotRecoveryRecord } from "@/lib/publicationSnapshotRecovery";
import { captureDraftSaveSelection } from "@/lib/dataClient";
import { PublishedSnapshotPreview } from "@/components/PublishedSnapshotPreview";
import { ClosedDraftDiscussionDialog, ClosedDraftDiscussionHistory } from "@/components/ClosedDraftDiscussionHistory";
import { listClosedDraftDiscussions, type ClosedDraftDiscussionSummary } from "@/lib/draftDiscussion";
import { DocumentPublicationHistory } from "./DocumentPublicationHistory";
import { publicationHistoryContextVersion, subscribePublicationHistoryContext } from "@/lib/documentPublishedHistory";
import { createPublicationHistorySelection, type PublicationHistorySelectionState } from "@/lib/documentPublishedHistorySelection";
import { verifyOfflineReplayContext } from "@/lib/offlineReplayOrigin";
import { captureOfflineServerContext, getApiBase, api } from "@/lib/api";
import { isApiError } from "@/lib/api";
import {
  dataClient,
  type BacklinkRecord,
  type DocumentRecord,
  type DocumentVersionRecord,
  type DocumentWorkflowEventRecord,
  type DocumentWorkflowRecord,
} from "@/lib/dataClient";
import { getCurrentLaunchMode, type AppLaunchMode } from "@/lib/launchMode";
import type { DocumentPageDetailsControls } from "./documentPageDetails";

export type RightPanelTab = "properties" | "backlinks" | "history" | "comments" | "ai";
export const RIGHT_PANEL_NAVIGATION_TABS: readonly RightPanelTab[] = [
  "properties",
  "backlinks",
  "history",
  "ai",
];
const SERVER_ONLY_TABS = new Set<RightPanelTab>(["comments", "ai"]);
export const RIGHT_PANEL_SCROLL_CONTENT_CLASS = "flex-1 overflow-y-auto";

export function formatBacklinkCount(count: number): string {
  return `${count} ${count === 1 ? "page links" : "pages link"} here`;
}

export type BacklinksErrorKind = "permission" | "unavailable";

export function getBacklinksErrorKind(error: unknown): BacklinksErrorKind {
  return isApiError(error) && error.status === 403 ? "permission" : "unavailable";
}

export function BacklinksErrorState({
  kind,
  onRetry,
}: {
  kind: BacklinksErrorKind;
  onRetry: () => void;
}) {
  if (kind === "permission") {
    return (
      <div role="alert" className="text-text-muted grid gap-2 text-sm">
        <p>You don’t have permission to view backlinks for this page.</p>
        <p className="text-xs">Ask a space owner to review your access.</p>
      </div>
    );
  }

  return (
    <div role="alert" className="text-text-muted grid gap-3 text-sm">
      <p>Unable to load backlinks.</p>
      <Button variant="secondary" size="sm" className="justify-self-start" onClick={onRetry}>
        Retry
      </Button>
    </div>
  );
}

export function LocalVersionHistoryNotice({
  workflowEventsAvailable = false,
}: {
  workflowEventsAvailable?: boolean;
}) {
  return (
    <div className="text-text-muted grid gap-1.5 text-sm" role="status">
      <p>Page version history and restore aren’t available in local vaults.</p>
      {!workflowEventsAvailable && (
        <p className="text-xs">Workflow events will appear here when recorded.</p>
      )}
    </div>
  );
}

export function ServerVersionHistoryNotice() {
  return (
    <div className="text-text-muted text-sm" role="status">
      Version records are available here as legacy metadata only; full source preview and restoring page content is not supported yet.
    </div>
  );
}

export function isRightPanelTabAvailable(
  tab: RightPanelTab,
  launchMode: AppLaunchMode = getCurrentLaunchMode(),
): boolean {
  return launchMode !== "local" || !SERVER_ONLY_TABS.has(tab);
}

export function SnapshotRecoveryRequests({records,busy,onReview,onCancel,onRetry}:{records:SnapshotRecoveryRecord[];busy:boolean;onReview:(intent:SnapshotRecoveryIntent)=>void;onCancel:(id:string)=>void;onRetry:(id:string)=>void}) {
  return <div className="grid gap-2">                {records.map(record => <div key={record.intent.operation_id} className="rounded-input border-border grid gap-2 border p-2">
                  <p className="font-medium">{record.state === "confirmed" ? "Recovered private draft" : record.state === "rejected" ? "Reviewed source changed before recovery" : record.state === "prepared" ? "Original recovery review retained" : "Original recovery outcome held"}</p>
                  <p className="text-text-muted text-xs">{record.intent.source.draft?.title} · {record.intent.operation_id}</p>
                  {record.error && <p className="text-text-muted text-xs">{record.error}</p>}
                  {record.state === "prepared" && <div className="flex flex-wrap gap-2"><Button size="sm" variant="secondary" disabled={busy} onClick={() => onReview(record.intent)}>Review original request</Button><Button size="sm" variant="secondary" disabled={busy} onClick={() => onCancel(record.intent.operation_id)}>Cancel prepared request</Button></div>}
                  {(record.state === "unknown" || record.state === "dispatched") && <><p className="text-text-muted text-xs">Retry only this original request. No replacement copy or publication is authorized.</p><Button size="sm" variant="secondary" className="justify-self-start" disabled={busy} onClick={() => onRetry(record.intent.operation_id)}>Retry original recovery</Button></>}
                  {record.state === "rejected" && <p className="text-text-muted text-xs">No copy was created by this operation. Preview a snapshot and explicitly review a new request.</p>}
                  {record.state === "confirmed" && record.receipt && <a className={`focus-ring rounded-button border-border bg-surface text-text hover:bg-surface-raised inline-flex items-center justify-center border px-3 py-1 text-xs font-medium transition-colors duration-100 ${BUTTON_RESPONSIVE_HEIGHT_CLASS}`} href={`/spaces/${encodeURIComponent(record.intent.space_id)}/docs/${encodeURIComponent(record.intent.document_id)}?draft=${encodeURIComponent(record.receipt.draft.id)}`} target="_blank" rel="noopener noreferrer">Open recovered draft in a new tab</a>}
                </div>)}</div>;
}

interface RightPanelProps {
  spaceId?: string;
  docId?: string;
  docData?: DocumentRecord;
  activeTab?: RightPanelTab;
  selectedCommentId?: string | null;
  commentPanelScope?: "all" | "inline";
  onTabChange?: (tab: RightPanelTab) => void;
  onClose?: () => void;
  pageDetailsControls?: DocumentPageDetailsControls | null;
}

export function RightPanel({
  spaceId,
  docId,
  docData,
  activeTab: controlledActiveTab,
  selectedCommentId,
  commentPanelScope = "all",
  onTabChange,
  onClose,
  pageDetailsControls,
}: RightPanelProps) {
  const [uncontrolledActiveTab, setUncontrolledActiveTab] = useState<RightPanelTab>("properties");
  const activeTab = controlledActiveTab ?? uncontrolledActiveTab;
  const setActiveTab = useCallback(
    (tab: RightPanelTab) => {
      if (controlledActiveTab === undefined) setUncontrolledActiveTab(tab);
      onTabChange?.(tab);
    },
    [controlledActiveTab, onTabChange],
  );
  const [fetchedDoc, setFetchedDoc] = useState<DocumentRecord | null>(null);
  const [propertiesError, setPropertiesError] = useState(false);
  const [backlinks, setBacklinks] = useState<BacklinkRecord[]>([]);
  const [backlinksLoading, setBacklinksLoading] = useState(
    controlledActiveTab === "backlinks" && Boolean(spaceId && docId),
  );
  const [backlinksError, setBacklinksError] = useState<BacklinksErrorKind | null>(null);
  const [versions, setVersions] = useState<DocumentVersionRecord[]>([]);
  const [versionsError, setVersionsError] = useState(false);
  const [versionsLoading, setVersionsLoading] = useState(true);
  const [closedDiscussions, setClosedDiscussions] = useState<ClosedDraftDiscussionSummary[]>([]);
  const [closedDiscussionsLoading, setClosedDiscussionsLoading] = useState(true);
  const [closedDiscussionsError, setClosedDiscussionsError] = useState(false);
  const [selectedClosedDiscussion, setSelectedClosedDiscussion] = useState<ClosedDraftDiscussionSummary | null>(null);
  const [workflowLoading, setWorkflowLoading] = useState(true);
  const historyEpoch = useRef(0);
  const [workflow, setWorkflow] = useState<DocumentWorkflowRecord | null>(null);
  const [workflowEvents, setWorkflowEvents] = useState<DocumentWorkflowEventRecord[]>([]);
  const [workflowHistoryError, setWorkflowHistoryError] = useState(false);
  const [aiDocumentContext, setAiDocumentContext] = useState<string | undefined>();
  const [aiDocumentContextLoading, setAiDocumentContextLoading] = useState(false);
  const [aiDocumentContextError, setAiDocumentContextError] = useState(false);
  const launchMode = getCurrentLaunchMode();
  const auth = useContext(AuthContext);
  const subscribeContext = useCallback((changed: () => void) => activeTab === "history" ? subscribePublicationHistoryContext(changed) : () => {}, [activeTab]);
  const contextVersion = useSyncExternalStore(subscribeContext, publicationHistoryContextVersion, () => 0);
  const accountId = auth?.user?.id;
  const accountLoading = auth?.loading;
  const accountProviderPresent = auth !== null;
  const historyScope = JSON.stringify([spaceId, docId, contextVersion, accountId, accountLoading, activeTab]);
  const assertHistoryAccount = useCallback(() => {
    if (!accountProviderPresent || getCurrentLaunchMode() === "local") return;
    if (!accountId || accountLoading || captureOfflineServerContext().origin.accountId !== accountId) throw new Error("Current account identity is unavailable for history review.");
  }, [accountId, accountLoading, accountProviderPresent]);
  const [snapshotView, setSnapshotView] = useState<{scope: string; state: PublicationHistorySelectionState} | null>(null);
  const snapshots = useMemo(() => createPublicationHistorySelection({
    list: (...args) => { assertHistoryAccount(); return dataClient.listDocumentPublicationSnapshots(...args); },
    get: (...args) => { assertHistoryAccount(); return dataClient.getDocumentPublicationSnapshot(...args); },
    raw: (...args) => { assertHistoryAccount(); return dataClient.readDocumentPublicationSnapshotRaw(...args); },
  }, state => setSnapshotView({scope: historyScope, state})), [historyScope, assertHistoryAccount]);
  // During the first render of a changed identity/route, old source must already be absent.
  const snapshotState = snapshotView?.scope === historyScope ? snapshotView.state : snapshots.snapshot();
  const metadataScope = useRef(historyScope);
  const metadataCurrent = metadataScope.current === historyScope;
  useLayoutEffect(() => {
    metadataScope.current = historyScope;
    historyEpoch.current++;
    setVersions([]); setVersionsLoading(true); setVersionsError(false);
    setClosedDiscussions([]); setClosedDiscussionsLoading(true); setClosedDiscussionsError(false); setSelectedClosedDiscussion(null);
    setWorkflow(null); setWorkflowEvents([]); setWorkflowLoading(true); setWorkflowHistoryError(false);
    if (spaceId && docId && activeTab === "history" && launchMode !== "local") {
      snapshots.setScope(spaceId, docId); void snapshots.refresh();
    }
    return () => { snapshots.reset(); historyEpoch.current++; };
  }, [historyScope, snapshots, spaceId, docId, activeTab, launchMode]);
  const [recoveryView, setRecoveryView] = useState<{scope:string; records:SnapshotRecoveryRecord[]; intent:SnapshotRecoveryIntent|null; comparison:RecoveryComparison; busy:boolean; error?:string} | null>(null);
  const recovery = recoveryView?.scope === historyScope ? recoveryView : null;
  const recoveryBusy = useRef(false);
  const refreshRecoveries = useCallback(() => {
    if (!spaceId || !docId || activeTab !== "history" || launchMode === "local") return;
    try {
      assertHistoryAccount();
      const records = dataClient.inspectPublicationSnapshotRecoveries(spaceId, docId).filter(record => record.state !== "cancelled");
      setRecoveryView(old => ({scope:historyScope,records,intent:old?.scope === historyScope ? old.intent : null,comparison:old?.scope === historyScope ? old.comparison : {status:"unavailable",reason:"Current published comparison has not been requested."},busy:old?.scope === historyScope && old.busy || false,error:old?.scope === historyScope ? old.error : undefined}));
    } catch (error) { setRecoveryView({scope:historyScope,records:[],intent:null,comparison:{status:"unavailable",reason:"Current history identity is unavailable."},busy:false,error:error instanceof Error ? error.message : "Recovery evidence is unavailable."}); }
  }, [spaceId,docId,activeTab,launchMode,historyScope,assertHistoryAccount]);
  useEffect(() => {
    refreshRecoveries();
    const changed = (event:StorageEvent) => { if (event.key?.startsWith("starlane_snapshot_recovery_v1:")) refreshRecoveries(); };
    window.addEventListener("storage",changed);
    return () => window.removeEventListener("storage",changed);
  }, [refreshRecoveries]);
  const reviewRecovery = async (intent:SnapshotRecoveryIntent) => {
    assertHistoryAccount();
    const context = captureOfflineServerContext(), scope = historyScope;
    setRecoveryView(old => ({scope,records:old?.scope === scope ? old.records : [],intent,comparison:{status:"loading"},busy:false}));
    snapshots.close();
    let comparison:RecoveryComparison = {status:"unavailable",reason:"Current published content is unavailable with your current View access."};
    try {
      if (!intent.source.page_published) throw new Error("This page has no current published content to compare.");
      if (navigator.onLine === false) throw new Error("Reconnect to verify current published comparison access.");
      // Uncached current View admission; a matching cached body alone is not permission evidence.
      const response = await fetch(context.origin.serverBase + `/spaces/${encodeURIComponent(intent.space_id)}/documents/${encodeURIComponent(intent.document_id)}/content`, {cache:"no-store",headers:{Authorization:`Bearer ${context.bearer}`}});
      if (!response.ok) throw new Error("Current published content is unavailable with your current View access.");
      const content = await response.json();
      verifyOfflineReplayContext(context,getApiBase(),api.getToken()); assertHistoryAccount();
      if (content.id !== intent.document_id || content.published_source_digest !== intent.source.page_source_digest || typeof content.title !== "string" || typeof content.content !== "string") throw new Error("The current published page changed after this original review; comparison is unavailable.");
      comparison = {status:"ready",title:content.title,content:content.content};
    } catch (error) { comparison = {status:"unavailable",reason:error instanceof Error ? error.message : "Current published comparison is unavailable."}; }
    if (metadataScope.current !== scope) return;
    try { assertHistoryAccount(); verifyOfflineReplayContext(context,getApiBase(),api.getToken()); } catch { return; }
    setRecoveryView(old => old?.scope === scope && old.intent?.operation_id === intent.operation_id ? {...old,comparison} : old);
  };
  const runRecovery = async (run:() => Promise<unknown>, closeReview=true) => {
    if (recoveryBusy.current) return;
    const scope = historyScope;
    assertHistoryAccount(); const context = captureOfflineServerContext();
    recoveryBusy.current = true;
    setRecoveryView(old => old?.scope === scope ? {...old,busy:true,error:undefined} : old);
    let error:string|undefined;
    try { await run(); } catch (failure) { error = failure instanceof Error ? failure.message : "Original recovery outcome is unavailable."; }
    finally { recoveryBusy.current = false; }
    if (metadataScope.current !== scope) return;
    try { assertHistoryAccount(); verifyOfflineReplayContext(context,getApiBase(),api.getToken()); } catch { return; }
    refreshRecoveries();
    setRecoveryView(old => old?.scope === scope ? {...old,busy:false,error,intent:closeReview ? null : old.intent} : old);
  };
  const prepareRecovery = async () => {
    const selected = snapshotState.detail;
    if (!selected || !spaceId || !docId || recoveryBusy.current) return;
    const scope = historyScope;
    assertHistoryAccount(); const selection = captureDraftSaveSelection(accountId);
    recoveryBusy.current = true;
    try {
      const intent = await dataClient.preparePublicationSnapshotRecovery(spaceId,docId,selected,selection);
      if (metadataScope.current !== scope) return;
      assertHistoryAccount(); refreshRecoveries(); await reviewRecovery(intent);
    } catch (error) {
      if (metadataScope.current === scope) { refreshRecoveries(); setRecoveryView(old => old?.scope === scope ? {...old,error:error instanceof Error ? error.message : "Recovery review is unavailable."} : old); }
    } finally { recoveryBusy.current = false; }
  };

  const downloadSnapshotRaw = async () => {
    try {
      assertHistoryAccount();
      const context = captureOfflineServerContext();
      const originalScope = historyScope;
      const raw = await snapshots.readRaw();
      if (!raw || originalScope !== metadataScope.current) return;
      assertHistoryAccount();
      verifyOfflineReplayContext(context, getApiBase(), api.getToken());
      const url = URL.createObjectURL(new Blob([new Uint8Array(raw.bytes)], {type:"application/octet-stream"}));
      const anchor = document.createElement("a"); anchor.href = url; anchor.download = "publication-snapshot.md";
      anchor.click(); URL.revokeObjectURL(url);
    } catch { snapshots.close(); }
  };


  useEffect(() => {
    setPropertiesError(false);
    setFetchedDoc(null);
    if (!spaceId || !docId || docData) return;

    let cancelled = false;
    dataClient
      .getDocument(spaceId, docId)
      .then((nextDoc) => {
        if (!cancelled) setFetchedDoc(nextDoc);
      })
      .catch(() => {
        if (!cancelled) setPropertiesError(true);
      });

    return () => {
      cancelled = true;
    };
  }, [docData, docId, spaceId]);

  const resolvedDoc = docData ?? fetchedDoc;

  const allTabs: { id: RightPanelTab; label: string; icon: ReactNode }[] = [
    { id: "properties", label: "Properties", icon: <FileText size={14} /> },
    { id: "backlinks", label: "Backlinks", icon: <Link2 size={14} /> },
    { id: "history", label: "History", icon: <Clock3 size={14} /> },
    { id: "ai", label: "AI", icon: <Sparkles size={14} /> },
  ];
  const tabs = allTabs.filter(
    (tab) =>
      RIGHT_PANEL_NAVIGATION_TABS.includes(tab.id) && isRightPanelTabAvailable(tab.id, launchMode),
  );
  const visibleTab = isRightPanelTabAvailable(activeTab, launchMode) ? activeTab : "properties";

  useEffect(() => {
    if (!isRightPanelTabAvailable(activeTab, launchMode)) {
      setActiveTab("properties");
    }
  }, [activeTab, launchMode, setActiveTab]);

  const loadAiDocumentContext = useCallback(() => {
    setAiDocumentContext(undefined);
    setAiDocumentContextError(false);
    if (!spaceId || !docId) {
      setAiDocumentContextLoading(false);
      return () => {};
    }
    setAiDocumentContextLoading(true);
    let cancelled = false;
    void dataClient
      .getDocumentContent(spaceId, docId)
      .then((record) => {
        if (!cancelled) {
          setAiDocumentContext(formatAiDocumentContext(record.title, record.content));
        }
      })
      .catch(() => {
        if (!cancelled) setAiDocumentContextError(true);
      })
      .finally(() => {
        if (!cancelled) setAiDocumentContextLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [docId, spaceId]);

  useEffect(() => {
    if (activeTab !== "ai") return;
    return loadAiDocumentContext();
  }, [activeTab, loadAiDocumentContext]);

  const loadBacklinks = useCallback(() => {
    setBacklinks([]);
    setBacklinksError(null);
    if (!spaceId || !docId) {
      setBacklinksLoading(false);
      return () => {};
    }
    setBacklinksLoading(true);

    let cancelled = false;
    void dataClient
      .listDocumentBacklinks(spaceId, docId)
      .then((records) => {
        if (!cancelled) {
          setBacklinks(records);
          setBacklinksLoading(false);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setBacklinksError(getBacklinksErrorKind(error));
          setBacklinksLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [docId, spaceId]);

  useEffect(() => {
    if (activeTab !== "backlinks") return;
    return loadBacklinks();
  }, [activeTab, loadBacklinks]);

  const loadHistory = useCallback(() => {
    const ticket = ++historyEpoch.current;
    setVersions([]); setVersionsError(false); setVersionsLoading(true);
    setClosedDiscussions([]); setClosedDiscussionsError(false); setClosedDiscussionsLoading(launchMode !== "local");
    setWorkflow(null); setWorkflowEvents([]); setWorkflowHistoryError(false); setWorkflowLoading(true);
    if (!spaceId || !docId) return () => {};
    try { assertHistoryAccount(); } catch { setVersionsError(true); setVersionsLoading(false); setClosedDiscussionsError(true); setClosedDiscussionsLoading(false); setWorkflowHistoryError(true); setWorkflowLoading(false); return () => {}; }
    const valid = () => ticket === historyEpoch.current && publicationHistoryContextVersion() === contextVersion;
    void dataClient.listDocumentVersions(spaceId, docId).then(records => { if (valid()) setVersions(records); })
      .catch(() => { if (valid()) setVersionsError(true); }).finally(() => { if (valid()) setVersionsLoading(false); });
    if (launchMode !== "local") void listClosedDraftDiscussions(spaceId, docId).then(records => { if (valid()) setClosedDiscussions(records); })
      .catch(() => { if (valid()) setClosedDiscussionsError(true); }).finally(() => { if (valid()) setClosedDiscussionsLoading(false); });
    void Promise.all([dataClient.getDocumentWorkflow(spaceId,docId),dataClient.listDocumentWorkflowEvents(spaceId,docId)])
      .then(([nextWorkflow,nextEvents]) => { if (valid()) { setWorkflow(nextWorkflow); setWorkflowEvents(nextEvents); } })
      .catch(() => { if (valid()) setWorkflowHistoryError(true); }).finally(() => { if (valid()) setWorkflowLoading(false); });
    return () => { if (ticket === historyEpoch.current) historyEpoch.current++; };
  }, [docId, spaceId, contextVersion, historyScope, assertHistoryAccount, launchMode]);
  useEffect(() => { if (activeTab === "history") return loadHistory(); }, [activeTab, loadHistory]);

  return (
    <div className="flex h-full flex-col">
      {visibleTab !== "comments" && (
        <div
          className="border-border shrink-0 border-b"
          role="tablist"
          aria-label="Right panel tabs"
        >
          <div
            className="grid scrollbar-none overflow-x-hidden"
            style={{ gridTemplateColumns: `repeat(${tabs.length}, minmax(0, 1fr))` }}
          >
            {tabs.map((tab) => {
              const active = visibleTab === tab.id;
              return (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => {
                    if (tab.id === "backlinks" && activeTab !== "backlinks") {
                      setBacklinksLoading(true);
                    }
                    setActiveTab(tab.id);
                  }}
                  role="tab"
                  id={`right-panel-tab-${tab.id}`}
                  aria-controls={`right-panel-panel-${tab.id}`}
                  className={`flex h-10 items-center justify-center border-b-2 transition-colors duration-100 ${
                    active
                      ? "border-primary text-primary"
                      : "text-text-muted hover:text-text border-transparent"
                  } ${tab.id === "ai" && active ? "text-ai" : ""}`}
                  aria-selected={active}
                  tabIndex={active ? 0 : -1}
                  aria-label={`${tab.label} tab`}
                  title={tab.label}
                >
                  {tab.icon}
                </button>
              );
            })}
          </div>
          <div className="border-border bg-bg/35 flex h-8 items-center justify-between gap-2 border-t px-3">
            <span className="text-text truncate text-xs font-medium">
              {tabs.find((tab) => tab.id === visibleTab)?.label}
            </span>
            {onClose && (
              <button
                type="button"
                onClick={onClose}
                className="focus-ring rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-6 w-6 shrink-0 items-center justify-center transition-colors duration-100"
                aria-label={`Close ${tabs.find((tab) => tab.id === visibleTab)?.label ?? "document"} panel`}
                title="Close panel"
              >
                <X size={14} />
              </button>
            )}
          </div>
        </div>
      )}

      {/* Tab content */}
      <div className={RIGHT_PANEL_SCROLL_CONTENT_CLASS}>
        {visibleTab === "properties" && spaceId && docId && resolvedDoc && (
          <div
            role="tabpanel"
            id="right-panel-panel-properties"
            aria-labelledby="right-panel-tab-properties"
          >
            <PropertiesPanel
              spaceId={spaceId}
              docId={docId}
              title={resolvedDoc.title}
              icon={resolvedDoc.icon}
              coverImage={pageDetailsControls?.coverImage ?? resolvedDoc.cover_image}
              coverEditable={pageDetailsControls?.coverEditable ?? false}
              coverDraftId={pageDetailsControls?.coverDraftId}
              onCoverUpload={pageDetailsControls?.onCoverUpload}
              nativeCoverRecovery={pageDetailsControls?.nativeCoverRecovery}
              onCoverUpdate={pageDetailsControls?.onCoverUpdate}
              pageWidth={pageDetailsControls?.pageWidth}
              pageWidthEditable={pageDetailsControls?.pageWidthEditable ?? false}
              onPageWidthChange={pageDetailsControls?.onPageWidthChange}
              pageValuesEditable={pageDetailsControls?.pageValuesEditable ?? false}
              schemaEditable={pageDetailsControls?.schemaEditable ?? false}
              contentMetadataEditable={pageDetailsControls?.contentMetadataEditable ?? false}
              synopsis={pageDetailsControls?.synopsis}
              pageCssSource={pageDetailsControls?.pageCssSource}
              onSynopsisChange={pageDetailsControls?.onSynopsisChange}
              onSynopsisCompositionChange={pageDetailsControls?.onSynopsisCompositionChange}
              onSynopsisUpdate={pageDetailsControls?.onSynopsisUpdate}
              createdBy={resolvedDoc.created_by}
              lastUpdatedBy={resolvedDoc.last_updated_by}
              createdAt={resolvedDoc.created_at}
              updatedAt={resolvedDoc.updated_at}
              locked={resolvedDoc.locked}
              isPublic={resolvedDoc.public}
            />
          </div>
        )}

        {visibleTab === "properties" && spaceId && docId && !resolvedDoc && (
          <div
            role="tabpanel"
            id="right-panel-panel-properties"
            aria-labelledby="right-panel-tab-properties"
            className="text-text-muted p-4 text-sm"
          >
            {propertiesError ? "Unable to load properties." : "Loading properties..."}
          </div>
        )}

        {visibleTab === "comments" && spaceId && docId && (
          <div
            role="region"
            aria-label={pageDetailsControls?.draftDiscussion ? "Draft discussion" : "Page comments"}
          >
            {pageDetailsControls?.draftDiscussion ? (
              <DraftDiscussionPanel
                target={pageDetailsControls.draftDiscussion.target}
                currentUserId={pageDetailsControls.draftDiscussion.currentUserId}
                pendingAnchor={pageDetailsControls.draftDiscussion.pendingAnchor}
                onAnchorConsumed={pageDetailsControls.draftDiscussion.onAnchorConsumed}
                onClose={onClose}
              />
            ) : (
              <CommentsPanel
                spaceId={spaceId}
                docId={docId}
                selectedCommentId={selectedCommentId}
                inlineOnly={commentPanelScope === "inline"}
                onClose={onClose}
              />
            )}
          </div>
        )}

        {visibleTab === "backlinks" && spaceId && docId && (
          <div
            role="tabpanel"
            id="right-panel-panel-backlinks"
            aria-labelledby="right-panel-tab-backlinks"
            aria-busy={backlinksLoading}
            className="grid gap-3 p-3"
          >
            {backlinksLoading ? (
              <div role="status" className="text-text-muted flex items-center gap-2 py-1 text-sm">
                <LoaderCircle size={15} className="animate-spin" aria-hidden="true" />
                Finding linked pages…
              </div>
            ) : backlinksError ? (
              <BacklinksErrorState kind={backlinksError} onRetry={() => void loadBacklinks()} />
            ) : backlinks.length === 0 ? (
              <div className="grid justify-items-start gap-1.5 py-1">
                <Link2 size={18} className="text-text-muted" aria-hidden="true" />
                <p className="text-text text-sm font-medium">No pages link here yet</p>
                <p className="text-text-muted text-xs leading-5">
                  Add a wiki link from another page to connect it here.
                </p>
              </div>
            ) : (
              <div className="grid gap-2">
                <p className="text-text-muted px-1 text-xs font-medium">
                  {formatBacklinkCount(backlinks.length)}
                </p>
                <nav aria-label="Pages linking here" className="grid gap-1">
                  {backlinks.map((backlink) => (
                    <Link
                      key={backlink.document_id}
                      to={`/spaces/${backlink.space_id}/docs/${backlink.document_id}`}
                      className="focus-ring rounded-input text-text hover:bg-surface-raised hover:text-primary group flex min-h-10 items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100"
                    >
                      <FileText
                        size={15}
                        className="text-text-muted group-hover:text-primary shrink-0"
                        aria-hidden="true"
                      />
                      <span className="min-w-0 flex-1 truncate">{backlink.title}</span>
                      <ChevronRight
                        size={14}
                        className="text-text-muted shrink-0"
                        aria-hidden="true"
                      />
                    </Link>
                  ))}
                </nav>
              </div>
            )}
          </div>
        )}

        {visibleTab === "history" && spaceId && docId && (
          <div
            role="tabpanel"
            id="right-panel-panel-history"
            aria-labelledby="right-panel-tab-history"
            className="p-3"
          >
            <div className="grid gap-4">
              {launchMode !== "local" && <DocumentPublicationHistory status={snapshotState.listStatus} records={snapshotState.records} onRetry={() => void snapshots.refresh()} onPreview={summary => void snapshots.preview(summary)} />}
              {launchMode !== "local" && recovery && <section aria-label="Retained snapshot recovery requests" className="grid gap-2 text-sm">
                {recovery.error && <p role="alert" className="text-text-muted">{recovery.error}</p>}
                <SnapshotRecoveryRequests records={recovery.records} busy={recovery.busy} onReview={intent => void reviewRecovery(intent)} onCancel={id => void runRecovery(() => dataClient.cancelPreparedPublicationSnapshotRecovery(id))} onRetry={id => void runRecovery(() => dataClient.retryPublicationSnapshotRecovery(id))} />
              </section>}
              {launchMode !== "local" && <ClosedDraftDiscussionHistory records={closedDiscussions} loading={!metadataCurrent || closedDiscussionsLoading} error={closedDiscussionsError} onRetry={() => void loadHistory()} onOpen={setSelectedClosedDiscussion} />}
              {launchMode === "local" && <LocalVersionHistoryNotice workflowEventsAvailable={workflowEvents.length > 0} />}
              {launchMode !== "local" && <section aria-label="Legacy history status" className="text-text-muted grid gap-2 text-sm">
                <ServerVersionHistoryNotice />
                {!metadataCurrent || versionsLoading ? <p role="status">Loading legacy change records…</p> : versionsError ? <><p>Legacy change records are unavailable.</p><Button variant="secondary" size="sm" className="justify-self-start" onClick={() => void loadHistory()}>Retry legacy records</Button></> : versions.length === 0 ? <p>No legacy change records.</p> : null}
              </section>}
              {!metadataCurrent || workflowLoading ? <p role="status" className="text-text-muted text-sm">Loading workflow history…</p> : workflowHistoryError ? <div role="alert" className="text-text-muted grid gap-2 text-sm"><p>Workflow history is unavailable.</p><Button variant="secondary" size="sm" className="justify-self-start" onClick={() => void loadHistory()}>Retry workflow history</Button></div> : workflowEvents.length === 0 && launchMode !== "local" ? <p className="text-text-muted text-sm">No workflow events.</p> : null}
                {metadataCurrent && !workflowLoading && !workflowHistoryError && workflowEvents.length > 0 && workflow && (
                  <section aria-label="Workflow history" className="grid gap-2">
                    <div className="text-text-muted text-xs font-medium uppercase">Workflow</div>
                    <div className="grid gap-2">
                      {workflowEvents.map((event) => (
                        <div key={event.id} className="rounded-input border-border border p-2">
                          <div className="flex items-center justify-between gap-2">
                            <span className="text-text truncate text-sm">
                              {workflowEventLabel(workflow, event)}
                            </span>
                            <span className="text-text-muted shrink-0 text-xs">
                              {formatDateTime(event.created_at)}
                            </span>
                          </div>
                          <div className="text-text-muted mt-1 text-xs">
                            {event.created_by || "Unknown"}
                            {event.note ? ` · ${event.note}` : ""}
                          </div>
                        </div>
                      ))}
                    </div>
                  </section>
                )}

                {metadataCurrent && !versionsLoading && !versionsError && versions.length > 0 && (
                  <section aria-label="Legacy change records" className="grid gap-2">
                    <div className="text-text-muted text-xs font-medium uppercase">Legacy change records · metadata only</div>
                    <div className="grid gap-2">
                      {versions.map((version) => (
                        <div key={version.id} className="rounded-input border-border border p-2">
                          <div className="flex items-center justify-between gap-2">
                            <span className="text-text text-sm">
                              Version {version.version_number}
                            </span>
                            <span className="text-text-muted text-xs">
                              {formatDateTime(version.created_at)}
                            </span>
                          </div>
                          <div className="text-text-muted mt-1 text-xs">
                            {version.author_name} · +{version.lines_added} / -
                            {version.lines_removed}
                          </div>
                        </div>
                      ))}
                    </div>
                  </section>
                )}
            </div>
          </div>
        )}

        {visibleTab === "ai" && (
          <div role="tabpanel" id="right-panel-panel-ai" aria-labelledby="right-panel-tab-ai">
            <AiChatPanel
              documentContext={aiDocumentContext}
              documentContextLoading={aiDocumentContextLoading}
              documentContextError={aiDocumentContextError}
              onRetryDocumentContext={() => {
                void loadAiDocumentContext();
              }}
            />
          </div>
        )}

        {!docId && (
          <div className="flex h-full items-center justify-center p-4">
            <p className="text-text-muted text-sm">Select a document to view properties</p>
          </div>
        )}
      </div>
      {recovery?.intent && activeTab === "history" && <SnapshotRecoveryConfirmation draft={recovery.intent.source.draft} comparison={recovery.comparison} loading={false} busy={recovery.busy} originalRequest onConfirm={() => { const intent = recovery.intent; if (intent) void runRecovery(() => dataClient.recoverPreparedPublicationSnapshot(intent)); }} onClose={() => setRecoveryView(old => old?.scope === historyScope ? {...old,intent:null} : old)} />}
      {snapshotState.previewOpen && activeTab === "history" && <PublishedSnapshotPreview detail={snapshotState.detail} raw={snapshotState.raw} loading={snapshotState.previewStatus === "loading"} rawLoading={snapshotState.rawLoading} error={snapshotState.previewStatus === "denied" ? "Published snapshot history requires current Edit access." : snapshotState.previewStatus === "offline" ? "Reconnect to verify current history access." : snapshotState.previewStatus === "unavailable" ? "Published snapshot source is unavailable." : undefined} onRecover={() => void prepareRecovery()} onClose={() => snapshots.close()} onReadRaw={() => void snapshots.readRaw()} onDownloadRaw={() => void downloadSnapshotRaw()} />}
      {selectedClosedDiscussion && spaceId && docId && activeTab === "history" && <ClosedDraftDiscussionDialog spaceId={spaceId} documentId={docId} record={selectedClosedDiscussion} onClose={() => setSelectedClosedDiscussion(null)} />}
    </div>
  );
}

export function formatAiDocumentContext(title: string, content: string): string {
  const maxContentLength = 24_000;
  const normalizedTitle = title.trim() || "Untitled document";
  const normalizedContent = content.trim();
  const truncated = normalizedContent.length > maxContentLength;
  const boundedContent = normalizedContent.slice(0, maxContentLength);
  return `# ${normalizedTitle}\n\n${boundedContent}${truncated ? "\n\n[Document context truncated]" : ""}`;
}

function formatDateTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Unknown";
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function workflowStageName(workflow: DocumentWorkflowRecord, stageId: string): string {
  return workflow.stages.find((stage) => stage.id === stageId)?.name ?? "Unknown stage";
}

function workflowEventLabel(
  workflow: DocumentWorkflowRecord,
  event: DocumentWorkflowEventRecord,
): string {
  const toStage = workflowStageName(workflow, event.to_stage_id);
  if (event.action === "enable") return `Enabled at ${toStage}`;
  if (event.action === "disable") return "Workflow disabled";
  if (event.action === "delegate") return `Delegated at ${toStage}`;
  if (event.action === "escalate") return `Escalated at ${toStage}`;
  if (event.from_stage_id) {
    return `Moved from ${workflowStageName(workflow, event.from_stage_id)} to ${toStage}`;
  }
  return `Moved to ${toStage}`;
}
