import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { Input } from "@/components/ui/input";

export function canSubmitEditorBookmark(href: string): boolean {
  return href.trim().length > 0;
}

interface EditorBookmarkDialogProps {
  href: string;
  onHrefChange: (href: string) => void;
  onCancel: () => void;
  onSubmit: () => void;
}

export function EditorBookmarkDialog({
  href,
  onHrefChange,
  onCancel,
  onSubmit,
}: EditorBookmarkDialogProps) {
  return (
    <DialogShell
      labelledBy="editor-bookmark-title"
      describedBy="editor-bookmark-description"
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
            <DialogTitle id="editor-bookmark-title">Insert bookmark</DialogTitle>
            <DialogDescription id="editor-bookmark-description">
              Paste a URL and we’ll add it to the page.
            </DialogDescription>
          </div>
        </DialogHeader>
        <DialogBody>
          <label htmlFor="editor-bookmark-url" className="text-text block text-sm font-medium">
            Link URL
          </label>
          <Input
            id="editor-bookmark-url"
            value={href}
            onChange={(event) => onHrefChange(event.target.value)}
            className="bg-surface mt-1 h-8 text-sm"
            placeholder="https://example.com"
            autoFocus
            autoComplete="off"
          />
        </DialogBody>
        <DialogFooter>
          <Button type="button" variant="secondary" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" size="sm" disabled={!canSubmitEditorBookmark(href)}>
            Insert bookmark
          </Button>
        </DialogFooter>
      </form>
    </DialogShell>
  );
}
