import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  ArrowRight,
  Check,
  Clock3,
  Pencil,
  Plus,
  Star,
  X,
} from "lucide-react";
import { Button, RESPONSIVE_TOUCH_ACTION_CLASS } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { DocumentIdentityIcon } from "@/components/DocumentIdentityIcon";
import {
  dataClient,
  type DocumentRecord,
  type SpaceRecord,
} from "@/lib/dataClient";
import { formatDateMetadata } from "@/lib/dateMetadata";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { useWorkspaceCreateAction } from "@/components/layout/workspaceCreateActions";
import {
  buildDocumentRoutePath,
  readLastViewedDocumentRoute,
  type LastViewedDocumentRoute,
} from "@/lib/lastViewedDocument";
import { loadStarredDocuments, type StarredDocumentRecord } from "@/lib/starredDocuments";

export type SpaceHomeSection = "journal" | "recent" | "graph" | "workflows";
export const SPACE_HOME_TOUCH_ACTION_CLASS = RESPONSIVE_TOUCH_ACTION_CLASS;
export const SPACE_DESCRIPTION_MAX_LENGTH = 1000;
const EMPTY_HOME_DOCUMENTS: DocumentRecord[] = [];
const EMPTY_HOME_STARRED_DOCUMENTS: StarredDocumentRecord[] = [];

export function canEditSpaceDescription(memberRole?: string): boolean {
  return ["admin", "owner", "space_admin"].includes(memberRole ?? "");
}

export function normalizeSpaceDescription(value: string): string {
  return value.trim();
}

export function getSpaceHomeDocumentPath(spaceId: string, documentId: string): string {
  return buildDocumentRoutePath(spaceId, documentId);
}

export function getSpaceHomeSectionPath(spaceId: string, section: SpaceHomeSection): string {
  return `/spaces/${encodeURIComponent(spaceId)}/${section}`;
}

export function sortSpaceHomeDocuments(documents: DocumentRecord[]): DocumentRecord[] {
  return [...documents].sort((a, b) => {
    const updatedComparison = (b.updated_at ?? "").localeCompare(a.updated_at ?? "");
    if (updatedComparison !== 0) return updatedComparison;
    return a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
  });
}

export function getSpaceHomeContinueDocument(
  spaceId: string,
  documents: DocumentRecord[],
  lastViewed: LastViewedDocumentRoute | null,
): DocumentRecord | undefined {
  if (!lastViewed || lastViewed.spaceId !== spaceId) return undefined;
  return documents.find((document) => document.id === lastViewed.docId);
}

export function getSpaceHomeQuickAccessDocuments(
  spaceId: string,
  documents: DocumentRecord[],
  starred: StarredDocumentRecord[],
  excludedDocumentId?: string,
): DocumentRecord[] {
  const documentById = new Map(documents.map((document) => [document.id, document]));
  return starred
    .filter((item) => item.spaceId === spaceId)
    .map((item) => documentById.get(item.id))
    .filter((document): document is DocumentRecord => Boolean(document))
    .filter((document) => document.id !== excludedDocumentId)
    .slice(0, 4);
}

export function getSpaceHomeRecentDocuments(
  documents: DocumentRecord[],
  continueDocument: DocumentRecord | undefined,
  quickAccessDocuments: DocumentRecord[],
): DocumentRecord[] {
  const excludedIds = new Set(quickAccessDocuments.map((document) => document.id));
  if (continueDocument) excludedIds.add(continueDocument.id);

  return documents.filter((document) => !excludedIds.has(document.id)).slice(0, 6);
}

export function shouldShowContinueWorking(document: DocumentRecord | undefined): boolean {
  return Boolean(document);
}

export function shouldShowQuickAccess(documents: DocumentRecord[]): boolean {
  return documents.length > 0;
}

export function shouldShowRecentPages(documents: DocumentRecord[]): boolean {
  return documents.length > 0;
}

export function shouldShowStartWriting(
  hasLoadedCurrentSpace: boolean,
  documents: DocumentRecord[],
): boolean {
  return hasLoadedCurrentSpace && isEmptySpaceHome(documents);
}

export async function settleCurrentSpaceHomeRequest<T>(
  request: Promise<T>,
  isCurrent: () => boolean,
  onSuccess: (result: T) => void | Promise<void>,
  onError: () => void | Promise<void>,
  onSettled: () => void | Promise<void>,
): Promise<void> {
  try {
    const result = await request;
    if (isCurrent()) await onSuccess(result);
  } catch {
    if (isCurrent()) await onError();
  } finally {
    if (isCurrent()) await onSettled();
  }
}

export function getSpaceHomeContextLabel(space: SpaceRecord, localMode: boolean): string {
  if (localMode) return "Local space · Owner";

  const visibilityLabels: Record<string, string> = {
    private: "Private",
    internal: "Internal",
    public: "Public",
  };
  const roleLabels: Record<string, string> = {
    admin: "Admin",
    owner: "Owner",
    space_admin: "Space admin",
    editor: "Editor",
    viewer: "Viewer",
  };
  const visibility = visibilityLabels[space.visibility ?? ""] ?? "Visibility unavailable";
  const role = roleLabels[space.member_role ?? ""] ?? "Access role unavailable";
  return `${visibility} · ${role}`;
}

export function isEmptySpaceHome(documents: DocumentRecord[]): boolean {
  return documents.length === 0;
}

export function shouldShowSpaceHomeHeaderCreateAction(documents: DocumentRecord[]): boolean {
  return !isEmptySpaceHome(documents);
}

export function getSpaceDescriptionActionLabel(description?: string): "Add description" | "Edit" {
  return description?.trim() ? "Edit" : "Add description";
}

export function SpaceHomePage() {
  const { spaceId } = useParams<{ spaceId: string }>();
  const navigate = useNavigate();
  const [space, setSpace] = useState<SpaceRecord | null>(null);
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [starredDocuments, setStarredDocuments] = useState<StarredDocumentRecord[]>([]);
  const [loadedSpaceId, setLoadedSpaceId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [editingDescription, setEditingDescription] = useState(false);
  const [descriptionDraft, setDescriptionDraft] = useState("");
  const [descriptionError, setDescriptionError] = useState("");
  const [savingDescription, setSavingDescription] = useState(false);
  const requestWorkspaceCreateAction = useWorkspaceCreateAction();
  const homeRequestIdRef = useRef(0);
  const activeSpaceIdRef = useRef(spaceId);
  activeSpaceIdRef.current = spaceId;

  const refresh = async () => {
    if (!spaceId) return;
    const requestId = ++homeRequestIdRef.current;
    const requestSpaceId = spaceId;
    setLoading(true);
    setError("");
    const request = Promise.all([
      dataClient.getSpace(spaceId),
      dataClient.listDocuments(spaceId, { include_nested: true }),
      loadStarredDocuments(spaceId),
    ] as const);
    await settleCurrentSpaceHomeRequest(
      request,
      () =>
        requestId === homeRequestIdRef.current &&
        requestSpaceId === activeSpaceIdRef.current,
      ([nextSpace, nextDocuments, nextStarredDocuments]) => {
        setSpace(nextSpace);
        setDescriptionDraft(nextSpace.description ?? "");
        setDocuments(sortSpaceHomeDocuments(nextDocuments));
        setStarredDocuments(nextStarredDocuments);
        setLoadedSpaceId(requestSpaceId);
      },
      () => {
        setLoadedSpaceId(null);
        setError("Unable to load this space.");
      },
      () => setLoading(false),
    );
  };

  useEffect(() => {
    void refresh();
    return () => {
      homeRequestIdRef.current += 1;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [spaceId]);

  const hasLoadedCurrentSpace = Boolean(spaceId) && loadedSpaceId === spaceId;
  const currentSpace = hasLoadedCurrentSpace ? space : null;
  const currentDocuments = useMemo(
    () => (hasLoadedCurrentSpace ? documents : EMPTY_HOME_DOCUMENTS),
    [documents, hasLoadedCurrentSpace],
  );
  const currentStarredDocuments = useMemo(
    () => (hasLoadedCurrentSpace ? starredDocuments : EMPTY_HOME_STARRED_DOCUMENTS),
    [hasLoadedCurrentSpace, starredDocuments],
  );

  const continueDocument = useMemo(
    () =>
      getSpaceHomeContinueDocument(spaceId ?? "", currentDocuments, readLastViewedDocumentRoute()),
    [currentDocuments, spaceId],
  );
  const quickAccessDocuments = useMemo(
    () =>
      getSpaceHomeQuickAccessDocuments(
        spaceId ?? "",
        currentDocuments,
        currentStarredDocuments,
        continueDocument?.id,
      ),
    [continueDocument?.id, currentDocuments, currentStarredDocuments, spaceId],
  );
  const recentDocuments = useMemo(
    () => getSpaceHomeRecentDocuments(currentDocuments, continueDocument, quickAccessDocuments),
    [currentDocuments, continueDocument, quickAccessDocuments],
  );
  const emptySpaceHome = shouldShowStartWriting(hasLoadedCurrentSpace, currentDocuments);
  const spaceContextLabel = currentSpace
    ? getSpaceHomeContextLabel(currentSpace, getCurrentLaunchMode() === "local")
    : "Space access details unavailable";

  const requestDocumentCreation = () => {
    requestWorkspaceCreateAction("document");
  };

  const cancelDescriptionEdit = () => {
    setDescriptionDraft(currentSpace?.description ?? "");
    setDescriptionError("");
    setEditingDescription(false);
  };

  const saveDescription = async () => {
    if (!spaceId || !currentSpace) return;
    const description = normalizeSpaceDescription(descriptionDraft);
    setSavingDescription(true);
    setDescriptionError("");
    try {
      const updated = await dataClient.updateSpace(spaceId, { description });
      setSpace(updated);
      setDescriptionDraft(updated.description ?? "");
      setEditingDescription(false);
    } catch (saveError) {
      setDescriptionError(
        saveError instanceof Error ? saveError.message : "Unable to save the space description.",
      );
    } finally {
      setSavingDescription(false);
    }
  };

  if (loading || (!hasLoadedCurrentSpace && !error)) {
    return (
      <div className="text-text-muted flex h-full items-center justify-center text-sm">
        Loading space...
      </div>
    );
  }

  return (
    <div className="bg-bg min-h-full">
      <div className="mx-auto max-w-3xl px-4 py-6 sm:px-6 sm:py-10">
        {error && (
          <div className="rounded-input border-danger/30 bg-danger/10 text-danger mb-6 border px-3 py-2 text-sm">
            {error}
          </div>
        )}

        <header className="border-border border-b pb-6">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0 flex-1">
              <h1 className="text-text break-words text-2xl font-semibold tracking-tight sm:text-3xl">
                {currentSpace?.name ?? "Space"}
              </h1>
              <p className="text-text-muted mt-1 text-xs">{spaceContextLabel}</p>
              {editingDescription ? (
                <form
                  className="mt-4 grid gap-3"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void saveDescription();
                  }}
                >
                  <label htmlFor="space-overview-description" className="sr-only">
                    Space description
                  </label>
                  <Textarea
                    id="space-overview-description"
                    value={descriptionDraft}
                    onChange={(event) => {
                      setDescriptionDraft(event.target.value);
                      setDescriptionError("");
                    }}
                    maxLength={SPACE_DESCRIPTION_MAX_LENGTH}
                    rows={4}
                    placeholder="Describe this space and what belongs here."
                    autoFocus
                  />
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <span className="text-text-muted text-xs">
                      {descriptionDraft.length} / {SPACE_DESCRIPTION_MAX_LENGTH}
                    </span>
                    <div className="flex items-center gap-2">
                      <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        className={SPACE_HOME_TOUCH_ACTION_CLASS}
                        onClick={cancelDescriptionEdit}
                        disabled={savingDescription}
                      >
                        <X size={14} aria-hidden="true" />
                        Cancel
                      </Button>
                      <Button
                        type="submit"
                        size="sm"
                        className={SPACE_HOME_TOUCH_ACTION_CLASS}
                        disabled={savingDescription}
                      >
                        <Check size={14} aria-hidden="true" />
                        {savingDescription ? "Saving..." : "Save"}
                      </Button>
                    </div>
                  </div>
                  {descriptionError && (
                    <div
                      role="alert"
                      className="rounded-input border-danger/30 bg-danger/10 text-danger border px-3 py-2 text-sm"
                    >
                      {descriptionError}
                    </div>
                  )}
                </form>
              ) : (
                <div className="mt-3 flex items-start gap-2">
                  {currentSpace?.description?.trim() ? (
                    <p className="text-text-muted min-w-0 flex-1 whitespace-pre-wrap text-sm leading-6 sm:text-base">
                      {currentSpace.description.trim()}
                    </p>
                  ) : !canEditSpaceDescription(currentSpace?.member_role) ? (
                    <p className="text-text-muted min-w-0 flex-1 text-sm leading-6 sm:text-base">
                      No description has been added yet.
                    </p>
                  ) : null}
                  {canEditSpaceDescription(currentSpace?.member_role) && (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className={`${
                        currentSpace?.description?.trim() ? "shrink-0" : "-ml-2"
                      } ${SPACE_HOME_TOUCH_ACTION_CLASS}`}
                      onClick={() => {
                        setDescriptionDraft(currentSpace?.description ?? "");
                        setDescriptionError("");
                        setEditingDescription(true);
                      }}
                    >
                      <Pencil size={14} aria-hidden="true" />
                      {getSpaceDescriptionActionLabel(currentSpace?.description)}
                    </Button>
                  )}
                </div>
              )}
            </div>
            {hasLoadedCurrentSpace && shouldShowSpaceHomeHeaderCreateAction(currentDocuments) && (
              <div className="flex w-full flex-wrap items-center gap-2 sm:w-auto sm:pt-1">
                <Button
                  type="button"
                  variant="primary"
                  size="sm"
                  className={SPACE_HOME_TOUCH_ACTION_CLASS}
                  onClick={requestDocumentCreation}
                >
                  <Plus size={14} />
                  New page
                </Button>
              </div>
            )}
          </div>
        </header>

        <main className="pt-8">
          {shouldShowContinueWorking(continueDocument) && continueDocument && (
            <section className="border-border border-b pb-7" aria-labelledby="continue-working-title">
              <div className="mb-3 flex items-center gap-2">
                <Clock3 size={16} className="text-text-muted" aria-hidden="true" />
                <h2 id="continue-working-title" className="text-text text-base font-semibold">
                  Continue working
                </h2>
              </div>
              <button
                type="button"
                onClick={() => {
                  if (spaceId) navigate(getSpaceHomeDocumentPath(spaceId, continueDocument.id));
                }}
                className="hover:bg-surface-raised focus-ring flex min-h-16 w-full items-center gap-3 text-left transition-colors"
              >
                <DocumentIdentityIcon icon={continueDocument.icon} />
                <span className="min-w-0 flex-1">
                  <span className="text-text block truncate text-sm font-medium">
                    {continueDocument.title}
                  </span>
                  <span className="text-text-muted block truncate text-xs">
                    {formatDateMetadata(
                      continueDocument.updated_at,
                      "Updated",
                      "Update date unavailable",
                    )}
                  </span>
                </span>
                <ArrowRight size={16} className="text-text-muted shrink-0" aria-hidden="true" />
              </button>
            </section>
          )}

          {shouldShowQuickAccess(quickAccessDocuments) && (
            <section className="border-border border-b py-7" aria-labelledby="quick-access-title">
              <div className="mb-3 flex items-center justify-between gap-2">
                <span className="flex items-center gap-2">
                  <Star size={16} className="text-text-muted" aria-hidden="true" />
                  <h2 id="quick-access-title" className="text-text text-base font-semibold">
                    Favorites
                  </h2>
                </span>
                <button
                  type="button"
                  onClick={() => {
                    if (spaceId) navigate(`/spaces/${encodeURIComponent(spaceId)}/starred`);
                  }}
                  className={`text-primary hover:bg-surface-raised focus-ring rounded-button px-2 py-1 text-sm ${SPACE_HOME_TOUCH_ACTION_CLASS}`}
                >
                  Favorites
                </button>
              </div>
              <div className="divide-border divide-y">
                {quickAccessDocuments.map((document) => (
                  <button
                    key={document.id}
                    type="button"
                    onClick={() => {
                      if (spaceId) navigate(getSpaceHomeDocumentPath(spaceId, document.id));
                    }}
                    className="hover:bg-surface-raised focus-ring flex min-h-11 w-full items-center gap-3 text-left transition-colors"
                  >
                    <DocumentIdentityIcon icon={document.icon} />
                    <span className="text-text min-w-0 flex-1 truncate text-sm font-medium">
                      {document.title}
                    </span>
                  </button>
                ))}
              </div>
            </section>
          )}

          {emptySpaceHome ? (
            <section className="border-border border-b py-10" aria-labelledby="start-writing-title">
              <h2 id="start-writing-title" className="text-text text-lg font-semibold">
                Start writing
              </h2>
              <p className="text-text-muted mt-2 max-w-xl text-sm leading-6">
                Create the first page in this space to give your work a home.
              </p>
              <Button
                type="button"
                variant="primary"
                size="sm"
                className={`mt-5 ${SPACE_HOME_TOUCH_ACTION_CLASS}`}
                onClick={requestDocumentCreation}
              >
                <Plus size={14} />
                New page
              </Button>
            </section>
          ) : shouldShowRecentPages(recentDocuments) ? (
            <section className="border-border border-b py-7" aria-labelledby="recent-pages-title">
              <div className="mb-3 flex items-center justify-between gap-2">
                <h2 id="recent-pages-title" className="text-text text-base font-semibold">
                  Recent pages
                </h2>
                <button
                  type="button"
                  onClick={() => {
                    if (spaceId) navigate(getSpaceHomeSectionPath(spaceId, "recent"));
                  }}
                  className={`text-primary hover:bg-surface-raised focus-ring rounded-button px-2 py-1 text-sm ${SPACE_HOME_TOUCH_ACTION_CLASS}`}
                >
                  View all
                </button>
              </div>
              <div className="divide-border divide-y">
                {recentDocuments.map((document) => (
                  <button
                    key={document.id}
                    type="button"
                    onClick={() => {
                      if (spaceId) navigate(getSpaceHomeDocumentPath(spaceId, document.id));
                    }}
                    className="hover:bg-surface-raised focus-ring flex min-h-14 w-full items-center gap-3 text-left transition-colors"
                  >
                    <DocumentIdentityIcon icon={document.icon} />
                    <span className="min-w-0 flex-1">
                      <span className="text-text block truncate text-sm font-medium">
                        {document.title}
                      </span>
                      <span className="text-text-muted block truncate text-xs">
                        {formatDateMetadata(
                          document.updated_at,
                          "Updated",
                          "Update date unavailable",
                        )}
                      </span>
                    </span>
                  </button>
                ))}
              </div>
            </section>
          ) : null}
        </main>
      </div>
    </div>
  );
}
