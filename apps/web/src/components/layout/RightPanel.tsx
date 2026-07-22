import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { Clock3, FileText, Link2, ListTree, MessageSquare, RotateCcw, Sparkles } from "lucide-react";
import { PropertiesPanel } from "@/components/PropertiesPanel";
import { CommentsPanel } from "@/components/CommentsPanel";
import { AiChatPanel } from "@/components/AiChatPanel";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import {
  dataClient,
  type BacklinkRecord,
  type DocumentRecord,
  type DocumentVersionRecord,
  type DocumentWorkflowEventRecord,
  type DocumentWorkflowRecord,
} from "@/lib/dataClient";
import { parseDocumentHeadings } from "@/lib/documentOutline";
import { getCurrentLaunchMode, type AppLaunchMode } from "@/lib/launchMode";

type Tab = "properties" | "contents" | "backlinks" | "history" | "comments" | "ai";
const SERVER_ONLY_TABS = new Set<Tab>(["comments", "ai"]);
export const RIGHT_PANEL_SCROLL_CONTENT_CLASS = "flex-1 overflow-y-auto";

export function getRestoreVersionConfirmationMessage(versionNumber: number): string {
  return `Restore version ${versionNumber}? This creates a new history checkpoint.`;
}

export function isRightPanelTabAvailable(
  tab: Tab,
  launchMode: AppLaunchMode = getCurrentLaunchMode(),
): boolean {
  return launchMode !== "local" || !SERVER_ONLY_TABS.has(tab);
}

interface RightPanelProps {
  spaceId?: string;
  docId?: string;
  docData?: DocumentRecord;
}

export function RightPanel({ spaceId, docId, docData }: RightPanelProps) {
  const [activeTab, setActiveTab] = useState<Tab>("properties");
  const [fetchedDoc, setFetchedDoc] = useState<DocumentRecord | null>(null);
  const [propertiesError, setPropertiesError] = useState(false);
  const [contents, setContents] = useState("");
  const [contentsError, setContentsError] = useState(false);
  const [backlinks, setBacklinks] = useState<BacklinkRecord[]>([]);
  const [backlinksError, setBacklinksError] = useState(false);
  const [versions, setVersions] = useState<DocumentVersionRecord[]>([]);
  const [versionsError, setVersionsError] = useState(false);
  const [restoringVersionId, setRestoringVersionId] = useState("");
  const [restoreVersionTarget, setRestoreVersionTarget] =
    useState<DocumentVersionRecord | null>(null);
  const [historyMessage, setHistoryMessage] = useState("");
  const [workflow, setWorkflow] = useState<DocumentWorkflowRecord | null>(null);
  const [workflowEvents, setWorkflowEvents] = useState<DocumentWorkflowEventRecord[]>([]);
  const [workflowHistoryError, setWorkflowHistoryError] = useState(false);
  const [aiDocumentContext, setAiDocumentContext] = useState<string | undefined>();
  const [aiDocumentContextLoading, setAiDocumentContextLoading] = useState(false);
  const [aiDocumentContextError, setAiDocumentContextError] = useState(false);
  const launchMode = getCurrentLaunchMode();

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
  const headings = parseDocumentHeadings(contents);

  const allTabs: { id: Tab; label: string; icon: ReactNode }[] = [
    { id: "properties", label: "Properties", icon: <FileText size={14} /> },
    { id: "contents", label: "Contents", icon: <ListTree size={14} /> },
    { id: "backlinks", label: "Backlinks", icon: <Link2 size={14} /> },
    { id: "history", label: "History", icon: <Clock3 size={14} /> },
    { id: "comments", label: "Comments", icon: <MessageSquare size={14} /> },
    { id: "ai", label: "AI", icon: <Sparkles size={14} /> },
  ];
  const tabs = allTabs.filter((tab) => isRightPanelTabAvailable(tab.id, launchMode));
  const visibleTab = isRightPanelTabAvailable(activeTab, launchMode) ? activeTab : "properties";

  useEffect(() => {
    if (!isRightPanelTabAvailable(activeTab, launchMode)) {
      setActiveTab("properties");
    }
  }, [activeTab, launchMode]);

  useEffect(() => {
    setContents("");
    setContentsError(false);
    if (activeTab !== "contents" || !spaceId || !docId) return;

    let cancelled = false;
    dataClient
      .getDocumentContent(spaceId, docId)
      .then((record) => {
        if (!cancelled) setContents(record.content);
      })
      .catch(() => {
        if (!cancelled) setContentsError(true);
      });

    return () => {
      cancelled = true;
    };
  }, [activeTab, docId, spaceId]);

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
    setBacklinksError(false);
    if (!spaceId || !docId) return () => {};

    let cancelled = false;
    void dataClient
      .listDocumentBacklinks(spaceId, docId)
      .then((records) => {
        if (!cancelled) setBacklinks(records);
      })
      .catch(() => {
        if (!cancelled) setBacklinksError(true);
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
    setVersions([]);
    setVersionsError(false);
    setWorkflow(null);
    setWorkflowEvents([]);
    setWorkflowHistoryError(false);
    if (!spaceId || !docId) return () => {};

    let cancelled = false;
    void Promise.all([
      dataClient
        .listDocumentVersions(spaceId, docId)
        .then((records) => {
          if (!cancelled) setVersions(records);
        })
        .catch(() => {
          if (!cancelled) setVersionsError(true);
        }),
      Promise.all([
        dataClient.getDocumentWorkflow(spaceId, docId),
        dataClient.listDocumentWorkflowEvents(spaceId, docId),
      ])
        .then(([nextWorkflow, nextEvents]) => {
          if (cancelled) return;
          setWorkflow(nextWorkflow);
          setWorkflowEvents(nextEvents);
        })
        .catch(() => {
          if (!cancelled) setWorkflowHistoryError(true);
        }),
    ]);

    return () => {
      cancelled = true;
    };
  }, [docId, spaceId]);

  useEffect(() => {
    setHistoryMessage("");
    if (activeTab !== "history") return;
    return loadHistory();
  }, [activeTab, loadHistory]);

  function restoreVersion(version: DocumentVersionRecord) {
    if (!spaceId || !docId) return;
    setRestoreVersionTarget(version);
  }

  async function confirmRestoreVersion() {
    if (!spaceId || !docId || !restoreVersionTarget) return;
    const version = restoreVersionTarget;
    setRestoringVersionId(version.id);
    setHistoryMessage("");
    try {
      const restored = await dataClient.restoreDocumentVersion(spaceId, docId, version.id);
      const nextVersions = await dataClient.listDocumentVersions(spaceId, docId);
      setVersions(nextVersions);
      setHistoryMessage(`Restored as version ${restored.version_number}.`);
    } catch {
      setHistoryMessage("Unable to restore that version.");
    } finally {
      setRestoringVersionId("");
      setRestoreVersionTarget(null);
    }
  }

  return (
    <div className="flex h-full flex-col">
      {restoreVersionTarget && (
        <ConfirmDialog
          labelledBy="restore-version-title"
          title="Restore version"
          message={getRestoreVersionConfirmationMessage(restoreVersionTarget.version_number)}
          confirmLabel="Restore"
          onCancel={() => setRestoreVersionTarget(null)}
          onConfirm={() => void confirmRestoreVersion()}
        />
      )}
      {/* Tab bar */}
      <div className="shrink-0 border-b border-border" role="tablist" aria-label="Right panel tabs">
        <div
          className="scrollbar-none grid overflow-x-hidden"
          style={{ gridTemplateColumns: `repeat(${tabs.length}, minmax(0, 1fr))` }}
        >
          {tabs.map((tab) => {
            const active = visibleTab === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                role="tab"
                id={`right-panel-tab-${tab.id}`}
                aria-controls={`right-panel-panel-${tab.id}`}
                className={`flex h-10 items-center justify-center border-b-2 transition-colors duration-100 ${
                  active
                    ? "border-primary text-primary"
                    : "border-transparent text-text-muted hover:text-text"
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
        <div className="border-border bg-bg/35 flex h-8 items-center border-t px-3">
          <span className="truncate text-xs font-medium text-text">
            {tabs.find((tab) => tab.id === visibleTab)?.label}
          </span>
        </div>
      </div>

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
              coverImage={resolvedDoc.cover_image}
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
            className="p-4 text-sm text-text-muted"
          >
            {propertiesError ? "Unable to load properties." : "Loading properties..."}
          </div>
        )}

        {visibleTab === "comments" && spaceId && docId && (
          <div
            role="tabpanel"
            id="right-panel-panel-comments"
            aria-labelledby="right-panel-tab-comments"
          >
            <CommentsPanel spaceId={spaceId} docId={docId} />
          </div>
        )}

        {visibleTab === "contents" && spaceId && docId && (
          <div
            role="tabpanel"
            id="right-panel-panel-contents"
            aria-labelledby="right-panel-tab-contents"
            className="p-3"
          >
            {contentsError ? (
              <p className="text-sm text-text-muted">Unable to load contents.</p>
            ) : headings.length === 0 ? (
              <p className="text-sm text-text-muted">No headings in this document.</p>
            ) : (
              <nav aria-label="Document contents" className="grid gap-1">
                {headings.map((heading) => (
                  <div
                    key={heading.id}
                    className="truncate rounded-button px-2 py-1 text-xs text-text-muted"
                    style={{ paddingLeft: `${(heading.level - 1) * 12 + 8}px` }}
                    title={heading.text}
                  >
                    {heading.text}
                  </div>
                ))}
              </nav>
            )}
          </div>
        )}

        {visibleTab === "backlinks" && spaceId && docId && (
          <div
            role="tabpanel"
            id="right-panel-panel-backlinks"
            aria-labelledby="right-panel-tab-backlinks"
            className="grid gap-3 p-3"
          >
            {backlinksError ? (
              <div className="grid gap-3 text-sm text-text-muted">
                <p>Unable to load backlinks.</p>
                <Button variant="secondary" size="sm" className="justify-self-start" onClick={() => void loadBacklinks()}>
                  Retry
                </Button>
              </div>
            ) : backlinks.length === 0 ? (
              <p className="text-sm text-text-muted">No documents link here yet.</p>
            ) : (
              <nav aria-label="Documents linking here" className="grid gap-1">
                {backlinks.map((backlink) => (
                  <Link
                    key={backlink.document_id}
                    to={`/spaces/${backlink.space_id}/docs/${backlink.document_id}`}
                    className="rounded-input px-2 py-1.5 text-sm text-text hover:bg-surface-raised hover:text-primary"
                  >
                    {backlink.title}
                  </Link>
                ))}
              </nav>
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
            {versionsError || workflowHistoryError ? (
              <div className="grid gap-3 text-sm text-text-muted">
                <p>Unable to load history.</p>
                <Button variant="secondary" size="sm" className="justify-self-start" onClick={() => void loadHistory()}>
                  Retry
                </Button>
              </div>
            ) : versions.length === 0 && workflowEvents.length === 0 ? (
              <p className="text-sm text-text-muted">No saved history yet.</p>
            ) : (
              <div className="grid gap-4">
                {historyMessage && (
                  <div className="rounded-input border border-border bg-surface px-2 py-1.5 text-xs text-text">
                    {historyMessage}
                  </div>
                )}
                {workflowEvents.length > 0 && workflow && (
                  <section aria-label="Workflow history" className="grid gap-2">
                    <div className="text-xs font-medium uppercase text-text-muted">
                      Workflow
                    </div>
                    <div className="grid gap-2">
                      {workflowEvents.map((event) => (
                        <div key={event.id} className="rounded-input border border-border p-2">
                          <div className="flex items-center justify-between gap-2">
                            <span className="truncate text-sm text-text">
                              {workflowEventLabel(workflow, event)}
                            </span>
                            <span className="shrink-0 text-xs text-text-muted">
                              {formatDateTime(event.created_at)}
                            </span>
                          </div>
                          <div className="mt-1 text-xs text-text-muted">
                            {event.created_by || "Unknown"}
                            {event.note ? ` · ${event.note}` : ""}
                          </div>
                        </div>
                      ))}
                    </div>
                  </section>
                )}

                {versions.length > 0 && (
                  <section aria-label="Version history" className="grid gap-2">
                    <div className="text-xs font-medium uppercase text-text-muted">
                      Versions
                    </div>
                    <div className="grid gap-2">
                      {versions.map((version) => (
                        <div key={version.id} className="rounded-input border border-border p-2">
                          <div className="flex items-center justify-between gap-2">
                            <span className="text-sm text-text">
                              Version {version.version_number}
                            </span>
                            <span className="text-xs text-text-muted">
                              {formatDateTime(version.created_at)}
                            </span>
                          </div>
                          <div className="mt-1 text-xs text-text-muted">
                            {version.author_name} · +{version.lines_added} / -
                            {version.lines_removed}
                          </div>
                          <button
                            type="button"
                            onClick={() => void restoreVersion(version)}
                            disabled={Boolean(restoringVersionId)}
                            className="mt-2 inline-flex items-center gap-1 rounded-input border border-border px-2 py-1 text-xs text-text-muted hover:bg-surface-raised hover:text-text disabled:opacity-60"
                          >
                            <RotateCcw size={12} />
                            {restoringVersionId === version.id ? "Restoring..." : "Restore"}
                          </button>
                        </div>
                      ))}
                    </div>
                  </section>
                )}
              </div>
            )}
          </div>
        )}

        {visibleTab === "ai" && (
          <div
            role="tabpanel"
            id="right-panel-panel-ai"
            aria-labelledby="right-panel-tab-ai"
          >
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
            <p className="text-sm text-text-muted">Select a document to view properties</p>
          </div>
        )}
      </div>
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
