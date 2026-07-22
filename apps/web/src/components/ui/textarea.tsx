import { forwardRef, type TextareaHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export const TEXTAREA_CLASS =
  "flex min-h-20 w-full rounded-input border border-border bg-surface px-3 py-2 text-base text-text placeholder:text-text-muted transition-colors duration-100 focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-1 focus:ring-offset-bg disabled:cursor-not-allowed disabled:opacity-50 md:text-sm";

export const Textarea = forwardRef<
  HTMLTextAreaElement,
  TextareaHTMLAttributes<HTMLTextAreaElement>
>(({ className, ...props }, ref) => (
    <textarea ref={ref} className={cn(TEXTAREA_CLASS, className)} {...props} />
));

Textarea.displayName = "Textarea";
