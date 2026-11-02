import { useEffect, useRef, useState } from "react";
import { Check, MessageSquareText } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogCloseAction,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { LoadingState } from "@/components/ui/loading-state";
import { isApiError } from "@/lib/api";
import {
  getClosedDraftDiscussion,
  type ClosedDraftDiscussionSummary,
  type DraftDiscussionComment,
  type DraftDiscussionResponse,
} from "@/lib/draftDiscussion";

export function ClosedDraftDiscussionHistory({
  records,
  loading,
  error,
  onRetry,
  onOpen,
}: {
  records: ClosedDraftDiscussionSummary[];
  loading: boolean;
  error: boolean;
  onRetry(): void;
  onOpen(record: ClosedDraftDiscussionSummary): void;
}) {
  return (
    <section aria-label="Retained draft discussions" className="grid gap-2 text-sm">
      <div>
        <p className="text-text text-xs font-medium uppercase">Retained draft discussions</p>
        <p className="text-text-muted mt-1 text-xs">
          Read-only owner records closed by publication. They cannot be edited or reopened.
        </p>
      </div>
      {loading ? (
        <p role="status" className="text-text-muted">Loading retained discussions…</p>
      ) : error ? (
        <div role="alert" className="text-text-muted grid gap-2">
          <p>Retained draft discussions are unavailable.</p>
          <Button variant="secondary" size="sm" className="justify-self-start" onClick={onRetry}>
            Retry retained discussions
          </Button>
        </div>
      ) : records.length === 0 ? (
        <p className="text-text-muted">No retained draft discussions for this page.</p>
      ) : (
        <div className="grid gap-2">
          {records.map((record) => (
            <div key={record.id} className="rounded-input border-border grid gap-2 border p-2">
              <div className="flex items-start justify-between gap-2">
                <div>
                  <p className="text-text font-medium">
                    {record.comment_count} {record.comment_count === 1 ? "comment" : "comments"}
                  </p>
                  <p className="text-text-muted text-xs">
                    Closed {new Date(record.closed_at).toLocaleString()}
                  </p>
                </div>
                {record.unresolved_count > 0 ? (
                  <span className="text-warning text-xs">
                    {record.unresolved_count} unresolved
                  </span>
                ) : (
                  <span className="text-success inline-flex items-center gap-1 text-xs">
                    <Check size={12} aria-hidden="true" /> Resolved
                  </span>
                )}
              </div>
              <Button variant="secondary" size="sm" className="justify-self-start" onClick={() => onOpen(record)}>
                Inspect discussion
              </Button>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function readError(error: unknown): string {
  if (isApiError(error) && error.status === 403) {
    return "This retained discussion is no longer available with your current owner and Comment access.";
  }
  return "The retained discussion could not be loaded. Close this review and try again.";
}

function ReadOnlyComment({ comment, reply = false }: { comment: DraftDiscussionComment; reply?: boolean }) {
  return (
    <article className={reply ? "border-border ml-7 border-l pl-3" : "border-border border-b pb-4"}>
      {comment.selection_text ? (
        <blockquote className="border-primary text-text-muted mb-2 border-l-2 pl-2 text-xs italic">
          “{comment.selection_text}”
          <span className="text-warning mt-1 block not-italic">Selection retained from the published draft.</span>
        </blockquote>
      ) : null}
      <div className="flex items-start gap-2">
        <span aria-hidden="true" className="border-border bg-surface-raised text-text-muted flex size-7 shrink-0 items-center justify-center border text-xs font-medium">
          {comment.user_name.trim().slice(0, 1).toUpperCase() || "?"}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-2">
            <span className="text-text truncate text-sm font-medium">{comment.user_name}</span>
            <time className="text-text-muted shrink-0 text-xs" dateTime={comment.created_at}>
              {new Date(comment.created_at).toLocaleDateString()}
            </time>
          </div>
          {comment.resolved ? (
            <span className="text-success mt-1 inline-flex items-center gap-1 text-xs">
              <Check size={12} aria-hidden="true" /> Resolved
            </span>
          ) : (
            <span className="text-warning mt-1 inline-flex text-xs">Unresolved at publication</span>
          )}
          <p className="text-text mt-2 text-sm leading-6 whitespace-pre-wrap">{comment.content}</p>
        </div>
      </div>
    </article>
  );
}

export function ClosedDraftDiscussionDialog({
  spaceId,
  documentId,
  record,
  onClose,
}: {
  spaceId: string;
  documentId: string;
  record: ClosedDraftDiscussionSummary;
  onClose(): void;
}) {
  const requestId = useRef(0);
  const [discussion, setDiscussion] = useState<DraftDiscussionResponse | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    const current = ++requestId.current;
    setDiscussion(null);
    setError("");
    void getClosedDraftDiscussion(spaceId, documentId, record.id)
      .then((response) => {
        if (requestId.current === current) setDiscussion(response);
      })
      .catch((failure: unknown) => {
        if (requestId.current === current) setError(readError(failure));
      });
    return () => {
      requestId.current += 1;
    };
  }, [documentId, record.id, spaceId]);

  return (
    <DialogShell labelledBy="closed-draft-discussion-title" maxWidth="lg" onClose={onClose}>
      <DialogHeader>
        <DialogTitle id="closed-draft-discussion-title">Retained draft discussion</DialogTitle>
        <DialogCloseAction onClick={onClose} />
      </DialogHeader>
      <DialogBody className="min-h-0 space-y-4 overflow-y-auto">
        <div className="border-border bg-surface-raised flex items-start gap-2 border p-3">
          <MessageSquareText className="text-primary mt-0.5 shrink-0" size={16} aria-hidden="true" />
          <p className="text-text-muted text-sm">
            Read-only custody from publication. Reviewing it does not reopen the draft, notify participants, or change the published page.
          </p>
        </div>
        <dl className="border-border grid gap-2 border p-3 text-xs sm:grid-cols-2">
          <div>
            <dt className="text-text-muted">Closed</dt>
            <dd className="text-text mt-1">{new Date(record.closed_at).toLocaleString()}</dd>
          </div>
          <div>
            <dt className="text-text-muted">Publication receipt</dt>
            <dd className="text-text mt-1 break-all font-mono">{record.publication_operation_id}</dd>
          </div>
        </dl>
        {!discussion && !error ? <LoadingState label="Loading retained draft discussion" /> : null}
        {error ? <p role="alert" className="text-text-muted text-sm">{error}</p> : null}
        {discussion && discussion.comments.length === 0 ? (
          <p className="text-text-muted text-sm">This retained discussion has no comments.</p>
        ) : null}
        {discussion ? (
          <div className="grid gap-4">
            {discussion.comments.map((comment) => (
              <div key={comment.id} className="grid gap-3">
                <ReadOnlyComment comment={comment} />
                {comment.replies.map((reply) => <ReadOnlyComment key={reply.id} comment={reply} reply />)}
              </div>
            ))}
          </div>
        ) : null}
      </DialogBody>
      <DialogFooter>
        <Button variant="secondary" onClick={onClose}>Close</Button>
      </DialogFooter>
    </DialogShell>
  );
}
