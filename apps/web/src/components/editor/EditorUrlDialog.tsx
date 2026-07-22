import { Button } from "@/components/ui/button";
import { DialogShell } from "@/components/ui/dialog-shell";
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
        className="p-3"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <div className="mb-3">
          <h2 id={labelledBy} className="text-sm font-medium text-text">
            {getEditorUrlDialogTitle(kind)}
          </h2>
          <label htmlFor={inputId} className="mt-3 block text-xs font-medium text-text-muted">
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
        </div>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" size="sm" disabled={!canSubmitEditorUrl(value)}>
            Insert
          </Button>
        </div>
      </form>
    </DialogShell>
  );
}
