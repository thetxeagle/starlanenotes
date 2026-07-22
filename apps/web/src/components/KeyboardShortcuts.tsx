import { useEffect, useRef } from "react";
import { X } from "lucide-react";

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
        description: "Toggle document side panel",
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
      { keys: `${mod}+N`, description: "New document" },
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
  const modalRef = useRef<HTMLDivElement>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const shortcuts = getKeyboardShortcutSections(canUseDocumentPanelShortcut);

  useEffect(() => {
    if (open) {
      previousFocusRef.current = document.activeElement as HTMLElement | null;
      setTimeout(() => closeButtonRef.current?.focus(), 0);
    } else {
      previousFocusRef.current?.focus();
    }
  }, [open]);

  // Keep keyboard focus inside the modal while open
  useEffect(() => {
    if (!open) return;

    const handler = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const root = modalRef.current;
      if (!root) return;

      const focusables = root.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      );
      if (focusables.length === 0) return;

      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement as HTMLElement | null;

      if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open]);

  // Global ? listener
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Don't trigger when typing in inputs
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) {
        return;
      }

      if (e.key === "?" && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        onClose(); // Toggle — parent handles state
      }

      if (e.key === "Escape" && open) {
        onClose();
      }
    };

    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div className="fixed inset-0 bg-black/40" onClick={onClose} aria-hidden="true" />

      {/* Modal */}
      <div
        ref={modalRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="keyboard-shortcuts-title"
        className="relative w-full max-w-md rounded-card border border-border bg-surface shadow-card"
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 id="keyboard-shortcuts-title" className="text-sm font-medium text-text">
            Keyboard Shortcuts
          </h2>
          <button
            ref={closeButtonRef}
            type="button"
            onClick={onClose}
            className="flex h-6 w-6 items-center justify-center rounded-button text-text-muted hover:bg-surface-raised hover:text-text"
            aria-label="Close keyboard shortcuts"
          >
            <X size={14} />
          </button>
        </div>

        {/* Content */}
        <div className="max-h-96 overflow-y-auto p-4">
          {shortcuts.map((section) => (
            <div key={section.category} className="mb-4 last:mb-0">
              <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-text-muted">
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
      </div>
    </div>
  );
}
