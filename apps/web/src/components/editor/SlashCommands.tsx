import { useEffect, useRef, useState } from "react";
import {
  Heading1,
  Heading2,
  Heading3,
  Heading4,
  Heading5,
  Heading6,
  List,
  ListOrdered,
  CheckSquare,
  Table,
  Database,
  FileCode,
  Quote,
  AlertCircle,
  Minus,
  Upload,
  Calculator,
  Link,
  Bookmark,
  AtSign,
  Calendar,
  FileText,
  FileType,
  Pilcrow,
  SquarePen,
} from "lucide-react";
import { renderTemplateContent, type TemplateRenderContext } from "@/lib/templates";
import { SearchInput } from "@/components/ui/search-input";

export interface SlashCommand {
  id: string;
  label: string;
  description: string;
  icon: React.ReactNode;
  action: string; // The markdown to insert
  cursorOffset?: number;
}

export interface SlashTemplate {
  id: string;
  name: string;
  description?: string;
  content: string;
}

/** Format a local calendar date for insertion into Markdown. */
export function formatSmartDate(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

const baseCommands: SlashCommand[] = [
  {
    id: "paragraph",
    label: "Paragraph",
    description: "Plain text block",
    icon: <Pilcrow size={16} />,
    action: "",
  },
  {
    id: "h1",
    label: "Heading 1",
    description: "Large heading",
    icon: <Heading1 size={16} />,
    action: "# ",
  },
  {
    id: "h2",
    label: "Heading 2",
    description: "Medium heading",
    icon: <Heading2 size={16} />,
    action: "## ",
  },
  {
    id: "h3",
    label: "Heading 3",
    description: "Small heading",
    icon: <Heading3 size={16} />,
    action: "### ",
  },
  {
    id: "h4",
    label: "Heading 4",
    description: "Tiny heading",
    icon: <Heading4 size={16} />,
    action: "#### ",
  },
  {
    id: "h5",
    label: "Heading 5",
    description: "Smaller heading",
    icon: <Heading5 size={16} />,
    action: "##### ",
  },
  {
    id: "h6",
    label: "Heading 6",
    description: "Smallest heading",
    icon: <Heading6 size={16} />,
    action: "###### ",
  },
  {
    id: "bullet",
    label: "Bullet list",
    description: "Unordered list",
    icon: <List size={16} />,
    action: "- ",
  },
  {
    id: "numbered",
    label: "Numbered list",
    description: "Ordered list",
    icon: <ListOrdered size={16} />,
    action: "1. ",
  },
  {
    id: "task",
    label: "Task list",
    description: "Checkbox list",
    icon: <CheckSquare size={16} />,
    action: "- [ ] ",
  },
  {
    id: "table",
    label: "Table",
    description: "Insert a table",
    icon: <Table size={16} />,
    action: "| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n| Cell | Cell | Cell |\n",
  },
  {
    id: "collection",
    label: "Collection view",
    description: "Saved property-backed page collection",
    icon: <Database size={16} />,
    action: [
      "```dataview",
      "name: Collection",
      'Status = "Open"',
      "view: table",
      "columns: title, space, updated",
      "sort: updated desc",
      "```",
    ].join("\n"),
  },
  {
    id: "code",
    label: "Code block",
    description: "Fenced code block",
    icon: <FileCode size={16} />,
    action: "```\n\n```",
  },
  {
    id: "quote",
    label: "Quote",
    description: "Block quote",
    icon: <Quote size={16} />,
    action: "> ",
  },
  {
    id: "callout",
    label: "Callout",
    description: "Note, warning, tip",
    icon: <AlertCircle size={16} />,
    action: "> [!NOTE]\n> ",
  },
  {
    id: "divider",
    label: "Divider",
    description: "Horizontal rule",
    icon: <Minus size={16} />,
    action: "---\n",
  },
  {
    id: "upload",
    label: "Upload",
    description: "Insert an image or attach a file",
    icon: <Upload size={16} />,
    action: "",
  },
  {
    id: "math",
    label: "Math block",
    description: "LaTeX equation",
    icon: <Calculator size={16} />,
    action: "$$\n\n$$",
  },
  {
    id: "bookmark",
    label: "Bookmark card",
    description: "Link preview card",
    icon: <Bookmark size={16} />,
    action: "[bookmark:Bookmark](https://example.com)",
  },
  {
    id: "link",
    label: "Link",
    description: "Insert a link",
    icon: <Link size={16} />,
    action: "[text](url)",
  },
  {
    id: "wiki-link",
    label: "Wiki link",
    description: "Link to a document in this space",
    icon: <Link size={16} />,
    action: "",
  },
  {
    id: "mention",
    label: "Mention",
    description: "Mention a user",
    icon: <AtSign size={16} />,
    action: "@",
  },
  {
    id: "date",
    label: "Smart date",
    description: "Insert today's date",
    icon: <Calendar size={16} />,
    action: "",
  },
  {
    id: "excalidraw",
    label: "Excalidraw canvas",
    description: "Insert a drawing canvas placeholder",
    icon: <SquarePen size={16} />,
    action: "![[excalidraw:untitled.excalidraw]]",
  },
  {
    id: "template",
    label: "Template",
    description: "Use a template",
    icon: <FileType size={16} />,
    action: "",
  },
];

function buildCommands(
  templates: SlashTemplate[],
  templateContext: TemplateRenderContext,
): SlashCommand[] {
  const templateCommands = templates.map((template) => {
    const rendered = renderTemplateContent(template.content, {
      ...templateContext,
      title: templateContext.title || template.name,
    });
    return {
      id: `template:${template.id}`,
      label: `Template: ${template.name}`,
      description: template.description || "Insert template content",
      icon: <FileType size={16} />,
      action: rendered.content,
      cursorOffset: rendered.cursorOffset,
    };
  });
  const dynamicCommands: SlashCommand[] = [
    {
      id: "dynamic:date",
      label: "Current date",
      description: "Insert today's date",
      icon: <Calendar size={16} />,
      action: renderTemplateContent("{{date}}", templateContext).content,
    },
    {
      id: "dynamic:time",
      label: "Current time",
      description: "Insert the current time",
      icon: <Calendar size={16} />,
      action: renderTemplateContent("{{time}}", templateContext).content,
    },
    {
      id: "dynamic:datetime",
      label: "Current date and time",
      description: "Insert the current date and time",
      icon: <Calendar size={16} />,
      action: renderTemplateContent("{{datetime}}", templateContext).content,
    },
    {
      id: "dynamic:space",
      label: "Space name",
      description: "Insert the current space name",
      icon: <FileText size={16} />,
      action: renderTemplateContent("{{space}}", templateContext).content,
    },
    {
      id: "dynamic:author",
      label: "Author",
      description: "Insert your display name",
      icon: <AtSign size={16} />,
      action: renderTemplateContent("{{author}}", templateContext).content,
    },
  ].filter((command) => command.action);

  if (templateCommands.length === 0) {
    return baseCommands
      .filter((command) => command.id !== "template")
      .flatMap((command) => (command.id === "date" ? dynamicCommands : [command]));
  }
  return baseCommands.flatMap((command) =>
    command.id === "template"
      ? templateCommands
      : command.id === "date"
        ? dynamicCommands
        : [command],
  );
}

interface SlashCommandPaletteProps {
  query: string;
  position: { top: number; left: number };
  searchable?: boolean;
  templates?: SlashTemplate[];
  templateContext?: TemplateRenderContext;
  onSelect: (command: SlashCommand) => void;
  onClose: () => void;
}

export function SlashCommandPalette({
  query,
  position,
  searchable = false,
  templates = [],
  templateContext = {},
  onSelect,
  onClose,
}: SlashCommandPaletteProps) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [searchQuery, setSearchQuery] = useState(query);
  const listRef = useRef<HTMLDivElement>(null);
  const commands = buildCommands(templates, templateContext);
  const activeQuery = searchable ? searchQuery : query;

  const filtered = commands.filter(
    (cmd) =>
      cmd.label.toLowerCase().includes(activeQuery.toLowerCase()) ||
      cmd.description.toLowerCase().includes(activeQuery.toLowerCase()),
  );
  const viewportWidth = typeof window === "undefined" ? 1024 : window.innerWidth;
  const viewportHeight = typeof window === "undefined" ? 768 : window.innerHeight;
  const compactViewport = viewportWidth < 640;
  const paletteWidth = Math.min(384, Math.max(0, viewportWidth - 16));
  const paletteHeight = compactViewport
    ? Math.min(560, Math.max(0, viewportHeight - 32))
    : Math.min(420, Math.max(0, viewportHeight - 16));
  const clampedPosition = {
    top: Math.max(8, Math.min(position.top, viewportHeight - paletteHeight - 8)),
    left: Math.max(8, Math.min(position.left, viewportWidth - paletteWidth - 8)),
  };

  // Reset selection when filter changes
  useEffect(() => {
    setSelectedIndex(0);
  }, [activeQuery]);

  // Keyboard navigation
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === "Enter") {
        e.preventDefault();
        if (filtered[selectedIndex]) {
          onSelect(filtered[selectedIndex]);
        }
      } else if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };

    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [filtered, selectedIndex, onSelect, onClose]);

  // Scroll selected item into view
  useEffect(() => {
    const el = listRef.current?.children[selectedIndex] as HTMLElement;
    el?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  return (
    <>
      <button
        type="button"
        aria-label="Close command menu"
        className={`fixed inset-0 z-[75] cursor-default ${compactViewport ? "bg-black/40" : "bg-transparent"}`}
        onMouseDown={(event) => event.preventDefault()}
        onClick={onClose}
      />
      <div
        className={`starlane-menu-surface fixed z-[80] overflow-hidden p-0 ${
          compactViewport
            ? "left-1/2 top-1/2 max-h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] max-w-lg -translate-x-1/2 -translate-y-1/2"
            : "w-[min(24rem,calc(100vw-1rem))]"
        }`}
        style={compactViewport ? undefined : { top: clampedPosition.top, left: clampedPosition.left }}
        role="dialog"
        aria-label="Command menu"
      >
        <div className="border-border border-b px-3 py-1">
          {searchable ? (
            <SearchInput
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              autoFocus
              placeholder="Search commands"
              aria-label="Search commands"
              className="h-8 border-0 bg-transparent px-2 pl-8 text-sm focus:ring-0 focus:ring-offset-0"
              containerClassName="w-full"
            />
          ) : (
            <span className="text-text-muted flex h-8 flex-1 items-center text-sm">
              Command menu
            </span>
          )}
        </div>
        <div
          ref={listRef}
          className={`${compactViewport ? "max-h-[min(26rem,calc(100dvh-10rem))]" : "max-h-80"} scrollbar-none overflow-y-auto p-1`}
        >
          {filtered.length > 0 ? (
            filtered.map((cmd, i) => (
              <button
                key={cmd.id}
                type="button"
                onClick={() => onSelect(cmd)}
                className={`rounded-button flex w-full items-center gap-3 px-3 py-2 text-left transition-colors duration-100 ${
                  i === selectedIndex
                    ? "bg-surface text-text"
                    : "text-text-muted hover:bg-surface hover:text-text"
                }`}
              >
                <span className="text-text-muted flex h-7 w-7 shrink-0 items-center justify-center">
                  {cmd.icon}
                </span>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{cmd.label}</div>
                  <div className="text-text-muted truncate text-xs">{cmd.description}</div>
                </div>
              </button>
            ))
          ) : (
            <div className="text-text-muted px-3 py-4 text-center text-sm">No commands found</div>
          )}
        </div>
        <div className="border-border text-text-muted flex items-center gap-3 border-t px-3 py-1.5 text-[10px]">
          <span>
            <kbd className="border-border rounded border px-1 font-mono">↑↓</kbd> navigate
          </span>
          <span>
            <kbd className="border-border rounded border px-1 font-mono">↵</kbd> run
          </span>
          <span>
            <kbd className="border-border rounded border px-1 font-mono">esc</kbd> close
          </span>
        </div>
      </div>
    </>
  );
}

export const commands = baseCommands;
