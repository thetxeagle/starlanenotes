import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { Input } from "@/components/ui/input";

export type EditorUrlInsertKind = "link" | "image" | "bookmark";

export function getEditorUrlDialogTitle(kind: EditorUrlInsertKind): string {
  if (kind === "link") return "Insert link";
  if (kind === "image") return "Insert image";
  return "Insert bookmark";
}

export function getEditorUrlDialogLabel(kind: EditorUrlInsertKind): string {
  return kind === "image" ? "Image URL" : "Link URL";
}

export function getEditorUrlDialogPlaceholder(kind: EditorUrlInsertKind): string {
  return kind === "image" ? "https://example.com/image.jpg" : "https://example.com";
}

export function canSubmitEditorUrl(url: string): boolean {
  return url.trim().length > 0;
}

interface EditorUrlDialogProps {
  kind: EditorUrlInsertKind;
  value: string;
  onChange: (value: string) => void;
  onCancel: () => void;
  onSubmit: () => void;
}

export function EditorUrlDialog({
  kind,
  value,
  onChange,
  onCancel,
  onSubmit,
}: EditorUrlDialogProps) {
  const labelledBy = `editor-${kind}-url-title`;
  const inputId = `editor-${kind}-url`;

  return (
    <DialogShell labelledBy={labelledBy} maxWidth="sm" onClose={onCancel}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <DialogHeader>
          <DialogTitle id={labelledBy}>{getEditorUrlDialogTitle(kind)}</DialogTitle>
        </DialogHeader>
        <DialogBody>
          <label htmlFor={inputId} className="block text-sm font-medium text-text">
            {getEditorUrlDialogLabel(kind)}
          </label>
          <Input
            id={inputId}
            value={value}
            onChange={(event) => onChange(event.target.value)}
            placeholder={getEditorUrlDialogPlaceholder(kind)}
            className="mt-1 h-8 bg-surface text-sm"
            autoComplete="off"
          />
        </DialogBody>
        <DialogFooter>
          <Button type="button" variant="secondary" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" size="sm" disabled={!canSubmitEditorUrl(value)}>
            Insert
          </Button>
        </DialogFooter>
      </form>
    </DialogShell>
  );
}
