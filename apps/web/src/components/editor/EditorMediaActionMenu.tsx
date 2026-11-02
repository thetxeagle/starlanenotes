import { useRef, useState, type ComponentType } from "react";
import { Check, MoreHorizontal, type LucideProps } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import { cn } from "@/lib/utils";

export interface EditorMediaAction {
  label: string;
  icon: ComponentType<LucideProps>;
  onSelect: () => void;
  active?: boolean;
  destructive?: boolean;
  disabled?: boolean;
  group?: string;
  /** Transfer focus to the shared dialog, preserving the stable menu trigger. */
  opensDialog?: boolean;
}

interface EditorMediaActionMenuProps {
  label: string;
  actions: EditorMediaAction[];
}

export async function triggerMediaDownload(url: string, name: string): Promise<void> {
  let downloadUrl = url;
  let objectUrl = "";
  if (/^data:/i.test(url)) {
    const blob = await fetch(url).then((response) => response.blob());
    objectUrl = URL.createObjectURL(blob);
    downloadUrl = objectUrl;
  }

  const anchor = document.createElement("a");
  anchor.href = downloadUrl;
  anchor.download = name;
  anchor.rel = "noopener noreferrer";
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  if (objectUrl) window.setTimeout(() => URL.revokeObjectURL(objectUrl), 1_000);
}

export function EditorMediaActionMenu({ label, actions }: EditorMediaActionMenuProps) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const dialogActionRef = useRef(false);
  const handleMenuKeyDown = useMenuKeyboardNavigation(menuRef);

  return (
    <Popover open={open} onOpenChange={(next) => {
      if (next) dialogActionRef.current = false;
      setOpen(next);
    }}>
      <PopoverTrigger asChild>
        <Button
          ref={triggerRef}
          type="button"
          variant="secondary"
          size="sm"
          aria-label={label}
          title={label}
          className="size-11 p-0 sm:size-8"
        >
          <MoreHorizontal size={16} aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        ref={menuRef}
        role="menu"
        aria-label={label}
        align="end"
        className="w-52"
        onKeyDown={handleMenuKeyDown}
        onCloseAutoFocus={(event) => {
          if (dialogActionRef.current) event.preventDefault();
        }}
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          window.requestAnimationFrame(() => {
            menuRef.current?.querySelector<HTMLButtonElement>("[role='menuitem']")?.focus();
          });
        }}
      >
        {actions.map((action, index) => {
          const Icon = action.icon;
          const startsGroup = index > 0 && actions[index - 1]?.group !== action.group;
          return (
            <button
              key={`${action.group || "action"}-${action.label}`}
              type="button"
              role="menuitem"
              disabled={action.disabled}
              onClick={() => {
                dialogActionRef.current = action.opensDialog === true;
                if (dialogActionRef.current) triggerRef.current?.focus();
                setOpen(false);
                action.onSelect();
              }}
              className={cn(
                "rounded-input flex min-h-11 w-full items-center gap-2 px-2 text-left text-sm transition-colors duration-100 sm:min-h-9",
                startsGroup && "border-border mt-1 border-t pt-2",
                action.destructive
                  ? "text-danger hover:bg-danger/10"
                  : "text-text-muted hover:bg-surface hover:text-text",
                action.disabled && "cursor-not-allowed opacity-40",
              )}
            >
              <Icon size={15} aria-hidden="true" />
              <span className="flex-1">{action.label}</span>
              {action.active ? (
                <Check size={15} className="text-primary" aria-hidden="true" />
              ) : null}
            </button>
          );
        })}
      </PopoverContent>
    </Popover>
  );
}
