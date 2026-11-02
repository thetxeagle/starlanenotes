import {
  forwardRef,
  Children,
  isValidElement,
  type ChangeEvent,
  type SelectHTMLAttributes,
} from "react";
import { cn } from "@/lib/utils";
import { RESPONSIVE_FORM_CONTROL_HEIGHT_CLASS } from "./form-control";
import { MenuSelect, type MenuSelectOption } from "./menu-select";

export const SELECT_CLASS =
  `flex h-9 w-full appearance-none rounded-input border border-border bg-surface py-1 pl-3 !pr-3 text-base text-text shadow-none transition-colors duration-100 hover:border-text-muted focus:border-primary focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-1 focus:ring-offset-bg disabled:cursor-not-allowed disabled:opacity-50 md:text-sm ${RESPONSIVE_FORM_CONTROL_HEIGHT_CLASS}`;

interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  containerClassName?: string;
  onOpenChange?: (open: boolean) => void;
}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(
  (
    {
      className,
      containerClassName,
      children,
      value,
      defaultValue,
      onChange,
      onOpenChange,
      id,
      disabled,
      ...props
    },
    ref,
  ) => {
    const options: MenuSelectOption[] = Children.toArray(children)
      .filter(
        (child): child is React.ReactElement<{ value?: string; children?: React.ReactNode }> =>
          isValidElement(child) && child.type === "option",
      )
      .map((option) => ({
        value: String(option.props.value ?? ""),
        label: String(option.props.children ?? option.props.value ?? ""),
      }));
    const selectedValue = String(value ?? defaultValue ?? options[0]?.value ?? "");
    const ariaLabel = props["aria-label"];
    const ariaLabelledBy = props["aria-labelledby"];
    const emitChange = (nextValue: string) => {
      onChange?.({
        target: { value: nextValue },
        currentTarget: { value: nextValue },
      } as ChangeEvent<HTMLSelectElement>);
    };

    // Keep the legacy Select API while routing rendering, portal behavior, and focus
    // through the shared anchored MenuSelect primitive. The ref is intentionally
    // unused because this is no longer a native HTMLSelectElement.
    void ref;
    return (
      <span
        className={cn("relative flex w-full min-w-0", containerClassName)}
        data-starlane-select-control="true"
      >
        <MenuSelect
          id={id}
          value={selectedValue}
          options={options}
          onChange={emitChange}
          onOpenChange={onOpenChange}
          aria-label={ariaLabel}
          aria-labelledby={ariaLabelledBy}
          disabled={disabled}
          className={cn(SELECT_CLASS, "appearance-none", className)}
        />
      </span>
    );
  },
);

Select.displayName = "Select";
