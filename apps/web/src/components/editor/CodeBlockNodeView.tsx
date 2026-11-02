import { normalizeBlockPlacement } from "./blockPlacement";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import { cn } from "@/lib/utils";
import { NodeViewContent, NodeViewWrapper, type ReactNodeViewProps } from "@tiptap/react";
import { Check, ChevronDown, Code2, Copy } from "lucide-react";
import { NodeSelection } from "prosemirror-state";
import {
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type SyntheticEvent,
} from "react";

export const CODE_LANGUAGE_OPTIONS = [
  { value: "", label: "Plain text" },
  { value: "css", label: "CSS" },
  { value: "html", label: "HTML" },
  { value: "markdown", label: "Markdown" },
  { value: "javascript", label: "JavaScript" },
  { value: "typescript", label: "TypeScript" },
  { value: "tsx", label: "TSX" },
  { value: "json", label: "JSON" },
  { value: "bash", label: "Bash" },
  { value: "powershell", label: "PowerShell" },
  { value: "rust", label: "Rust" },
  { value: "python", label: "Python" },
  { value: "sql", label: "SQL" },
  { value: "ocaml", label: "OCaml" },
  { value: "yaml", label: "YAML" },
] as const;

export function CodeLanguagePicker({
  language,
  onChange,
}: {
  language: string;
  onChange: (language: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const handleMenuKeyDown = useMenuKeyboardNavigation(menuRef);
  const selected =
    CODE_LANGUAGE_OPTIONS.find((option) => option.value === language) ?? CODE_LANGUAGE_OPTIONS[0];

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label="Code language"
          aria-haspopup="menu"
          aria-expanded={open}
          className="focus-ring border-border bg-surface-raised text-text hover:border-text-muted rounded-input flex h-10 w-full min-w-0 items-center justify-between gap-2 border px-3 text-left text-sm transition-colors duration-100"
        >
          <span className="truncate">{selected.label}</span>
          <ChevronDown
            aria-hidden="true"
            className={cn(
              "text-text-muted shrink-0 transition-transform duration-100",
              open && "rotate-180",
            )}
            size={14}
            strokeWidth={1.75}
          />
        </button>
      </PopoverTrigger>
      <PopoverContent
        ref={menuRef}
        align="start"
        sideOffset={4}
        className="max-h-[min(22rem,calc(100dvh-2rem))] w-[min(12rem,calc(100vw-2rem))] overflow-y-auto p-1"
        role="menu"
        aria-label="Code languages"
        onPointerDown={(event) => event.stopPropagation()}
        onClick={(event) => event.stopPropagation()}
        onKeyDown={handleMenuKeyDown}
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          window.requestAnimationFrame(() => {
            menuRef.current
              ?.querySelector<HTMLButtonElement>('[role="menuitemradio"][aria-checked="true"]')
              ?.focus();
          });
        }}
      >
        {CODE_LANGUAGE_OPTIONS.map((option) => {
          const active = option.value === language;
          return (
            <button
              key={option.value || "plain"}
              type="button"
              role="menuitemradio"
              aria-checked={active}
              className={cn(
                "focus-ring rounded-button flex min-h-9 w-full items-center justify-between gap-3 px-2.5 text-left text-sm transition-colors duration-100",
                active
                  ? "bg-primary/15 text-primary"
                  : "text-text-muted hover:bg-surface-raised hover:text-text",
              )}
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onChange(option.value);
                setOpen(false);
              }}
              onKeyDown={(event) => {
                if (event.key !== "Enter" && event.key !== " ") return;
                event.preventDefault();
                event.stopPropagation();
                onChange(option.value);
                setOpen(false);
              }}
            >
              <span>{option.label}</span>
              {active && <Check aria-hidden="true" size={14} strokeWidth={1.75} />}
            </button>
          );
        })}
      </PopoverContent>
    </Popover>
  );
}

/**
 * Editor-owned framed code block. The header deliberately lives inside the
 * node view so its controls cannot escape into document/page chrome.
 */
export function CodeBlockNodeView({ node, updateAttributes, editor, getPos }: ReactNodeViewProps) {
  const [copied, setCopied] = useState(false);
  const language = typeof node.attrs.language === "string" ? node.attrs.language : "";
  const title = typeof node.attrs.title === "string" ? node.attrs.title : "";
  const isEditable = editor.isEditable;

  const copyCode = async () => {
    try {
      await navigator.clipboard.writeText(node.textContent);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  };

  const keepMetadataControlInteraction = (event: SyntheticEvent) => {
    event.stopPropagation();
  };

  return (
    <NodeViewWrapper
      data-starlane-code-frame="true"
      data-block-placement={normalizeBlockPlacement(node.attrs.placement)}
      data-block-id={typeof node.attrs.id === "string" ? node.attrs.id : undefined}
      data-block-type="codeBlock"
      className="code-block-frame border-border bg-bg shadow-card my-3 overflow-hidden border"
      onPointerDown={(event: ReactPointerEvent<HTMLDivElement>) => {
        if (!editor.isEditable || !(event.target instanceof Element)) return;
        if (!event.target.closest(".code-block-frame__header")) return;
        if (event.target.closest("[data-code-block-edit-controls='true']")) return;
        if (typeof getPos !== "function") return;
        const position = getPos();
        if (typeof position !== "number") return;
        event.preventDefault();
        editor.view.dispatch(
          editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, position)),
        );
        editor.view.focus();
      }}
    >
      <div
        className="code-block-frame__header border-border bg-surface border-b p-2"
        contentEditable={false}
      >
        {isEditable ? (
          <div
            data-code-block-edit-controls="true"
            className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-[12rem_minmax(0,1fr)]"
            onPointerDown={keepMetadataControlInteraction}
            onMouseDown={keepMetadataControlInteraction}
            onClick={keepMetadataControlInteraction}
            onKeyDown={keepMetadataControlInteraction}
          >
            <div className="code-block-metadata-field min-w-0">
              <CodeLanguagePicker
                language={language}
                onChange={(value) => updateAttributes({ language: value })}
              />
            </div>
            <div className="code-block-metadata-field min-w-0">
              <span className="relative flex min-w-0 items-center">
                <Code2
                  aria-hidden="true"
                  className="text-text-muted pointer-events-none absolute left-3"
                  size={15}
                  strokeWidth={1.75}
                />
                <Input
                  id={`code-title-${node.attrs.id ?? node.attrs.title ?? "block"}`}
                  value={title}
                  onChange={(event) =>
                    updateAttributes({ title: event.target.value.slice(0, 120) })
                  }
                  placeholder="Optional name"
                  className="bg-surface-raised h-10 min-w-0 pl-9 text-sm"
                  aria-label="Code block name"
                />
              </span>
            </div>
          </div>
        ) : (
          <div className="flex min-h-8 items-center gap-2">
            <span className="text-text-muted min-w-0 flex-1 truncate font-mono text-xs">
              {title ||
                CODE_LANGUAGE_OPTIONS.find((option) => option.value === language)?.label ||
                "Plain text"}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => void copyCode()}
              className={cn("ml-auto shrink-0", copied && "text-primary")}
              aria-label={`Copy ${title || language || "plain text"} code`}
            >
              {copied ? <Check data-icon="inline-start" /> : <Copy data-icon="inline-start" />}
              <span>{copied ? "Copied" : "Copy"}</span>
            </Button>
          </div>
        )}
      </div>
      <NodeViewContent className="code-block-frame__content min-h-14 overflow-x-auto px-4 py-3 font-mono text-sm" />
    </NodeViewWrapper>
  );
}
