import { useCallback, useEffect, useRef, useState } from "react";
import { AtSign, Check, MessageSquareText, Pencil, Reply, Send, Trash2, X } from "lucide-react";
import {
  EditorMentionDialog,
  type EditorMentionMember,
} from "@/components/editor/EditorMentionDialog";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Textarea } from "@/components/ui/textarea";
import { isApiError } from "@/lib/api";
import {
  createDraftDiscussionComment,
  deleteDraftDiscussionComment,
  draftDiscussionScope,
  listDraftDiscussion,
  resolveDraftDiscussionComment,
  updateDraftDiscussionComment,
  type DraftDiscussionAnchor,
  type DraftDiscussionComment,
  type DraftDiscussionMentionCandidate,
  type DraftDiscussionTarget,
} from "@/lib/draftDiscussion";

export function draftDiscussionErrorMessage(error: unknown): string {
  if (isApiError(error) && error.status === 409) {
    return "The draft changed. Your text is still here; refresh the draft before trying again.";
  }
  if (isApiError(error) && error.status === 403) {
    return "Your discussion access changed. Your text is still here for you to copy.";
  }
  return "The discussion could not be updated. Your text is still here; try again.";
}

function Avatar({ name }: { name: string }) {
  return (
    <span
      aria-hidden="true"
      className="border-border bg-surface-raised text-text-muted flex size-7 shrink-0 items-center justify-center border text-xs font-medium"
    >
      {name.trim().slice(0, 1).toUpperCase() || "?"}
    </span>
  );
}

interface DraftDiscussionPanelProps {
  target: DraftDiscussionTarget;
  currentUserId: string;
  pendingAnchor?: DraftDiscussionAnchor | null;
  onAnchorConsumed?: () => void;
  onClose?: () => void;
}

export function appendDraftDiscussionMention(
  value: string,
  member: Pick<EditorMentionMember, "display_name" | "user_id">,
): string {
  const displayName = member.display_name
    .replace(/[\[\]()\r\n]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  const portable = `@[${displayName || "Member"}](${member.user_id})`;
  return `${value}${value && !value.endsWith(" ") ? " " : ""}${portable} `;
}

export function DraftDiscussionPanel({
  target,
  currentUserId,
  pendingAnchor = null,
  onAnchorConsumed,
  onClose,
}: DraftDiscussionPanelProps) {
  const scope = draftDiscussionScope(target);
  const draftIdentity = [target.spaceId, target.documentId, target.draftId].join("\u0000");
  const activeDraftIdentityRef = useRef(draftIdentity);
  const activeScopeRef = useRef(scope);
  activeScopeRef.current = scope;
  const requestIdRef = useRef(0);
  const [comments, setComments] = useState<DraftDiscussionComment[]>([]);
  const [mentionCandidates, setMentionCandidates] = useState<DraftDiscussionMentionCandidate[]>([]);
  const [mentionTarget, setMentionTarget] = useState<
    "composer" | `reply:${string}` | `edit:${string}` | null
  >(null);
  const [mentionQuery, setMentionQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [composer, setComposer] = useState("");
  const [reply, setReply] = useState<{ id: string; content: string } | null>(null);
  const [editing, setEditing] = useState<{ id: string; content: string } | null>(null);
  const [deleting, setDeleting] = useState<DraftDiscussionComment | null>(null);
  const [pending, setPending] = useState("");

  const load = useCallback(async () => {
    const requestId = ++requestIdRef.current;
    const requestScope = draftDiscussionScope(target);
    setLoading(true);
    setError("");
    try {
      const discussion = await listDraftDiscussion(target);
      if (requestId === requestIdRef.current && requestScope === activeScopeRef.current) {
        setComments(discussion.comments);
        setMentionCandidates(discussion.mention_candidates);
      }
    } catch (loadError) {
      if (requestId === requestIdRef.current && requestScope === activeScopeRef.current) {
        setError(draftDiscussionErrorMessage(loadError));
      }
    } finally {
      if (requestId === requestIdRef.current && requestScope === activeScopeRef.current) {
        setLoading(false);
      }
    }
  }, [target]);

  useEffect(() => {
    if (activeDraftIdentityRef.current !== draftIdentity) {
      activeDraftIdentityRef.current = draftIdentity;
      setComposer("");
    }
    setComments([]);
    setMentionCandidates([]);
    setMentionTarget(null);
    setMentionQuery("");
    setReply(null);
    setEditing(null);
    setDeleting(null);
    setPending("");
    void load();
    return () => {
      requestIdRef.current += 1;
    };
  }, [draftIdentity, load, scope]);

  const mutate = async (action: string, operation: () => Promise<void>, onSuccess: () => void) => {
    if (pending) return;
    const requestScope = scope;
    setPending(action);
    setError("");
    try {
      await operation();
      if (requestScope !== activeScopeRef.current) return;
      onSuccess();
      await load();
    } catch (mutationError) {
      if (requestScope === activeScopeRef.current) {
        setError(draftDiscussionErrorMessage(mutationError));
      }
    } finally {
      if (requestScope === activeScopeRef.current) setPending("");
    }
  };

  const submitComment = () => {
    const content = composer.trim();
    if (!content) return;
    void mutate(
      "create",
      async () => {
        await createDraftDiscussionComment(target, content, pendingAnchor);
      },
      () => {
        setComposer("");
        if (pendingAnchor) onAnchorConsumed?.();
      },
    );
  };

  const submitReply = (parentId: string) => {
    const content = reply?.id === parentId ? reply.content.trim() : "";
    if (!content) return;
    void mutate(
      `reply:${parentId}`,
      async () => {
        await createDraftDiscussionComment(target, content, null, parentId);
      },
      () => setReply(null),
    );
  };

  const submitEdit = (commentId: string) => {
    const content = editing?.id === commentId ? editing.content.trim() : "";
    if (!content) return;
    void mutate(
      `edit:${commentId}`,
      () => updateDraftDiscussionComment(target, commentId, content),
      () => setEditing(null),
    );
  };

  const openMention = (target: "composer" | `reply:${string}` | `edit:${string}`) => {
    setMentionQuery("");
    setMentionTarget(target);
  };

  const insertMention = (member: EditorMentionMember) => {
    if (mentionTarget === "composer") {
      setComposer((value) => appendDraftDiscussionMention(value, member));
    } else if (mentionTarget?.startsWith("reply:")) {
      const id = mentionTarget.slice("reply:".length);
      setReply((current) =>
        current?.id === id
          ? { ...current, content: appendDraftDiscussionMention(current.content, member) }
          : current,
      );
    } else if (mentionTarget?.startsWith("edit:")) {
      const id = mentionTarget.slice("edit:".length);
      setEditing((current) =>
        current?.id === id
          ? { ...current, content: appendDraftDiscussionMention(current.content, member) }
          : current,
      );
    }
    setMentionTarget(null);
  };

  const commentActions = (comment: DraftDiscussionComment, allowReply: boolean) => {
    const isAuthor = comment.user_id === currentUserId;
    return (
      <div className="mt-2 flex flex-wrap gap-1">
        {allowReply && !comment.resolved ? (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2"
            disabled={Boolean(pending)}
            onClick={() =>
              setReply(reply?.id === comment.id ? null : { id: comment.id, content: "" })
            }
            aria-expanded={reply?.id === comment.id}
          >
            <Reply size={12} /> Reply
          </Button>
        ) : null}
        {allowReply && !comment.resolved ? (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2"
            disabled={Boolean(pending)}
            onClick={() =>
              void mutate(
                `resolve:${comment.id}`,
                () => resolveDraftDiscussionComment(target, comment.id),
                () => undefined,
              )
            }
          >
            <Check size={12} />
            {pending === `resolve:${comment.id}` ? "Resolving..." : "Resolve"}
          </Button>
        ) : null}
        {isAuthor ? (
          <>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2"
              disabled={Boolean(pending)}
              onClick={() => setEditing({ id: comment.id, content: comment.content })}
            >
              <Pencil size={12} /> Edit
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="text-danger h-7 px-2"
              disabled={Boolean(pending)}
              onClick={() => setDeleting(comment)}
            >
              <Trash2 size={12} /> Delete
            </Button>
          </>
        ) : null}
      </div>
    );
  };

  const renderComment = (comment: DraftDiscussionComment, replyComment = false) => (
    <article
      key={comment.id}
      data-draft-comment-id={comment.id}
      className={`${replyComment ? "border-border ml-7 border-l pl-3" : "border-border border-b pb-4"} ${comment.resolved ? "opacity-60" : ""}`}
    >
      {comment.selection_text ? (
        <blockquote className="border-primary text-text-muted mb-2 border-l-2 pl-2 text-xs italic">
          “{comment.selection_text}”
          {comment.anchor_outdated ? (
            <span className="text-attention mt-1 block not-italic">
              Selection is from an earlier draft source.
            </span>
          ) : null}
        </blockquote>
      ) : null}
      <div className="flex items-start gap-2">
        <Avatar name={comment.user_name} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-2">
            <span className="text-text truncate text-sm font-medium">{comment.user_name}</span>
            <time className="text-text-muted shrink-0 text-xs" dateTime={comment.created_at}>
              {new Date(comment.created_at).toLocaleDateString()}
            </time>
          </div>
          {comment.resolved ? (
            <span className="text-success mt-1 inline-flex items-center gap-1 text-xs">
              <Check size={12} /> Resolved
            </span>
          ) : null}
          {editing?.id === comment.id ? (
            <div className="mt-2 grid gap-2">
              <Textarea
                value={editing.content}
                onChange={(event) => setEditing({ id: comment.id, content: event.target.value })}
                aria-label="Edit draft comment"
                disabled={Boolean(pending)}
              />
              <div className="flex justify-end gap-2">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => openMention(`edit:${comment.id}`)}
                  disabled={Boolean(pending)}
                >
                  <AtSign size={12} /> Mention
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => setEditing(null)}
                  disabled={Boolean(pending)}
                >
                  Cancel
                </Button>
                <Button
                  size="sm"
                  onClick={() => submitEdit(comment.id)}
                  disabled={!editing.content.trim() || Boolean(pending)}
                >
                  {pending === `edit:${comment.id}` ? "Saving..." : "Save"}
                </Button>
              </div>
            </div>
          ) : (
            <p className="text-text mt-2 text-sm leading-6 whitespace-pre-wrap">
              {comment.content}
            </p>
          )}
          {editing?.id !== comment.id ? commentActions(comment, !replyComment) : null}
        </div>
      </div>
    </article>
  );

  const unresolvedCount = comments.filter((comment) => !comment.resolved).length;

  return (
    <div className="flex h-full flex-col">
      <div className="border-border flex items-center justify-between gap-2 border-b px-4 py-3">
        <div className="flex min-w-0 items-center gap-2">
          <MessageSquareText className="text-primary" size={15} aria-hidden="true" />
          <span className="text-text text-sm font-medium">Draft discussion</span>
          {unresolvedCount ? (
            <span className="bg-primary/10 text-primary rounded px-1.5 py-0.5 text-[10px] font-medium">
              {unresolvedCount}
            </span>
          ) : null}
        </div>
        {onClose ? (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 px-0"
            onClick={onClose}
            aria-label="Close draft discussion"
          >
            <X size={14} />
          </Button>
        ) : null}
      </div>

      <div className="flex-1 overflow-y-auto p-3">
        {error ? (
          <div
            className="border-attention/30 bg-attention/5 text-text mb-3 border p-3 text-sm"
            role="alert"
          >
            <p>{error}</p>
            <Button
              variant="secondary"
              size="sm"
              className="mt-2"
              onClick={() => void load()}
              disabled={loading}
            >
              Refresh discussion
            </Button>
          </div>
        ) : null}
        {loading && comments.length === 0 ? (
          <p className="text-text-muted py-8 text-center text-sm" role="status" aria-live="polite">
            Loading draft discussion...
          </p>
        ) : comments.length === 0 ? (
          <p className="text-text-muted py-8 text-center text-sm">No draft comments yet.</p>
        ) : (
          <div className="grid gap-4">
            {comments.map((comment) => (
              <div key={comment.id} className="grid gap-3">
                {renderComment(comment)}
                {comment.replies.map((child) => renderComment(child, true))}
                {reply?.id === comment.id ? (
                  <div className="ml-7 grid gap-2">
                    <Textarea
                      value={reply.content}
                      onChange={(event) =>
                        setReply({ id: comment.id, content: event.target.value })
                      }
                      placeholder="Reply to this draft comment..."
                      aria-label="Draft comment reply"
                      disabled={Boolean(pending)}
                    />
                    <div className="flex justify-end gap-2">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => openMention(`reply:${comment.id}`)}
                        disabled={Boolean(pending)}
                      >
                        <AtSign size={12} /> Mention
                      </Button>
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => setReply(null)}
                        disabled={Boolean(pending)}
                      >
                        Cancel
                      </Button>
                      <Button
                        size="sm"
                        onClick={() => submitReply(comment.id)}
                        disabled={!reply.content.trim() || Boolean(pending)}
                      >
                        <Send size={12} />{" "}
                        {pending === `reply:${comment.id}` ? "Sending..." : "Reply"}
                      </Button>
                    </div>
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="border-border border-t p-3">
        {pendingAnchor ? (
          <div className="border-primary text-text-muted mb-2 border-l-2 pl-2 text-xs">
            Commenting on “{pendingAnchor.selection_text}”
          </div>
        ) : null}
        <Textarea
          value={composer}
          onChange={(event) => setComposer(event.target.value)}
          placeholder="Discuss this shared draft..."
          aria-label="New draft comment"
          disabled={Boolean(pending)}
          className="min-h-20 resize-y"
        />
        <div className="mt-2 flex justify-between gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => openMention("composer")}
            disabled={Boolean(pending)}
          >
            <AtSign size={12} /> Mention
          </Button>
          <Button size="sm" onClick={submitComment} disabled={!composer.trim() || Boolean(pending)}>
            <Send size={12} /> {pending === "create" ? "Sending..." : "Comment"}
          </Button>
        </div>
      </div>

      {deleting ? (
        <ConfirmDialog
          title="Delete draft comment?"
          message="This removes your comment from the shared draft discussion. Replies prevent deletion so review context is not silently lost."
          confirmLabel="Delete comment"
          destructive
          disabled={Boolean(pending)}
          labelledBy="delete-draft-comment-title"
          onCancel={() => setDeleting(null)}
          onConfirm={() =>
            void mutate(
              `delete:${deleting.id}`,
              () => deleteDraftDiscussionComment(target, deleting.id),
              () => setDeleting(null),
            )
          }
        />
      ) : null}
      {mentionTarget ? (
        <EditorMentionDialog
          members={mentionCandidates}
          query={mentionQuery}
          loading={false}
          error={false}
          description="Only people who currently participate in this exact shared draft and retain Comment access can be mentioned."
          onQueryChange={setMentionQuery}
          onSelect={insertMention}
          onCancel={() => setMentionTarget(null)}
        />
      ) : null}
    </div>
  );
}
