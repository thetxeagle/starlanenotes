import { useEffect } from "react";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DialogShell } from "@/components/ui/dialog-shell";
import { isWorkspaceTextEntryTarget } from "@/components/layout/workspaceShortcuts";

const isMac = typeof navigator !== "undefined" && navigator.platform.includes("Mac");
const mod = isMac ? "\u2318" : "Ctrl";

interface KeyboardShortcutSection {
  category: string;
  items: { keys: string; description: string; documentPanelOnly?: boolean }[];
}

const shortcutSections: KeyboardShortcutSection[] = [
  {
    category: "General",
    items: [
      { keys: `${mod}+K`, description: "Search" },
      { keys: "?", description: "Keyboard shortcuts" },
      { keys: `${mod}+\\`, description: "Toggle sidebar" },
      {
        keys: `${mod}+Shift+P`,
        description: "Toggle page side panel",
        documentPanelOnly: true,
      },
      { keys: `${mod}+Shift+D`, description: "Toggle dark mode" },
    ],
  },
  {
    category: "Editor",
    items: [
      { keys: `${mod}+B`, description: "Bold" },
      { keys: `${mod}+I`, description: "Italic" },
      { keys: `${mod}+E`, description: "Inline code" },
      { keys: "Ctrl+/", description: "Command menu" },
      { keys: "Tab", description: "Indent" },
    ],
  },
  {
    category: "Navigation",
    items: [
      { keys: `${mod}+N`, description: "New page" },
      { keys: `${mod}+Shift+N`, description: "New space" },
    ],
  },
];

export function getKeyboardShortcutSections(
  canUseDocumentPanelShortcut = true,
): KeyboardShortcutSection[] {
  return shortcutSections
    .map((section) => ({
      ...section,
      items: section.items.filter(
        (item) => canUseDocumentPanelShortcut || !item.documentPanelOnly,
      ),
    }))
    .filter((section) => section.items.length > 0);
}

interface KeyboardShortcutsProps {
  open: boolean;
  onClose: () => void;
  canUseDocumentPanelShortcut?: boolean;
}

export function KeyboardShortcuts({
  open,
  onClose,
  canUseDocumentPanelShortcut = true,
}: KeyboardShortcutsProps) {
  const shortcuts = getKeyboardShortcutSections(canUseDocumentPanelShortcut);

  // Global ? listener
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (isWorkspaceTextEntryTarget(e.target as HTMLElement)) return;

      if (e.key === "?" && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        onClose(); // Toggle — parent handles state
      }

    };

    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <DialogShell
      labelledBy="keyboard-shortcuts-title"
      onClose={onClose}
    >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 id="keyboard-shortcuts-title" className="text-sm font-medium text-text">
            Keyboard Shortcuts
          </h2>
          <Button
            type="button"
            onClick={onClose}
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            aria-label="Close keyboard shortcuts"
          >
            <X size={14} />
          </Button>
        </div>

        {/* Content */}
        <div className="max-h-96 overflow-y-auto p-4">
          {shortcuts.map((section) => (
            <div key={section.category} className="mb-4 last:mb-0">
              <h3 className="mb-2 text-xs font-medium text-text-muted">
                {section.category}
              </h3>
              <div className="space-y-1">
                {section.items.map((item) => (
                  <div
                    key={item.keys}
                    className="flex items-center justify-between py-1"
                  >
                    <span className="text-sm text-text">{item.description}</span>
                    <div className="flex gap-0.5">
                      {item.keys.split("+").map((key, i) => (
                        <kbd
                          key={i}
                          className="rounded border border-border bg-bg px-1.5 py-0.5 font-mono text-[11px] text-text-muted"
                        >
                          {key}
                        </kbd>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
    </DialogShell>
  );
}
