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
import { normalizeEditorLinkHref } from "@/components/editor/slashEditorLink";

export function canSubmitEditorLink(href: string): boolean {
  return Boolean(normalizeEditorLinkHref(href));
}

interface EditorLinkDialogProps {
  text: string;
  href: string;
  onTextChange: (text: string) => void;
  onHrefChange: (href: string) => void;
  onCancel: () => void;
  onSubmit: () => void;
}

export function EditorLinkDialog({
  text,
  href,
  onTextChange,
  onHrefChange,
  onCancel,
  onSubmit,
}: EditorLinkDialogProps) {
  const canSubmit = canSubmitEditorLink(href);

  return (
    <DialogShell
      labelledBy="editor-link-title"
      describedBy="editor-link-description"
      maxWidth="sm"
      onClose={onCancel}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (canSubmit) onSubmit();
        }}
      >
        <DialogHeader>
          <div>
            <DialogTitle id="editor-link-title">Edit link</DialogTitle>
            <DialogDescription id="editor-link-description">
              Set readable link text and a destination for this page.
            </DialogDescription>
          </div>
        </DialogHeader>

        <DialogBody>
          <label htmlFor="editor-link-text" className="block text-sm font-medium text-text">
            Display text
          </label>
          <Input
            id="editor-link-text"
            value={text}
            onChange={(event) => onTextChange(event.target.value)}
            className="mt-1 h-8 bg-surface text-sm"
            placeholder="Helpful label"
            autoFocus
          />

          <label htmlFor="editor-link-url" className="mt-4 block text-sm font-medium text-text">
            Link URL
          </label>
          <Input
            id="editor-link-url"
            value={href}
            onChange={(event) => onHrefChange(event.target.value)}
            className="mt-1 h-8 bg-surface text-sm"
            placeholder="https://example.com"
            autoComplete="off"
          />
        </DialogBody>

        <DialogFooter>
          <Button type="button" variant="secondary" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" size="sm" disabled={!canSubmit}>
            Save link
          </Button>
        </DialogFooter>
      </form>
    </DialogShell>
  );
}
