import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
}

export const BUTTON_DANGER_VARIANT_CLASS = "bg-danger text-white hover:bg-danger/90";
export const BUTTON_RESPONSIVE_HEIGHT_CLASS = "min-h-11 sm:min-h-0";
export const RESPONSIVE_TOUCH_ACTION_CLASS = BUTTON_RESPONSIVE_HEIGHT_CLASS;
export const RESPONSIVE_TOUCH_ICON_ACTION_CLASS = "min-h-11 min-w-11 sm:min-h-0 sm:min-w-0";

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "primary", size = "md", disabled, ...props }, ref) => {
    return (
      <button
        ref={ref}
        disabled={disabled}
        className={cn(
          // Base: no pill shapes, max 6px radius, 100-150ms transitions
          "inline-flex items-center justify-center gap-2 font-medium transition-colors duration-100",
          "rounded-button focus-ring disabled:pointer-events-none disabled:opacity-50",
          BUTTON_RESPONSIVE_HEIGHT_CLASS,
          // Sizes
          size === "sm" && "h-7 px-3 text-xs",
          size === "md" && "h-8 px-4 text-sm",
          size === "lg" && "h-9 px-6 text-sm",
          // Variants — solid fills or simple borders, no gradients
          variant === "primary" && "bg-primary text-white hover:opacity-90",
          variant === "secondary" &&
            "border border-border bg-surface text-text hover:bg-surface-raised",
          variant === "ghost" && "text-text-muted hover:bg-surface-raised hover:text-text",
          variant === "danger" && BUTTON_DANGER_VARIANT_CLASS,
          className,
        )}
        {...props}
      />
    );
  },
);

Button.displayName = "Button";
