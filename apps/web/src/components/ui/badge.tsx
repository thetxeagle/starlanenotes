import { forwardRef, type HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: "secondary" | "outline" | "success" | "warning" | "danger";
}

export const Badge = forwardRef<HTMLSpanElement, BadgeProps>(
  ({ className, variant = "secondary", ...props }, ref) => (
    <span
      ref={ref}
      className={cn(
        "rounded-input inline-flex min-h-5 items-center border px-1.5 py-0.5 font-mono text-[11px] leading-none",
        variant === "secondary" && "border-transparent bg-surface-raised text-text",
        variant === "outline" && "border-border bg-surface text-text-muted",
        variant === "success" && "border-success/30 bg-success/10 text-success",
        variant === "warning" && "border-warning/30 bg-warning/10 text-warning",
        variant === "danger" && "border-danger/30 bg-danger/10 text-danger",
        className,
      )}
      {...props}
    />
  ),
);

Badge.displayName = "Badge";
