import { useCallback, useEffect, useRef, useState } from "react";
import { Check, Send, Reply, X } from "lucide-react";
import { api } from "@/lib/api";
import {
  emitDocumentCommentsChanged,
  listDocumentComments,
  subscribeToDocumentComments,
  type DocumentComment,
} from "@/lib/documentComments";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface ReactionGroup {
  emoji: string;
  count: number;
  user_reacted: boolean;
}

interface CommentMutationLock {
  scope: string;
  action: string;
}

const QUICK_REACTIONS = ["👍", "❤️", "🎉"];

export function claimCommentMutation(
  lock: CommentMutationLock,
  scope: string,
  action: string,
): boolean {
  if (lock.scope === scope && lock.action) return false;
  lock.scope = scope;
  lock.action = action;
  return true;
}

export function releaseCommentMutation(
  lock: CommentMutationLock,
  scope: string,
  action: string,
): boolean {
  if (lock.scope !== scope || lock.action !== action) return false;
  lock.action = "";
  return true;
}

export function shouldSubmitCommentOnEnter(key: string, isComposing: boolean): boolean {
  return key === "Enter" && !isComposing;
}

export async function settleCurrentCommentsRequest<T>(
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

function CommentAvatar({ name }: { name: string }) {
  const initial = name.trim().slice(0, 1).toUpperCase() || "?";
  return (
    <span
      aria-hidden="true"
      className="border-border bg-surface-raised text-text-muted flex size-7 shrink-0 items-center justify-center border text-xs font-medium"
    >
      {initial}
    </span>
  );
}

interface CommentsPanelProps {
  spaceId: string;
  docId: string;
  selectedCommentId?: string | null;
  inlineOnly?: boolean;
  onClose?: () => void;
}

export function CommentsPanel({
  spaceId,
  docId,
  selectedCommentId,
  inlineOnly = false,
  onClose,
}: CommentsPanelProps) {
  const listRef = useRef<HTMLDivElement | null>(null);
  const [comments, setComments] = useState<DocumentComment[]>([]);
  const [newComment, setNewComment] = useState("");
  const [replyTo, setReplyTo] = useState<string | null>(null);
  const [replyContent, setReplyContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [pendingAction, setPendingAction] = useState<CommentMutationLock | null>(null);
  const [reactions, setReactions] = useState<ReactionGroup[]>([]);
  const [reactionsError, setReactionsError] = useState(false);
  const commentsRequestIdRef = useRef(0);
  const reactionsRequestIdRef = useRef(0);
  const mutationRequestIdRef = useRef(0);
  const documentScope = `${spaceId}\u0000${docId}`;
  const activeDocumentScopeRef = useRef(documentScope);
  activeDocumentScopeRef.current = documentScope;
  const mutationLockRef = useRef<CommentMutationLock>({ scope: documentScope, action: "" });
  const activePendingAction = pendingAction?.scope === documentScope ? pendingAction.action : "";

  const visibleComments = inlineOnly
    ? comments.filter((comment) => Boolean(comment.selection_text))
    : comments;
  const unresolvedCount = visibleComments.filter((c) => !c.resolved).length;

  const loadComments = useCallback(async () => {
    const requestId = ++commentsRequestIdRef.current;
    setLoading(true);
    setError("");
    await settleCurrentCommentsRequest(
      listDocumentComments(spaceId, docId),
      () => requestId === commentsRequestIdRef.current,
      setComments,
      () => setError("Unable to load comments. Check your connection and try again."),
      () => setLoading(false),
    );
  }, [spaceId, docId]);

  const loadReactions = useCallback(async () => {
    const requestScope = `${spaceId}\u0000${docId}`;
    if (activeDocumentScopeRef.current !== requestScope) return;

    const requestId = ++reactionsRequestIdRef.current;
    setReactionsError(false);
    await settleCurrentCommentsRequest(
      api.fetch<ReactionGroup[]>(`/spaces/${spaceId}/documents/${docId}/reactions`),
      () =>
        requestId === reactionsRequestIdRef.current &&
        requestScope === activeDocumentScopeRef.current,
      setReactions,
      () => setReactionsError(true),
      () => undefined,
    );
  }, [docId, spaceId]);

  useEffect(() => {
    loadComments();
    loadReactions();
    return () => {
      reactionsRequestIdRef.current += 1;
      mutationRequestIdRef.current += 1;
    };
  }, [loadComments, loadReactions]);

  useEffect(
    () => subscribeToDocumentComments(spaceId, docId, () => void loadComments()),
    [docId, loadComments, spaceId],
  );

  useEffect(() => {
    if (!selectedCommentId) return;
    const comment = listRef.current?.querySelector<HTMLElement>(
      `[data-document-comment-id="${selectedCommentId}"]`,
    );
    comment?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [comments, inlineOnly, selectedCommentId]);

  const toggleReaction = async (emoji: string) => {
    const action = `reaction:${emoji}`;
    if (!claimCommentMutation(mutationLockRef.current, documentScope, action)) return;
    const requestId = ++mutationRequestIdRef.current;
    const requestScope = documentScope;
    const isCurrent = () =>
      requestId === mutationRequestIdRef.current &&
      requestScope === activeDocumentScopeRef.current;
    setPendingAction({ scope: documentScope, action });
    await settleCurrentCommentsRequest(
      api.fetch(`/spaces/${spaceId}/documents/${docId}/reactions`, {
        method: "POST",
        body: { emoji },
      }),
      isCurrent,
      async () => {
        await loadReactions();
      },
      () => setReactionsError(true),
      () => {
        if (releaseCommentMutation(mutationLockRef.current, requestScope, action)) {
          setPendingAction(null);
        }
      },
    );
  };

  const addComment = async () => {
    if (!newComment.trim()) return;
    const action = "comment";
    if (!claimCommentMutation(mutationLockRef.current, documentScope, action)) return;
    const requestId = ++mutationRequestIdRef.current;
    const requestScope = documentScope;
    const isCurrent = () =>
      requestId === mutationRequestIdRef.current &&
      requestScope === activeDocumentScopeRef.current;
    setPendingAction({ scope: documentScope, action });
    setError("");
    await settleCurrentCommentsRequest(
      api.fetch(`/spaces/${spaceId}/documents/${docId}/comments`, {
        method: "POST",
        body: { content: newComment },
      }),
      isCurrent,
      async () => {
        setNewComment("");
        await loadComments();
        if (isCurrent()) emitDocumentCommentsChanged(spaceId, docId);
      },
      () => setError("Unable to add your comment. Please try again."),
      () => {
        if (releaseCommentMutation(mutationLockRef.current, requestScope, action)) {
          setPendingAction(null);
        }
      },
    );
  };

  const addReply = async (parentId: string) => {
    if (!replyContent.trim()) return;
    const action = `reply:${parentId}`;
    if (!claimCommentMutation(mutationLockRef.current, documentScope, action)) return;
    const requestId = ++mutationRequestIdRef.current;
    const requestScope = documentScope;
    const isCurrent = () =>
      requestId === mutationRequestIdRef.current &&
      requestScope === activeDocumentScopeRef.current;
    setPendingAction({ scope: documentScope, action });
    setError("");
    await settleCurrentCommentsRequest(
      api.fetch(`/spaces/${spaceId}/documents/${docId}/comments`, {
        method: "POST",
        body: { content: replyContent, parent_id: parentId },
      }),
      isCurrent,
      async () => {
        setReplyTo(null);
        setReplyContent("");
        await loadComments();
        if (isCurrent()) emitDocumentCommentsChanged(spaceId, docId);
      },
      () => setError("Unable to add your reply. Please try again."),
      () => {
        if (releaseCommentMutation(mutationLockRef.current, requestScope, action)) {
          setPendingAction(null);
        }
      },
    );
  };

  const resolveComment = async (commentId: string) => {
    const action = `resolve:${commentId}`;
    if (!claimCommentMutation(mutationLockRef.current, documentScope, action)) return;
    const requestId = ++mutationRequestIdRef.current;
    const requestScope = documentScope;
    const isCurrent = () =>
      requestId === mutationRequestIdRef.current &&
      requestScope === activeDocumentScopeRef.current;
    setPendingAction({ scope: documentScope, action });
    setError("");
    await settleCurrentCommentsRequest(
      api.fetch(`/spaces/${spaceId}/documents/${docId}/comments/${commentId}/resolve`, {
        method: "POST",
      }),
      isCurrent,
      async () => {
        await loadComments();
        if (isCurrent()) emitDocumentCommentsChanged(spaceId, docId);
      },
      () => setError("Unable to resolve this comment. Please try again."),
      () => {
        if (releaseCommentMutation(mutationLockRef.current, requestScope, action)) {
          setPendingAction(null);
        }
      },
    );
  };

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="border-border flex items-center justify-between gap-2 border-b px-4 py-3">
        <div className="flex min-w-0 items-center gap-2">
          <span className="text-text text-sm font-medium">Comments</span>
          {unresolvedCount > 0 && (
            <span className="bg-accent/10 text-accent rounded-full px-1.5 py-0.5 text-[10px] font-medium">
              {unresolvedCount}
            </span>
          )}
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="focus-ring rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 shrink-0 items-center justify-center transition-colors duration-100"
            aria-label="Close comments panel"
            title="Close panel"
          >
            <X size={14} />
          </button>
        )}
      </div>

      {/* Comment list */}
      <div ref={listRef} className="flex-1 overflow-y-auto p-3">
        {loading ? (
          <div
            className="text-text-muted py-8 text-center text-sm"
            role="status"
            aria-live="polite"
          >
            Loading comments...
          </div>
        ) : error ? (
          <div className="text-text-muted grid gap-3 py-8 text-center text-sm" role="alert">
            <p>{error}</p>
            <Button
              variant="secondary"
              size="sm"
              className="justify-self-center"
              onClick={() => void loadComments()}
            >
              Retry
            </Button>
          </div>
        ) : visibleComments.length === 0 ? (
          <div className="text-text-muted py-8 text-center text-sm">
            {inlineOnly ? "No inline comments yet" : "No comments yet"}
          </div>
        ) : (
          visibleComments.map((comment) => (
            <div
              key={comment.id}
              data-document-comment-id={comment.id}
              className={`border-border border-b py-4 last:border-b-0 ${
                selectedCommentId === comment.id ? "bg-primary/5 -mx-2 rounded-input px-2" : ""
              } ${
                comment.resolved ? "opacity-60" : ""
              }`}
              aria-current={selectedCommentId === comment.id ? "true" : undefined}
            >
              {/* Selection context */}
              {comment.selection_text && (
                <div className="border-accent text-text-muted mb-2 border-l-2 pl-2 text-xs italic">
                  "{comment.selection_text}"
                </div>
              )}

              {/* Comment header */}
              <div className="flex items-start gap-2">
                <CommentAvatar name={comment.user_name} />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-text truncate text-sm font-medium">
                      {comment.user_name}
                    </span>
                    <span className="text-text-muted shrink-0 text-xs">
                      {new Date(comment.created_at).toLocaleDateString()}
                    </span>
                  </div>
                  {comment.resolved ? (
                    <span className="text-success mt-1 inline-flex items-center gap-1 text-xs">
                      <Check size={12} /> Resolved
                    </span>
                  ) : null}
                </div>
              </div>

              {/* Content */}
              <p className="text-text ml-9 mt-2 text-sm leading-6">{comment.content}</p>

              {/* Actions */}
              <div className="ml-9 mt-2 flex items-center gap-1">
                {!comment.resolved && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => resolveComment(comment.id)}
                    disabled={Boolean(activePendingAction)}
                    className="h-auto gap-1 px-0 py-0 text-[10px]"
                  >
                    <Check size={12} />{" "}
                    {activePendingAction === `resolve:${comment.id}` ? "Resolving..." : "Resolve"}
                  </Button>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setReplyTo(replyTo === comment.id ? null : comment.id)}
                  disabled={Boolean(activePendingAction)}
                  className="h-auto gap-1 px-2 py-1 text-xs"
                  aria-expanded={replyTo === comment.id}
                  aria-controls={
                    replyTo === comment.id ? `comment-reply-composer-${comment.id}` : undefined
                  }
                >
                  <Reply size={12} /> Reply
                </Button>
              </div>

              {/* Replies */}
              {comment.replies.map((reply) => (
                <div key={reply.id} className="border-border ml-9 mt-3 border-l pl-3">
                  <div className="flex items-start gap-2">
                    <CommentAvatar name={reply.user_name} />
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-text text-xs font-medium">{reply.user_name}</span>
                        <span className="text-text-muted text-xs">
                          {new Date(reply.created_at).toLocaleDateString()}
                        </span>
                      </div>
                      <p className="text-text mt-1 text-sm leading-6">{reply.content}</p>
                    </div>
                  </div>
                </div>
              ))}

              {/* Reply input */}
              {replyTo === comment.id && (
                <div id={`comment-reply-composer-${comment.id}`} className="ml-4 mt-2 flex gap-1">
                  <Input
                    type="text"
                    value={replyContent}
                    onChange={(e) => setReplyContent(e.target.value)}
                    onKeyDown={(event) => {
                      if (!shouldSubmitCommentOnEnter(event.key, event.nativeEvent.isComposing)) {
                        return;
                      }
                      event.preventDefault();
                      void addReply(comment.id);
                    }}
                    placeholder="Reply..."
                    aria-label="Reply to comment"
                    className="bg-bg h-6 flex-1 px-2 py-1 text-xs"
                    autoFocus
                    disabled={Boolean(activePendingAction)}
                  />
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => addReply(comment.id)}
                    disabled={!replyContent.trim() || Boolean(activePendingAction)}
                    className="text-primary h-6 w-6 px-0"
                    aria-label="Send reply"
                  >
                    <Send size={12} />
                  </Button>
                </div>
              )}
            </div>
          ))
        )}
      </div>

      {/* New comment input */}
      <div className="border-border border-t p-3">
        <div
          className="mb-3 flex flex-wrap items-center gap-1"
          role="group"
          aria-label="Document reactions"
        >
          {reactionsError ? (
            <div className="flex items-center gap-2 text-xs" role="alert">
              <span className="text-text-muted">Unable to load reactions.</span>
              <Button
                variant="ghost"
                size="sm"
                className="h-6 px-1.5 text-xs"
                onClick={() => void loadReactions()}
              >
                Retry
              </Button>
            </div>
          ) : (
            QUICK_REACTIONS.map((emoji) => {
              const reaction = reactions.find((item) => item.emoji === emoji);
              const active = reaction?.user_reacted ?? false;
              return (
                <Button
                  key={emoji}
                  variant="ghost"
                  size="sm"
                  className={`h-6 gap-1 px-1.5 text-xs ${active ? "bg-primary/10 text-primary" : ""}`}
                  onClick={() => void toggleReaction(emoji)}
                  disabled={Boolean(activePendingAction)}
                  aria-label={`React with ${emoji}`}
                  aria-pressed={active}
                >
                  {emoji}
                  {reaction ? ` ${reaction.count}` : ""}
                </Button>
              );
            })
          )}
        </div>
        <div className="flex gap-1">
          <Input
            type="text"
            value={newComment}
            onChange={(e) => setNewComment(e.target.value)}
            onKeyDown={(event) => {
              if (!shouldSubmitCommentOnEnter(event.key, event.nativeEvent.isComposing)) return;
              event.preventDefault();
              void addComment();
            }}
            placeholder="Add a comment..."
            aria-label="New comment"
            className="bg-bg h-8 flex-1 px-3 py-1.5 text-sm"
            disabled={Boolean(activePendingAction)}
          />
          <Button
            size="sm"
            onClick={addComment}
            disabled={!newComment.trim() || Boolean(activePendingAction)}
            aria-label="Send comment"
          >
            <Send size={12} />
          </Button>
        </div>
      </div>
    </div>
  );
}
