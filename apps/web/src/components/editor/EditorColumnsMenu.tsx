import { useRef } from "react";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import { SECTION_LAYOUT_OPTIONS, type SectionLayout } from "./EditorSection";

/** Radix owns positioning, collisions, dismissal, and focus for the gutter menu. */
export function EditorColumnsMenu({
  rect,
  layout,
  canMoveUp,
  canMoveDown,
  onClose,
  onLayout,
  onMove,
  onInsert,
  onDuplicate,
  onDelete,
}: {
  rect: DOMRect;
  layout: string;
  canMoveUp: boolean;
  canMoveDown: boolean;
  onClose: () => void;
  onLayout: (layout: SectionLayout) => void;
  onMove: (direction: "up" | "down") => void;
  onInsert: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
}) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const handleKeyDown = useMenuKeyboardNavigation(menuRef);
  const anchor = useRef({ getBoundingClientRect: () => rect });
  anchor.current.getBoundingClientRect = () => rect;
  return (
    <Popover
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <PopoverAnchor virtualRef={anchor} />
      <PopoverContent
        ref={menuRef}
        onKeyDown={handleKeyDown}
        data-starlane-columns-menu="true"
        role="menu"
        aria-label="Columns block actions"
        className="w-56 p-1"
        sideOffset={8}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          onClose();
        }}
      >
        <div>
          <p className="text-text-muted px-2 py-2 text-xs">Columns block</p>
          {SECTION_LAYOUT_OPTIONS.filter((option) => option.id !== "single").map((option) => (
            <button
              key={option.id}
              type="button"
              role="menuitemradio"
              aria-checked={layout === option.id}
              className="starlane-menu-item block w-full text-left"
              onClick={() => onLayout(option.id)}
            >
              {option.label}
            </button>
          ))}
          <div className="bg-border my-1 h-px" />
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item block w-full text-left"
            disabled={!canMoveUp}
            onClick={() => onMove("up")}
          >
            Move columns up
          </button>
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item block w-full text-left"
            disabled={!canMoveDown}
            onClick={() => onMove("down")}
          >
            Move columns down
          </button>
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item block w-full text-left"
            onClick={onInsert}
          >
            Insert block below
          </button>
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item block w-full text-left"
            onClick={onDuplicate}
          >
            Duplicate columns
          </button>
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item text-danger hover:bg-danger/10 block w-full text-left"
            onClick={onDelete}
          >
            Delete columns
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
