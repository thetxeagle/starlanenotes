import { useMemo, useState, type KeyboardEvent } from "react";
import { Check, FileText, Folder } from "lucide-react";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

export type SlashPathPickerIcon = "folder" | "document";

export interface SlashPathPickerOption {
  id: string;
  label: string;
  description: string;
  icon: SlashPathPickerIcon;
}

export function splitSlashPathSegments(value: string): string[] {
  return value
    .split("/")
    .map((segment) => segment.trim())
    .filter(Boolean);
}

export function filterSlashPathOptions<T extends SlashPathPickerOption>(
  options: T[],
  query: string,
): T[] {
  const querySegments = splitSlashPathSegments(query).map((segment) => segment.toLowerCase());
  if (querySegments.length === 0) return options;

  return options.filter((option) => {
    const optionSegments = splitSlashPathSegments(option.label).map((segment) =>
      segment.toLowerCase(),
    );
    let nextOptionIndex = 0;

    for (const querySegment of querySegments) {
      const matchedIndex = optionSegments.findIndex(
        (optionSegment, index) =>
          index >= nextOptionIndex && optionSegment.includes(querySegment),
      );

      if (matchedIndex === -1) return false;
      nextOptionIndex = matchedIndex + 1;
    }

    return true;
  });
}

export function getNextSlashPathActiveIndex(
  currentIndex: number,
  optionCount: number,
  direction: "next" | "previous",
): number {
  if (optionCount <= 0) return -1;
  if (direction === "next") return (currentIndex + 1) % optionCount;
  return (currentIndex - 1 + optionCount) % optionCount;
}

interface SlashPathPickerProps<T extends SlashPathPickerOption> {
  id: string;
  value: string;
  options: T[];
  selectedOptionId?: string;
  placeholder?: string;
  emptyMessage?: string;
  ariaLabel: string;
  className?: string;
  onInputChange: (value: string) => void;
  onSelect: (option: T) => void;
  onEscape?: () => void;
}

export function SlashPathPicker<T extends SlashPathPickerOption>({
  id,
  value,
  options,
  selectedOptionId,
  placeholder,
  emptyMessage = "No matching locations.",
  ariaLabel,
  className,
  onInputChange,
  onSelect,
  onEscape,
}: SlashPathPickerProps<T>) {
  const [activeIndex, setActiveIndex] = useState(0);
  const listboxId = `${id}-listbox`;
  const filteredOptions = useMemo(() => filterSlashPathOptions(options, value), [options, value]);
  const boundedActiveIndex =
    filteredOptions.length === 0 ? -1 : Math.min(activeIndex, filteredOptions.length - 1);

  const selectOption = (option: T) => {
    onInputChange(option.label);
    onSelect(option);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      onEscape?.();
      return;
    }

    if (filteredOptions.length === 0) return;

    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((current) =>
        getNextSlashPathActiveIndex(current, filteredOptions.length, "next"),
      );
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((current) =>
        getNextSlashPathActiveIndex(current, filteredOptions.length, "previous"),
      );
    } else if (event.key === "Enter") {
      event.preventDefault();
      selectOption(filteredOptions[Math.max(0, boundedActiveIndex)]);
    }
  };

  return (
    <div className={cn("space-y-2", className)}>
      <Input
        id={id}
        value={value}
        onChange={(event) => {
          setActiveIndex(0);
          onInputChange(event.target.value);
        }}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        aria-label={ariaLabel}
        aria-autocomplete="list"
        aria-controls={listboxId}
        aria-expanded={filteredOptions.length > 0}
        aria-activedescendant={
          boundedActiveIndex >= 0 ? `${listboxId}-option-${filteredOptions[boundedActiveIndex].id}` : undefined
        }
        role="combobox"
        autoComplete="off"
        className="text-sm"
      />
      <div
        id={listboxId}
        role="listbox"
        aria-label={`${ariaLabel} suggestions`}
        className="border-border bg-surface max-h-[min(18rem,calc(100dvh-14rem))] overflow-y-auto rounded-input border p-1"
      >
        {filteredOptions.length > 0 ? (
          filteredOptions.map((option, index) => {
            const selected = selectedOptionId === option.id;
            const active = index === boundedActiveIndex;

            return (
              <button
                key={option.id}
                id={`${listboxId}-option-${option.id}`}
                type="button"
                role="option"
                aria-selected={selected}
                onMouseEnter={() => setActiveIndex(index)}
                onClick={() => selectOption(option)}
                className={cn(
                  "rounded-button flex min-h-11 w-full items-center gap-2 px-2.5 py-2 text-left text-sm transition-colors duration-100",
                  selected || active
                    ? "bg-surface-raised text-text"
                    : "text-text-muted hover:bg-surface-raised hover:text-text",
                )}
              >
                {option.icon === "folder" ? (
                  <Folder size={14} className="shrink-0" />
                ) : (
                  <FileText size={14} className="shrink-0" />
                )}
                <span className="min-w-0 flex-1">
                  <span className="block truncate">{option.label}</span>
                  <span className="block truncate text-xs text-text-muted">
                    {option.description}
                  </span>
                </span>
                {selected && <Check size={13} className="shrink-0" />}
              </button>
            );
          })
        ) : (
          <p className="text-text-muted px-2 py-2 text-sm">{emptyMessage}</p>
        )}
      </div>
    </div>
  );
}
