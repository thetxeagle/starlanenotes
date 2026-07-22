import { Search } from "lucide-react";
import { forwardRef, type InputHTMLAttributes } from "react";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

type SearchInputProps = Omit<InputHTMLAttributes<HTMLInputElement>, "type"> & {
  containerClassName?: string;
};

export const SearchInput = forwardRef<HTMLInputElement, SearchInputProps>(
  ({ className, containerClassName, ...props }, ref) => (
    <label className={cn("relative block", containerClassName)}>
      <Search
        size={15}
        className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-text-muted"
      />
      <Input
        ref={ref}
        type="search"
        className={cn("pl-9", className)}
        {...props}
      />
    </label>
  ),
);

SearchInput.displayName = "SearchInput";
