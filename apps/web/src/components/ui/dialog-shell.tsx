import {
  useCallback,
  useRef,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type ReactNode,
  type RefObject,
} from "react";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "./button";
import { useOverlayFocusLifecycle } from "./useOverlayFocusLifecycle";

interface DialogShellProps {
  children: ReactNode;
  labelledBy: string;
  describedBy?: string;
  maxWidth?: "sm" | "md" | "lg" | "xl" | "settings" | "canvas" | "workspace";
  presentation?: "centered" | "fullscreen" | "drawer-right";
  onClose: () => void;
  /** Stable opener when a transient palette launches this dialog. */
  triggerRef?: RefObject<HTMLElement | SVGElement | null>;
  /** Let an owned nested menu handle Escape without dismissing this dialog. */
  closeOnEscape?: boolean;
  onDialogMount?: (dialog: HTMLDivElement | null) => void;
}

const widthClass = {
  sm: "max-w-sm",
  md: "max-w-md",
  lg: "max-w-lg",
  xl: "max-w-3xl",
  settings: "max-w-6xl",
  canvas: "max-w-[96rem]",
  workspace: "max-w-[min(80rem,92vw)]",
};

export function DialogHeader({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "border-border flex items-start justify-between gap-4 border-b px-4 py-3",
        className,
      )}
      data-dialog-header=""
      {...props}
    />
  );
}

export function DialogTitle({ className, ...props }: HTMLAttributes<HTMLHeadingElement>) {
  return <h2 className={cn("text-text text-base font-semibold", className)} {...props} />;
}

export function DialogDescription({ className, ...props }: HTMLAttributes<HTMLParagraphElement>) {
  return (
    <p className={cn("text-text-muted mt-1 text-sm leading-5", className)} {...props} />
  );
}

export function DialogBody({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div data-dialog-body="" className={cn("p-4", className)} {...props} />;
}

export function DialogFooter({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn("border-border flex justify-end gap-2 border-t px-4 py-3", className)}
      data-dialog-footer=""
      {...props}
    />
  );
}

interface DialogCloseActionProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  label?: string;
}

export function DialogCloseAction({
  label = "Close dialog",
  className,
  type = "button",
  ...props
}: DialogCloseActionProps) {
  return (
    <Button
      type={type}
      variant="ghost"
      size="sm"
      className={cn("h-8 w-8 shrink-0 p-0", className)}
      aria-label={label}
      title={label}
      {...props}
    >
      <X size={16} aria-hidden="true" />
    </Button>
  );
}

export function DialogShell({
  children,
  labelledBy,
  describedBy,
  maxWidth = "md",
  presentation = "centered",
  onClose,
  triggerRef,
  closeOnEscape = true,
  onDialogMount,
}: DialogShellProps) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const closeAndRestoreFocus = useOverlayFocusLifecycle({
    open: true,
    onClose,
    containerRef: dialogRef,
    triggerRef,
    closeOnEscape,
  });
  const handleDialogMount = useCallback(
    (dialog: HTMLDivElement | null) => {
      dialogRef.current = dialog;
      onDialogMount?.(dialog);
    },
    [onDialogMount],
  );

  const fullscreen = presentation === "fullscreen";
  const drawer = presentation === "drawer-right";

  return (
    <div
      className={
        fullscreen || drawer
          ? "fixed inset-0 z-[60] flex bg-surface/0"
          : "fixed inset-0 z-50 flex items-center justify-center p-4"
      }
    >
      {!fullscreen && (
        <button
          type="button"
          aria-hidden="true"
          tabIndex={-1}
          className="fixed inset-0 cursor-default bg-black/40"
          onClick={closeAndRestoreFocus}
        />
      )}
      <div
        ref={handleDialogMount}
        data-dialog-shell=""
        data-dialog-presentation={presentation}
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-describedby={describedBy}
        tabIndex={-1}
        className={
          fullscreen
            ? "relative h-full w-full overflow-hidden bg-surface"
            : drawer
              ? "relative ml-auto flex h-[100dvh] max-h-[100dvh] w-[min(24rem,92vw)] flex-col overflow-hidden overscroll-none border-l border-border bg-surface pb-[env(safe-area-inset-bottom)] pt-[env(safe-area-inset-top)] shadow-modal"
              : `rounded-card border-border bg-surface shadow-modal relative w-full ${widthClass[maxWidth]} max-h-[calc(100dvh-2rem)] overflow-x-hidden overflow-y-auto overscroll-contain border`
        }
      >
        {children}
      </div>
    </div>
  );
}
