import type { ReactNode } from "react";

interface DialogShellProps {
  children: ReactNode;
  labelledBy: string;
  maxWidth?: "sm" | "md" | "lg" | "xl";
  onClose: () => void;
}

const widthClass = {
  sm: "max-w-sm",
  md: "max-w-md",
  lg: "max-w-lg",
  xl: "max-w-3xl",
};

export function DialogShell({ children, labelledBy, maxWidth = "md", onClose }: DialogShellProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <button
        type="button"
        aria-label="Close dialog"
        className="fixed inset-0 cursor-default bg-black/40"
        onClick={onClose}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        className={`rounded-card border-border bg-surface shadow-modal relative w-full ${widthClass[maxWidth]} max-h-[calc(100dvh-2rem)] overflow-hidden border`}
      >
        {children}
      </div>
    </div>
  );
}
