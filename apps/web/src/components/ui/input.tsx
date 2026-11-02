import { forwardRef, type InputHTMLAttributes } from "react";
import { cn } from "@/lib/utils";
import { RESPONSIVE_FORM_CONTROL_HEIGHT_CLASS } from "./form-control";

export const INPUT_CLASS = [
  "flex h-9 w-full rounded-input border border-border bg-surface px-3 py-1",
  RESPONSIVE_FORM_CONTROL_HEIGHT_CLASS,
  "text-base text-text placeholder:text-text-muted md:text-sm",
  "transition-colors duration-100",
  "focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-1 focus:ring-offset-bg",
  "disabled:cursor-not-allowed disabled:opacity-50",
].join(" ");

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        ref={ref}
        className={cn(
          // Solid 1px border, 4px radius, simple focus ring — no animated underlines
          INPUT_CLASS,
          className,
        )}
        {...props}
      />
    );
  },
);

Input.displayName = "Input";
