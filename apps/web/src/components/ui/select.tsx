import { forwardRef, type SelectHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

export const SELECT_CLASS =
  "flex h-9 w-full rounded-input border border-border bg-surface px-3 py-1 text-base text-text transition-colors duration-100 focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-1 focus:ring-offset-bg disabled:cursor-not-allowed disabled:opacity-50 md:text-sm";

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(
  ({ className, ...props }, ref) => (
    <select ref={ref} className={cn(SELECT_CLASS, className)} {...props} />
  ),
);

Select.displayName = "Select";
