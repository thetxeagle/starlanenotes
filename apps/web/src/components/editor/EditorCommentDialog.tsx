import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { Textarea } from "@/components/ui/textarea";

export interface EditorCommentTarget {
  selectionText?: string;
}

export function canSubmitEditorComment(content: string): boolean {
  return content.trim().length > 0;
}

interface EditorCommentDialogProps {
  content: string;
  error?: string;
  saving?: boolean;
  target: EditorCommentTarget;
  onCancel: () => void;
  onChange: (content: string) => void;
  onSubmit: () => void;
}

export function EditorCommentDialog({
  content,
  error,
  saving = false,
  target,
  onCancel,
  onChange,
  onSubmit,
}: EditorCommentDialogProps) {
  const quotedSelection = target.selectionText?.trim();

  return (
    <DialogShell
      labelledBy="editor-comment-title"
      describedBy="editor-comment-description"
      maxWidth="sm"
      onClose={onCancel}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <DialogHeader>
          <div>
            <DialogTitle id="editor-comment-title">Add comment</DialogTitle>
            <DialogDescription id="editor-comment-description">
              {quotedSelection
                ? "Your comment will stay attached to this selected text."
                : "Leave a document comment."}
            </DialogDescription>
          </div>
        </DialogHeader>
        <DialogBody>
          {quotedSelection && (
            <blockquote className="mb-4 max-h-24 overflow-y-auto border-l-2 border-accent pl-3 text-sm italic text-text-muted">
              {quotedSelection}
            </blockquote>
          )}
          <label htmlFor="editor-comment-content" className="block text-sm font-medium text-text">
            Comment
          </label>
          <Textarea
            id="editor-comment-content"
            value={content}
            onChange={(event) => onChange(event.target.value)}
            placeholder="Add context for your collaborators..."
            className="mt-1 min-h-24 resize-y"
            autoFocus
            disabled={saving}
          />
          {error && <p className="mt-2 text-sm text-danger">{error}</p>}
        </DialogBody>
        <DialogFooter>
          <Button type="button" variant="secondary" size="sm" onClick={onCancel} disabled={saving}>
            Cancel
          </Button>
          <Button type="submit" size="sm" disabled={!canSubmitEditorComment(content) || saving}>
            {saving ? "Saving…" : "Add comment"}
          </Button>
        </DialogFooter>
      </form>
    </DialogShell>
  );
}
