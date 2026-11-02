import { api } from "./api";

export interface DraftDiscussionTarget {
  spaceId: string;
  documentId: string;
  draftId: string;
  sourceDigest: string;
}

export interface DraftDiscussionAnchor {
  source_digest: string;
  selection_start: number;
  selection_end: number;
  selection_text: string;
}

export interface DraftDiscussionComment {
  id: string;
  draft_id: string;
  source_digest: string;
  parent_id: string | null;
  user_id: string;
  user_name: string;
  content: string;
  selection_start: number | null;
  selection_end: number | null;
  selection_text: string | null;
  anchor_outdated: boolean;
  resolved: boolean;
  resolved_by: string | null;
  created_at: string;
  updated_at: string;
  replies: DraftDiscussionComment[];
}

export interface DraftDiscussionResponse {
  discussion_id: string | null;
  draft_id: string;
  source_digest: string;
  state: "active" | "closed";
  mention_candidates: DraftDiscussionMentionCandidate[];
  comments: DraftDiscussionComment[];
}

export interface DraftDiscussionMentionCandidate {
  user_id: string;
  display_name: string;
  email: string;
}

export interface DraftDiscussionCreated {
  id: string;
  discussion_id: string;
}

export interface ClosedDraftDiscussionSummary {
  id: string;
  draft_id: string;
  publication_operation_id: string;
  closed_at: string;
  comment_count: number;
  unresolved_count: number;
}

export interface ClosedDraftDiscussionList {
  schema_version: 1;
  space_id: string;
  document_id: string;
  discussions: ClosedDraftDiscussionSummary[];
}

export function closedDraftDiscussionsRoute(spaceId: string, documentId: string): string {
  return `/spaces/${encodeURIComponent(spaceId)}/documents/${encodeURIComponent(documentId)}/draft-discussions`;
}

export async function listClosedDraftDiscussions(
  spaceId: string,
  documentId: string,
): Promise<ClosedDraftDiscussionSummary[]> {
  const response = await api.fetch<ClosedDraftDiscussionList>(
    closedDraftDiscussionsRoute(spaceId, documentId),
  );
  if (
    response.schema_version !== 1 ||
    response.space_id !== spaceId ||
    response.document_id !== documentId ||
    response.discussions.length > 100
  ) {
    throw new Error("Closed draft discussion custody does not match the current page");
  }
  return response.discussions;
}

export async function getClosedDraftDiscussion(
  spaceId: string,
  documentId: string,
  discussionId: string,
): Promise<DraftDiscussionResponse> {
  const response = await api.fetch<DraftDiscussionResponse>(
    `${closedDraftDiscussionsRoute(spaceId, documentId)}/${encodeURIComponent(discussionId)}`,
  );
  if (response.discussion_id !== discussionId || response.state !== "closed") {
    throw new Error("Closed draft discussion response does not match the selected custody record");
  }
  return response;
}

export function draftDiscussionRoute(target: DraftDiscussionTarget): string {
  return `/spaces/${encodeURIComponent(target.spaceId)}/documents/${encodeURIComponent(target.documentId)}/drafts/${encodeURIComponent(target.draftId)}/discussion`;
}

export function draftDiscussionCommentRoute(
  target: DraftDiscussionTarget,
  commentId: string,
): string {
  return `${draftDiscussionRoute(target)}/${encodeURIComponent(commentId)}`;
}

function sourceQuery(target: DraftDiscussionTarget): string {
  return `source_digest=${encodeURIComponent(target.sourceDigest)}`;
}

export async function listDraftDiscussion(
  target: DraftDiscussionTarget,
): Promise<DraftDiscussionResponse> {
  const discussion = await api.fetch<DraftDiscussionResponse>(
    `${draftDiscussionRoute(target)}?${sourceQuery(target)}`,
  );
  if (
    discussion.draft_id !== target.draftId ||
    discussion.source_digest !== target.sourceDigest ||
    discussion.state !== "active" ||
    discussion.mention_candidates.length > 100
  ) {
    throw new Error("Draft discussion response does not match the selected draft source");
  }
  return discussion;
}

export async function createDraftDiscussionComment(
  target: DraftDiscussionTarget,
  content: string,
  anchor: DraftDiscussionAnchor | null = null,
  parentId: string | null = null,
): Promise<DraftDiscussionCreated> {
  return api.fetch<DraftDiscussionCreated>(draftDiscussionRoute(target), {
    method: "POST",
    body: {
      source_digest: target.sourceDigest,
      content,
      parent_id: parentId,
      selection_start: anchor?.selection_start ?? null,
      selection_end: anchor?.selection_end ?? null,
      selection_text: anchor?.selection_text ?? null,
    },
  });
}

export async function updateDraftDiscussionComment(
  target: DraftDiscussionTarget,
  commentId: string,
  content: string,
): Promise<void> {
  await api.fetch<void>(draftDiscussionCommentRoute(target, commentId), {
    method: "PUT",
    body: { source_digest: target.sourceDigest, content },
  });
}

export async function deleteDraftDiscussionComment(
  target: DraftDiscussionTarget,
  commentId: string,
): Promise<void> {
  await api.fetch<void>(
    `${draftDiscussionCommentRoute(target, commentId)}?${sourceQuery(target)}`,
    { method: "DELETE" },
  );
}

export async function resolveDraftDiscussionComment(
  target: DraftDiscussionTarget,
  commentId: string,
): Promise<void> {
  await api.fetch<void>(
    `${draftDiscussionCommentRoute(target, commentId)}/resolve?${sourceQuery(target)}`,
    { method: "POST" },
  );
}

export function draftDiscussionScope(target: DraftDiscussionTarget): string {
  return [target.spaceId, target.documentId, target.draftId, target.sourceDigest].join("\u0000");
}

function isUtf16ScalarBoundary(source: string, offset: number): boolean {
  if (!Number.isInteger(offset) || offset < 0 || offset > source.length) return false;
  if (offset === 0 || offset === source.length) return true;
  const previous = source.charCodeAt(offset - 1);
  const current = source.charCodeAt(offset);
  return !(previous >= 0xd800 && previous <= 0xdbff && current >= 0xdc00 && current <= 0xdfff);
}

export function createDraftDiscussionAnchor(
  source: string,
  sourceDigest: string,
  start: number,
  end: number,
): DraftDiscussionAnchor | null {
  if (
    !sourceDigest ||
    start >= end ||
    !isUtf16ScalarBoundary(source, start) ||
    !isUtf16ScalarBoundary(source, end)
  ) {
    return null;
  }
  return {
    source_digest: sourceDigest,
    selection_start: start,
    selection_end: end,
    selection_text: source.slice(start, end),
  };
}

export function isDraftDiscussionAnchorCurrent(
  anchor: DraftDiscussionAnchor,
  source: string,
  sourceDigest: string,
): boolean {
  return (
    anchor.source_digest === sourceDigest &&
    isUtf16ScalarBoundary(source, anchor.selection_start) &&
    isUtf16ScalarBoundary(source, anchor.selection_end) &&
    anchor.selection_start < anchor.selection_end &&
    source.slice(anchor.selection_start, anchor.selection_end) === anchor.selection_text
  );
}
