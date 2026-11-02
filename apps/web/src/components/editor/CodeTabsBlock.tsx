import { NodeViewWrapper, type ReactNodeViewProps } from "@tiptap/react";
import { Code2, Plus, X } from "lucide-react";
import { useRef, useState } from "react";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { CodeLanguagePicker, CODE_LANGUAGE_OPTIONS } from "@/components/editor/CodeBlockNodeView";
import { highlightedCodeToHtml } from "@/components/editor/Editor";
import { cn } from "@/lib/utils";

export interface CodeTabValue {
  label: string;
  language: string;
  code: string;
}

export const CODE_TAB_LANGUAGE_OPTIONS = CODE_LANGUAGE_OPTIONS;

const MAX_CODE_TABS = 10;

function normalizeTab(tab: Partial<CodeTabValue>, _index: number): CodeTabValue {
  const label = typeof tab.label === "string" ? tab.label.trim().slice(0, 80) : "";
  return {
    // Keep an intentionally empty label empty while the user edits it. The
    // tab strip supplies a display fallback, rather than mutating the value
    // on every keystroke.
    label,
    language: String(tab.language || "").trim().slice(0, 32),
    code: String(tab.code || ""),
  };
}

export function normalizeCodeTabs(value: unknown): CodeTabValue[] {
  if (!Array.isArray(value)) return [normalizeTab({ label: "Tab 1" }, 0)];
  const tabs = value.slice(0, MAX_CODE_TABS).map((tab, index) =>
    normalizeTab(tab && typeof tab === "object" ? (tab as Partial<CodeTabValue>) : {}, index),
  );
  return tabs.length > 0 ? tabs : [normalizeTab({ label: "Tab 1" }, 0)];
}

export function parseCodeTabsSource(source: string): { title?: string; tabs: CodeTabValue[] } {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const titleLine = lines[0]?.match(/^title:\s*(.+)$/i);
  const tabs: CodeTabValue[] = [];
  let current: CodeTabValue | null = null;
  for (const line of titleLine ? lines.slice(1) : lines) {
    const tab = line.match(/^tab:\s*([^|\n]*?)(?:\s*\|\s*([\w-]+))?\s*$/i);
    if (tab) {
      if (current) tabs.push(current);
      current = normalizeTab({ label: tab[1], language: tab[2] }, tabs.length);
      continue;
    }
    if (line.trim() === "---" && current) {
      tabs.push(current);
      current = null;
      continue;
    }
    if (current) current.code += `${line}\n`;
  }
  if (current) tabs.push(current);
  return {
    title: titleLine?.[1].trim().slice(0, 120),
    tabs: normalizeCodeTabs(tabs.map((tab) => ({ ...tab, code: tab.code.replace(/\n$/, "") }))),
  };
}

export function serializeCodeTabs(title: string | undefined, tabs: CodeTabValue[]): string {
  const normalized = normalizeCodeTabs(tabs);
  return [
    "```code-tabs",
    title?.trim() ? `title: ${title.trim().slice(0, 120)}` : "",
    ...normalized.flatMap((tab, index) => [
      `tab: ${tab.label} | ${tab.language || "text"}`,
      tab.code,
      ...(index < normalized.length - 1 ? ["---"] : []),
    ]),
    "```",
  ]
    .filter((line, index, lines) => !(line === "" && index === lines.length - 1))
    .join("\n");
}

export function CodeTabsNodeView({ node, updateAttributes, editor }: ReactNodeViewProps) {
  const [activeIndex, setActiveIndex] = useState(0);
  const tabs = normalizeCodeTabs(node.attrs.tabs);
  const active = tabs[Math.min(activeIndex, tabs.length - 1)] || tabs[0];
  const codeScrollRef = useRef<HTMLTextAreaElement | null>(null);
  const highlightedScrollRef = useRef<HTMLPreElement | null>(null);
  if (!active) return null;

  const updateTabs = (next: CodeTabValue[]) => updateAttributes({ tabs: normalizeCodeTabs(next) });
  const updateActive = (patch: Partial<CodeTabValue>) =>
    updateTabs(tabs.map((tab, index) => (index === activeIndex ? { ...tab, ...patch } : tab)));
  const activeLabel = active.label || `Tab ${activeIndex + 1}`;

  return (
    <NodeViewWrapper
      data-code-tabs-block="true"
      className="code-block-frame border-border bg-bg shadow-card my-3 overflow-hidden border"
      contentEditable={false}
    >
      <div className="code-block-frame__header border-border bg-surface border-b p-2">
        <div className="flex items-center gap-1 overflow-x-auto">
        {tabs.map((tab, index) => (
          <button
            key={`${tab.label}-${index}`}
            type="button"
            aria-selected={index === activeIndex}
            className={cn(
              "rounded-button shrink-0 px-3 py-1.5 text-xs transition-colors duration-100",
              index === activeIndex
                ? "bg-primary text-primary-foreground"
                : "text-text-muted hover:bg-surface",
            )}
            onClick={() => setActiveIndex(index)}
          >
            {tab.label || `Tab ${index + 1}`}
          </button>
        ))}
        {editor.isEditable && tabs.length < MAX_CODE_TABS ? (
          <button
            type="button"
            aria-label="Add code tab"
            className="rounded-button text-text-muted hover:bg-surface shrink-0 p-1.5"
            onClick={() => {
              updateTabs([...tabs, normalizeTab({ label: `Tab ${tabs.length + 1}` }, tabs.length)]);
              setActiveIndex(tabs.length);
            }}
          >
            <Plus size={14} />
          </button>
        ) : null}
        </div>
      </div>
      {editor.isEditable ? (
        <div
          className="grid min-w-0 gap-2 p-3 sm:grid-cols-[12rem_minmax(0,1fr)]"
          onPointerDown={(event) => event.stopPropagation()}
          onMouseDown={(event) => event.stopPropagation()}
          onClick={(event) => event.stopPropagation()}
        >
          <CodeLanguagePicker
            language={active.language}
            onChange={(language) => updateActive({ language })}
          />
          <span className="relative flex min-w-0 items-center">
            <Code2
              aria-hidden="true"
              className="text-text-muted pointer-events-none absolute left-3"
              size={15}
              strokeWidth={1.75}
            />
            <Input
              value={active.label}
              aria-label="Code tab name"
              onChange={(event) => updateActive({ label: event.target.value })}
              placeholder="Optional name"
              className="bg-surface-raised h-10 min-w-0 pl-9"
            />
          </span>
          <div className="code-tabs-editor-surface border-border bg-bg focus-within:ring-primary/50 relative min-h-32 overflow-hidden border sm:col-span-2 sm:focus-within:ring-1">
            <pre
              ref={highlightedScrollRef}
              aria-hidden="true"
              className="code-tabs-editor-surface__highlight pointer-events-none absolute inset-0 m-0 overflow-hidden p-3 font-mono text-sm"
              dangerouslySetInnerHTML={{
                __html: highlightedCodeToHtml(active.code, active.language),
              }}
            />
            <Textarea
              ref={codeScrollRef}
              value={active.code}
              aria-label={`${activeLabel} code`}
              onChange={(event) => updateActive({ code: event.target.value })}
              onScroll={(event) => {
                if (!highlightedScrollRef.current) return;
                highlightedScrollRef.current.scrollTop = event.currentTarget.scrollTop;
                highlightedScrollRef.current.scrollLeft = event.currentTarget.scrollLeft;
              }}
              onKeyDown={(event) => {
                if (event.key !== "Tab") return;
                event.preventDefault();
                const start = event.currentTarget.selectionStart;
                const end = event.currentTarget.selectionEnd;
                const next = `${active.code.slice(0, start)}  ${active.code.slice(end)}`;
                updateActive({ code: next });
                requestAnimationFrame(() => {
                  event.currentTarget.selectionStart = start + 2;
                  event.currentTarget.selectionEnd = start + 2;
                });
              }}
              className="code-tabs-editor-surface__input text-text focus-ring relative z-10 min-h-32 resize-y border-0 bg-transparent p-3 font-mono text-sm [color:transparent] caret-text selection:bg-primary/30 [-webkit-text-fill-color:transparent]"
              placeholder="Enter code"
            />
          </div>
          {tabs.length > 1 ? (
            <button
              type="button"
              className="text-danger hover:bg-danger/10 rounded-button inline-flex w-fit items-center gap-1 px-2 py-1 text-xs"
              onClick={() => {
                const next = tabs.filter((_, index) => index !== activeIndex);
                updateTabs(next);
                setActiveIndex(Math.max(0, activeIndex - 1));
              }}
            >
              <X size={13} /> Remove tab
            </button>
          ) : null}
        </div>
      ) : (
        <pre className="code-block-frame__content bg-bg m-0 overflow-x-auto p-4 font-mono text-sm">
          <code
            className={active.language ? `language-${active.language}` : undefined}
            dangerouslySetInnerHTML={{
              __html: highlightedCodeToHtml(active.code, active.language),
            }}
          />
        </pre>
      )}
    </NodeViewWrapper>
  );
}
