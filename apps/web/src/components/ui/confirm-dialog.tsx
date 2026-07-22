import { DialogShell } from "./dialog-shell";
import { Button } from "./button";

interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
  labelledBy: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function getConfirmDialogConfirmVariant(destructive: boolean): "danger" | "primary" {
  return destructive ? "danger" : "primary";
}

export function ConfirmDialog({
  title,
  message,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  destructive = false,
  labelledBy,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  return (
    <DialogShell labelledBy={labelledBy} maxWidth="sm" onClose={onCancel}>
      <div className="p-3">
        <div className="mb-3">
          <h2 id={labelledBy} className="text-text text-sm font-medium">
            {title}
          </h2>
          <p className="text-text-muted mt-1 whitespace-pre-line text-xs">{message}</p>
        </div>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button
            type="button"
            variant={getConfirmDialogConfirmVariant(destructive)}
            size="sm"
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </div>
      </div>
    </DialogShell>
  );
}
