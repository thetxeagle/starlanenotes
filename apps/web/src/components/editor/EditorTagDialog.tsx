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

export function canSubmitEditorTag(normalizedTag: string): boolean {
  return normalizedTag.length > 0;
}

interface EditorTagDialogProps {
  value: string;
  normalizedTag: string;
  onChange: (value: string) => void;
  onCancel: () => void;
  onSubmit: () => void;
}

export function EditorTagDialog({
  value,
  normalizedTag,
  onChange,
  onCancel,
  onSubmit,
}: EditorTagDialogProps) {
  const canSubmit = canSubmitEditorTag(normalizedTag);

  return (
    <DialogShell
      labelledBy="editor-tag-title"
      describedBy="editor-tag-description"
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
            <DialogTitle id="editor-tag-title">Insert tag</DialogTitle>
            <DialogDescription id="editor-tag-description">
              Keep a portable tag with this page.
            </DialogDescription>
          </div>
        </DialogHeader>
        <DialogBody>
          <label htmlFor="editor-tag-name" className="block text-sm font-medium text-text">
            Tag
          </label>
          <Input
            id="editor-tag-name"
            value={value}
            onChange={(event) => onChange(event.target.value)}
            className="mt-1 h-8 bg-surface text-sm"
            placeholder="project-notes"
            autoFocus
            autoComplete="off"
          />
          <p className="mt-2 text-xs text-text-muted">
            Inserts as <span className="font-medium text-text">#{normalizedTag || "tag"}</span>
          </p>
        </DialogBody>
        <DialogFooter>
          <Button type="button" variant="secondary" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" size="sm" disabled={!canSubmit}>
            Insert tag
          </Button>
        </DialogFooter>
      </form>
    </DialogShell>
  );
}
