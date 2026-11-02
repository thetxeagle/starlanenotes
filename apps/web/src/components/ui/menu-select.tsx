import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import {
  RESPONSIVE_FORM_CONTROL_HEIGHT_CLASS,
  RESPONSIVE_MENU_OPTION_HEIGHT_CLASS,
} from "./form-control";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";

export interface MenuSelectOption {
  value: string;
  label: string;
}

interface MenuSelectProps {
  id?: string;
  value: string;
  options: MenuSelectOption[];
  onChange: (value: string) => void;
  "aria-label"?: string;
  "aria-labelledby"?: string;
  disabled?: boolean;
  className?: string;
  menuClassName?: string;
  onOpenChange?: (open: boolean) => void;
}

/** Starlane's anchored select pattern for settings and other compact controls. */
export function MenuSelect({
  id,
  value,
  options,
  onChange,
  "aria-label": ariaLabel,
  "aria-labelledby": ariaLabelledBy,
  disabled = false,
  className = "",
  menuClassName = "",
  onOpenChange,
}: MenuSelectProps) {
  const selected = options.find((option) => option.value === value) ?? options[0];
  const [open, setOpen] = useState(false);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const onOpenChangeRef = useRef(onOpenChange);
  onOpenChangeRef.current = onOpenChange;
  const menuOpen = open && !disabled && options.length > 0;
  useEffect(() => {
    if (open && (disabled || options.length === 0)) {
      setOpen(false);
      onOpenChangeRef.current?.(false);
    }
  }, [disabled, open, options.length]);
  useEffect(() => () => { onOpenChangeRef.current?.(false); }, []);
  const selectedIndex = Math.max(
    0,
    options.findIndex((option) => option.value === value),
  );

  useEffect(() => {
    if (!menuOpen) return;
    const frame = window.requestAnimationFrame(() => optionRefs.current[selectedIndex]?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, [menuOpen, options.length, selectedIndex]);

  const setMenuOpen = (nextOpen: boolean) => {
    setOpen(nextOpen);
    onOpenChangeRef.current?.(nextOpen);
  };

  return (
    <Popover open={menuOpen} onOpenChange={setMenuOpen}>
      <PopoverTrigger asChild>
        <button
          id={id}
          type="button"
          aria-label={ariaLabel}
          aria-labelledby={ariaLabelledBy}
          aria-haspopup="listbox"
          aria-expanded={menuOpen}
          disabled={disabled || options.length === 0}
          onKeyDown={(event) => {
            if ((event.key === "ArrowDown" || event.key === "ArrowUp") && options.length > 0) {
              event.preventDefault();
              setMenuOpen(true);
            }
          }}
          className={`focus-ring border-border bg-surface text-text hover:border-text-muted rounded-input flex h-9 w-full items-center justify-between gap-2 border px-3 text-left text-sm transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-50 ${RESPONSIVE_FORM_CONTROL_HEIGHT_CLASS} ${className}`}
        >
          <span className="truncate">{selected?.label ?? "Select an option"}</span>
          <ChevronDown aria-hidden="true" className="text-text-muted shrink-0" size={14} />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        sideOffset={4}
        className={`w-[min(20rem,calc(100vw-2rem))] p-1 ${menuClassName}`}
        role="listbox"
        aria-label={ariaLabel}
      >
        {options.map((option, index) => {
          const selectedOption = option.value === value;
          return (
            <button
              key={option.value}
              ref={(element) => {
                optionRefs.current[index] = element;
              }}
              type="button"
              role="option"
              aria-selected={selectedOption}
              className={`focus-ring rounded-button flex w-full items-center justify-between gap-3 px-2.5 text-left text-sm transition-colors duration-100 ${RESPONSIVE_MENU_OPTION_HEIGHT_CLASS} ${selectedOption ? "bg-primary/15 text-primary" : "text-text-muted hover:bg-surface-raised hover:text-text"}`}
              onClick={() => {
                onChange(option.value);
                setMenuOpen(false);
              }}
              onKeyDown={(event) => {
                if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                  event.preventDefault();
                  const direction = event.key === "ArrowDown" ? 1 : -1;
                  const nextIndex = (index + direction + options.length) % options.length;
                  optionRefs.current[nextIndex]?.focus();
                } else if (event.key === "Home" || event.key === "End") {
                  event.preventDefault();
                  optionRefs.current[event.key === "Home" ? 0 : options.length - 1]?.focus();
                }
              }}
            >
              <span className="truncate">{option.label}</span>
              {selectedOption ? <Check aria-hidden="true" size={14} /> : null}
            </button>
          );
        })}
      </PopoverContent>
    </Popover>
  );
}
