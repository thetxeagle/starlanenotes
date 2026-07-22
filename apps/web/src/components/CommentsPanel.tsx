import { useCallback, useEffect, useState } from "react";
import { MessageSquare, Check, Send } from "lucide-react";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface Comment {
  id: string;
  document_id: string;
  parent_id?: string;
  user_id: string;
  user_name: string;
  content: string;
  selection_text?: string;
  resolved: boolean;
  created_at: string;
  replies: Comment[];
}

interface ReactionGroup {
  emoji: string;
  count: number;
  user_reacted: boolean;
}

const QUICK_REACTIONS = ["👍", "❤️", "🎉"];

interface CommentsPanelProps {
  spaceId: string;
  docId: string;
}

export function CommentsPanel({ spaceId, docId }: CommentsPanelProps) {
  const [comments, setComments] = useState<Comment[]>([]);
  const [newComment, setNewComment] = useState("");
  const [replyTo, setReplyTo] = useState<string | null>(null);
  const [replyContent, setReplyContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [pendingAction, setPendingAction] = useState("");
  const [reactions, setReactions] = useState<ReactionGroup[]>([]);
  const [reactionsError, setReactionsError] = useState(false);

  const unresolvedCount = comments.filter((c) => !c.resolved).length;

  const loadComments = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const data = await api.fetch<Comment[]>(
        `/spaces/${spaceId}/documents/${docId}/comments`,
      );
      setComments(data);
    } catch {
      setError("Unable to load comments. Check your connection and try again.");
    } finally {
      setLoading(false);
    }
  }, [spaceId, docId]);

  const loadReactions = useCallback(async () => {
    setReactionsError(false);
    try {
      const data = await api.fetch<ReactionGroup[]>(
        `/spaces/${spaceId}/documents/${docId}/reactions`,
      );
      setReactions(data);
    } catch {
      setReactionsError(true);
    }
  }, [spaceId, docId]);

  useEffect(() => {
    loadComments();
    loadReactions();
  }, [loadComments, loadReactions]);

  const toggleReaction = async (emoji: string) => {
    setPendingAction(`reaction:${emoji}`);
    try {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/reactions`, {
        method: "POST",
        body: { emoji },
      });
      await loadReactions();
    } catch {
      setReactionsError(true);
    } finally {
      setPendingAction("");
    }
  };

  const addComment = async () => {
    if (!newComment.trim()) return;
    setPendingAction("comment");
    setError("");
    try {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/comments`, {
        method: "POST",
        body: { content: newComment },
      });
      setNewComment("");
      await loadComments();
    } catch {
      setError("Unable to add your comment. Please try again.");
    } finally {
      setPendingAction("");
    }
  };

  const addReply = async (parentId: string) => {
    if (!replyContent.trim()) return;
    setPendingAction(`reply:${parentId}`);
    setError("");
    try {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/comments`, {
        method: "POST",
        body: { content: replyContent, parent_id: parentId },
      });
      setReplyTo(null);
      setReplyContent("");
      await loadComments();
    } catch {
      setError("Unable to add your reply. Please try again.");
    } finally {
      setPendingAction("");
    }
  };

  const resolveComment = async (commentId: string) => {
    setPendingAction(`resolve:${commentId}`);
    setError("");
    try {
      await api.fetch(
        `/spaces/${spaceId}/documents/${docId}/comments/${commentId}/resolve`,
        { method: "POST" },
      );
      await loadComments();
    } catch {
      setError("Unable to resolve this comment. Please try again.");
    } finally {
      setPendingAction("");
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-border px-4 py-2">
        <MessageSquare size={14} className="text-text-muted" />
        <span className="text-sm font-medium text-text">Comments</span>
        {unresolvedCount > 0 && (
          <span className="rounded-full bg-accent/10 px-1.5 py-0.5 text-[10px] font-medium text-accent">
            {unresolvedCount}
          </span>
        )}
      </div>

      {/* Comment list */}
      <div className="flex-1 overflow-y-auto p-3">
        {loading ? (
          <div className="py-8 text-center text-sm text-text-muted">Loading comments...</div>
        ) : error ? (
          <div className="grid gap-3 py-8 text-center text-sm text-text-muted">
            <p>{error}</p>
            <Button variant="secondary" size="sm" className="justify-self-center" onClick={() => void loadComments()}>
              Retry
            </Button>
          </div>
        ) : comments.length === 0 ? (
          <div className="py-8 text-center text-sm text-text-muted">No comments yet</div>
        ) : (
          comments.map((comment) => (
            <div
              key={comment.id}
              className={`mb-3 rounded-card border border-border p-3 ${
                comment.resolved ? "opacity-60" : ""
              }`}
            >
              {/* Selection context */}
              {comment.selection_text && (
                <div className="mb-2 border-l-2 border-accent pl-2 text-xs italic text-text-muted">
                  "{comment.selection_text}"
                </div>
              )}

              {/* Comment header */}
              <div className="flex items-center justify-between">
                <span className="text-xs font-medium text-text">{comment.user_name}</span>
                <span className="text-[10px] text-text-muted">
                  {new Date(comment.created_at).toLocaleDateString()}
                </span>
              </div>

              {/* Content */}
              <p className="mt-1 text-sm text-text">{comment.content}</p>

              {/* Actions */}
              <div className="mt-2 flex items-center gap-2">
                {!comment.resolved && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => resolveComment(comment.id)}
                    disabled={Boolean(pendingAction)}
                    className="h-auto gap-1 px-0 py-0 text-[10px]"
                  >
                    <Check size={10} /> {pendingAction === `resolve:${comment.id}` ? "Resolving..." : "Resolve"}
                  </Button>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setReplyTo(replyTo === comment.id ? null : comment.id)}
                  disabled={Boolean(pendingAction)}
                  className="h-auto px-0 py-0 text-[10px]"
                >
                  Reply
                </Button>
              </div>

              {/* Replies */}
              {comment.replies.map((reply) => (
                <div key={reply.id} className="ml-4 mt-2 border-l border-border pl-3">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-medium text-text">{reply.user_name}</span>
                    <span className="text-[10px] text-text-muted">
                      {new Date(reply.created_at).toLocaleDateString()}
                    </span>
                  </div>
                  <p className="mt-0.5 text-sm text-text">{reply.content}</p>
                </div>
              ))}

              {/* Reply input */}
              {replyTo === comment.id && (
                <div className="ml-4 mt-2 flex gap-1">
                  <Input
                    type="text"
                    value={replyContent}
                    onChange={(e) => setReplyContent(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && addReply(comment.id)}
                    placeholder="Reply..."
                    aria-label="Reply to comment"
                    className="h-6 flex-1 bg-bg px-2 py-1 text-xs"
                    autoFocus
                    disabled={Boolean(pendingAction)}
                  />
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => addReply(comment.id)}
                    disabled={!replyContent.trim() || Boolean(pendingAction)}
                    className="h-6 w-6 px-0 text-primary"
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
      <div className="border-t border-border p-3">
        <div className="mb-3 flex flex-wrap items-center gap-1" aria-label="Document reactions">
          {reactionsError ? (
            <Button variant="ghost" size="sm" className="h-6 px-1.5 text-xs" onClick={() => void loadReactions()}>
              Retry reactions
            </Button>
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
                  disabled={Boolean(pendingAction)}
                  aria-label={`React with ${emoji}`}
                >
                  {emoji}{reaction ? ` ${reaction.count}` : ""}
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
            onKeyDown={(e) => e.key === "Enter" && addComment()}
            placeholder="Add a comment..."
            aria-label="New comment"
            className="h-8 flex-1 bg-bg px-3 py-1.5 text-sm"
            disabled={Boolean(pendingAction)}
          />
          <Button size="sm" onClick={addComment} disabled={!newComment.trim() || Boolean(pendingAction)}>
            <Send size={12} />
          </Button>
        </div>
      </div>
    </div>
  );
}
