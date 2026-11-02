import { DialogShell } from "./dialog-shell";
import { Button, RESPONSIVE_TOUCH_ACTION_CLASS } from "./button";
import { AlertTriangle, CircleHelp } from "lucide-react";

interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
  disabled?: boolean;
  helperText?: string;
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
  disabled = false,
  helperText,
  labelledBy,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const descriptionId = `${labelledBy}-description`;

  return (
    <DialogShell
      labelledBy={labelledBy}
      describedBy={descriptionId}
      maxWidth="sm"
      onClose={onCancel}
    >
      <div className="p-4">
        <div className="flex gap-3">
          <span
            className={`flex h-8 w-8 shrink-0 items-center justify-center rounded border ${
              destructive
                ? "border-danger/30 bg-danger/10 text-danger"
                : "border-border bg-surface-raised text-text-muted"
            }`}
            aria-hidden="true"
          >
            {destructive ? <AlertTriangle size={16} /> : <CircleHelp size={16} />}
          </span>
          <div className="min-w-0">
            <h2 id={labelledBy} className="text-text text-base font-semibold [overflow-wrap:anywhere]">
              {title}
            </h2>
            <p
              id={descriptionId}
              className="text-text-muted mt-1 whitespace-pre-line [overflow-wrap:anywhere] text-sm leading-5"
            >
              {message}
            </p>
          </div>
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            className={RESPONSIVE_TOUCH_ACTION_CLASS}
            onClick={onCancel}
          >
            {cancelLabel}
          </Button>
          <Button
            type="button"
            variant={getConfirmDialogConfirmVariant(destructive)}
            size="sm"
            className={RESPONSIVE_TOUCH_ACTION_CLASS}
            disabled={disabled}
            aria-describedby={helperText ? `${labelledBy}-helper` : undefined}
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </div>
        {helperText && (
          <p id={`${labelledBy}-helper`} role="status" className="mt-3 text-xs leading-5 text-text-muted">
            {helperText}
          </p>
        )}
      </div>
    </DialogShell>
  );
}
