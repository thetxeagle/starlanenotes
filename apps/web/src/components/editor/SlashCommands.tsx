import { useEffect, useRef, useState, type RefObject } from "react";
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
  Image,
  Play,
  Columns2,
  Rows3,
  ListTree,
  BookOpenText,
  Braces,
} from "lucide-react";
import { renderTemplateContent, type TemplateRenderContext } from "@/lib/templates";
import { serializeTemplateMarker } from "@/lib/templateMarkers";
import { SearchInput } from "@/components/ui/search-input";
import { useOverlayFocusLifecycle } from "@/components/ui/useOverlayFocusLifecycle";
import {
  ColumnLayoutPreview,
  SECTION_LAYOUT_OPTIONS,
  type SectionLayout,
} from "@/components/editor/EditorSection";

export type ColumnLayout = Exclude<SectionLayout, "single">;

export interface SlashCommand {
  id: string;
  label: string;
  description: string;
  /** Searchable shorthand and familiar names for the same portable action. */
  aliases?: string[];
  icon: React.ReactNode;
  action: string; // The markdown to insert
  cursorOffset?: number;
  /** Creates one portable columns node using the selected visible layout preset. */
  columnLayout?: ColumnLayout;
}

export interface SlashTemplate {
  id: string;
  name: string;
  description?: string;
  content: string;
  properties?: unknown;
}

export interface SlashTemplateField {
  id: string;
  label: string;
}

export const SLASH_COMMAND_GROUPS = [
  "Basic blocks",
  "Lists & tables",
  "Knowledge blocks",
  "Media & embeds",
  "Links & people",
  "Utilities",
  "Templates",
] as const;

export type SlashCommandGroup = (typeof SLASH_COMMAND_GROUPS)[number];

export type SlashCommandPaletteKeyboardAction = "next" | "previous" | "select" | "close" | "none";

/**
 * The searchable palette shares its Escape owner with the common overlay
 * lifecycle. Once that owner has prevented the event, the palette must not
 * close a second time and bypass its focus restoration callback.
 */
export function getSlashCommandPaletteKeyboardAction(
  key: string,
  defaultPrevented: boolean,
): SlashCommandPaletteKeyboardAction {
  if (defaultPrevented) return "none";
  if (key === "ArrowDown") return "next";
  if (key === "ArrowUp") return "previous";
  if (key === "Enter") return "select";
  if (key === "Escape") return "close";
  return "none";
}

export function getSlashCommandPaletteScopedKeyboardAction(
  key: string,
  defaultPrevented: boolean,
  scope: {
    searchable: boolean;
    targetInPalette: boolean;
    targetInTrigger: boolean;
    targetInOtherDialog: boolean;
  },
): SlashCommandPaletteKeyboardAction {
  if (
    scope.targetInOtherDialog ||
    (scope.searchable ? !scope.targetInPalette : !scope.targetInPalette && !scope.targetInTrigger)
  ) {
    return "none";
  }
  return getSlashCommandPaletteKeyboardAction(key, defaultPrevented);
}

export function getSlashCommandGroup(commandId: string): SlashCommandGroup {
  if (commandId === "template" || commandId.startsWith("template:") || commandId.startsWith("template-field:") || commandId === "template-guidance") return "Templates";
  if (commandId.startsWith("dynamic:")) return "Utilities";
  if (["bullet", "numbered", "task", "table"].includes(commandId)) return "Lists & tables";
  if (
    commandId.startsWith("columns:") ||
    [
      "collection",
      "toc",
      "child-pages",
      "child-pages-collapsible",
      "callout",
      "expand",
      "math",
    ].includes(commandId)
  )
    return "Knowledge blocks";
  if (["upload", "bookmark", "carousel"].includes(commandId)) return "Media & embeds";
  if (["link", "wiki-link", "mention"].includes(commandId)) return "Links & people";
  return "Basic blocks";
}

export function doesSlashCommandMatchQuery(command: SlashCommand, query: string): boolean {
  const normalizedQuery = query.trim().replace(/^\/+/, "").toLocaleLowerCase();
  if (!normalizedQuery) return true;
  return [command.id, command.label, command.description, ...(command.aliases ?? [])]
    .join(" ")
    .toLocaleLowerCase()
    .includes(normalizedQuery);
}

export function getSlashCommandOptionId(commandId: string): string {
  return `slash-command-option-${commandId.replace(/[^A-Za-z0-9_-]/g, "-")}`;
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
    aliases: ["text", "p"],
    icon: <Pilcrow size={16} />,
    action: "",
  },
  {
    id: "h1",
    label: "Heading 1",
    description: "Large heading",
    aliases: ["heading", "title", "h1"],
    icon: <Heading1 size={16} />,
    action: "# ",
  },
  {
    id: "h2",
    label: "Heading 2",
    description: "Medium heading",
    aliases: ["heading", "h2"],
    icon: <Heading2 size={16} />,
    action: "## ",
  },
  {
    id: "h3",
    label: "Heading 3",
    description: "Small heading",
    aliases: ["heading", "h3"],
    icon: <Heading3 size={16} />,
    action: "### ",
  },
  {
    id: "h4",
    label: "Heading 4",
    description: "Tiny heading",
    aliases: ["heading", "h4"],
    icon: <Heading4 size={16} />,
    action: "#### ",
  },
  {
    id: "h5",
    label: "Heading 5",
    description: "Smaller heading",
    aliases: ["heading", "h5"],
    icon: <Heading5 size={16} />,
    action: "##### ",
  },
  {
    id: "h6",
    label: "Heading 6",
    description: "Smallest heading",
    aliases: ["heading", "h6"],
    icon: <Heading6 size={16} />,
    action: "###### ",
  },
  {
    id: "bullet",
    label: "Bullet list",
    description: "Unordered list",
    aliases: ["bullets", "list", "ul"],
    icon: <List size={16} />,
    action: "- ",
  },
  {
    id: "numbered",
    label: "Numbered list",
    description: "Ordered list",
    aliases: ["ordered", "list", "ol"],
    icon: <ListOrdered size={16} />,
    action: "1. ",
  },
  {
    id: "task",
    label: "Task list",
    description: "Checkbox list",
    aliases: ["todo", "to-do", "checklist"],
    icon: <CheckSquare size={16} />,
    action: "- [ ] ",
  },
  {
    id: "table",
    label: "Table",
    description: "Insert a table",
    aliases: ["grid"],
    icon: <Table size={16} />,
    action: "| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n| Cell | Cell | Cell |\n",
  },
  {
    id: "collection",
    label: "Collection view",
    description: "Saved property-backed page collection",
    aliases: ["dataview", "database", "query"],
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
  ...SECTION_LAYOUT_OPTIONS.filter(
    (option): option is (typeof SECTION_LAYOUT_OPTIONS)[number] & { id: ColumnLayout } =>
      option.id !== "single",
  ).map((option) => ({
    id: `columns:${option.id}`,
    label: option.label,
    description: option.description,
    aliases: [
      "columns",
      "column layout",
      option.id.replace("-", " "),
      ...(option.id === "two-equal" ? ["two columns", "side by side"] : []),
      ...(option.id === "three-equal" ? ["three columns"] : []),
    ],
    icon: <Columns2 size={16} />,
    action: "",
    columnLayout: option.id,
  })),
  {
    id: "code",
    label: "Code block",
    description: "Fenced code block",
    aliases: ["codeblock", "fence", "pre"],
    icon: <FileCode size={16} />,
    action: "```\n\n```",
  },
  {
    id: "code-tabs",
    label: "Code tabs",
    description: "Tabbed portable code examples",
    aliases: ["code tabs", "snippet tabs", "examples"],
    icon: <FileCode size={16} />,
    action: [
      "```code-tabs",
      "title: Examples",
      "tab: TypeScript | ts",
      "const answer = 42;",
      "---",
      "tab: Python | python",
      "answer = 42",
      "```",
    ].join("\n"),
  },
  {
    id: "status",
    label: "Status",
    description: "Service or project status",
    aliases: ["online", "offline", "maintenance", "degraded"],
    icon: <AlertCircle size={16} />,
    action: [
      "```status",
      "state: online",
      "label: Service reachable",
      "detail: Last checked just now.",
      "```",
    ].join("\n"),
  },
  {
    id: "video",
    label: "Video player",
    description: "Portable video embed",
    aliases: ["media", "movie", "clip"],
    icon: <Play size={16} />,
    action: ["```video", "title: Demo", "src: https://example.com/video.mp4", "```"].join("\n"),
  },
  {
    id: "gallery",
    label: "Gallery",
    description: "Portable image gallery",
    aliases: ["images", "coverflow", "timeline", "photos"],
    icon: <Image size={16} />,
    action: [
      "```gallery",
      "layout: responsive-columns",
      "title: Gallery",
      "---",
      "![Caption](https://example.com/image.jpg)",
      "```",
    ].join("\n"),
  },
  {
    id: "carousel",
    label: "Carousel",
    description: "Swipeable image carousel with up to nine images",
    aliases: ["carousel", "slider", "photos"],
    icon: <Image size={16} />,
    action: [
      "```carousel",
      "title: Carousel",
      "---",
      "![Image 1](https://example.com/image.jpg)",
      "```",
    ].join("\n"),
  },
  {
    id: "quote",
    label: "Quote",
    description: "Block quote",
    aliases: ["blockquote", "citation"],
    icon: <Quote size={16} />,
    action: "> ",
  },
  {
    id: "callout",
    label: "Callout",
    description: "Note, warning, tip",
    aliases: ["note", "warning", "tip"],
    icon: <AlertCircle size={16} />,
    action: "> [!NOTE]\n> ",
  },
  {
    id: "expand",
    label: "Expand block",
    description: "Collapsible content for compact pages",
    aliases: ["collapse", "toggle", "details"],
    icon: <Rows3 size={16} />,
    action: ":::expand Click to expand\n\n:::",
  },
  {
    id: "child-pages",
    label: "Child pages",
    description: "List pages nested directly under this page",
    aliases: ["children", "subpages", "page index", "toc"],
    icon: <ListTree size={16} />,
    action: "[[children]]",
  },
  {
    id: "child-pages-collapsible",
    label: "Child pages (collapsible)",
    description: "Collapsible list of pages nested under this page",
    aliases: ["children", "subpages", "collapse", "toggle"],
    icon: <ListTree size={16} />,
    action: "[[children:collapsed]]",
  },
  {
    id: "divider",
    label: "Divider",
    description: "Horizontal rule",
    aliases: ["rule", "hr", "separator"],
    icon: <Minus size={16} />,
    action: "---\n",
  },
  {
    id: "upload",
    label: "Upload",
    description: "Insert an image or attach a file",
    aliases: ["image", "file", "attachment", "media"],
    icon: <Upload size={16} />,
    action: "",
  },
  {
    id: "math",
    label: "Math block",
    description: "LaTeX equation",
    aliases: ["equation", "latex"],
    icon: <Calculator size={16} />,
    action: "$$\n\n$$",
  },
  {
    id: "bookmark",
    label: "Bookmark card",
    description: "Link preview card",
    aliases: ["embed", "preview", "link preview"],
    icon: <Bookmark size={16} />,
    action: "[bookmark:Bookmark](https://example.com)",
  },
  {
    id: "link",
    label: "Link",
    description: "Insert a link",
    aliases: ["url", "hyperlink"],
    icon: <Link size={16} />,
    action: "[text](url)",
  },
  {
    id: "wiki-link",
    label: "Wiki link",
    description: "Link to a document in this space",
    aliases: ["wikilink", "page link", "internal link"],
    icon: <Link size={16} />,
    action: "",
  },
  {
    id: "mention",
    label: "Mention",
    description: "Mention a user",
    aliases: ["user", "person"],
    icon: <AtSign size={16} />,
    action: "@",
  },
  {
    id: "date",
    label: "Smart date",
    description: "Insert today's date",
    aliases: ["today", "date"],
    icon: <Calendar size={16} />,
    action: "",
  },
  {
    id: "template",
    label: "Template",
    description: "Use a template",
    aliases: ["snippet"],
    icon: <FileType size={16} />,
    action: "",
  },
];

export function getSlashCommandCatalog(
  templates: SlashTemplate[],
  templateContext: TemplateRenderContext,
  templateFields: SlashTemplateField[] = [],
  templateAuthoring = false,
): SlashCommand[] {
  const markerId = () =>
    globalThis.crypto?.randomUUID?.() ?? `marker_${Date.now()}_${Math.random().toString(36).slice(2)}`;
  const templateCommands = templates.map((template) => {
    const rendered = renderTemplateContent(template.content, {
      ...templateContext,
      title: templateContext.title || template.name,
    });
    return {
      id: `template:${template.id}`,
      label: `Template: ${template.name}`,
      description: template.description || "Insert template content",
      aliases: [template.name, "template"],
      icon: <FileType size={16} />,
      action: rendered.content,
      cursorOffset: rendered.cursorOffset,
    };
  });
  const browseTemplatesCommand: SlashCommand = {
    id: "template",
    label: "Browse reusable sections",
    description: "Find and insert a reusable section into this page",
    aliases: ["template", "snippet", "browse"],
    icon: <FileType size={16} />,
    action: "",
  };
  const authoringCommands: SlashCommand[] = templateAuthoring
    ? [
        {
          id: "template-guidance",
          label: "Writing guidance",
          description: "Insert replaceable instructions for the author",
          aliases: ["guidance", "instruction", "placeholder"],
          icon: <BookOpenText size={16} />,
          action: serializeTemplateMarker({
            kind: "guidance",
            id: markerId(),
            text: "Add a writing instruction for this section.",
          }),
        },
        ...templateFields.map((field) => ({
          id: `template-field:${field.id}`,
          label: `Insert field: ${field.label}`,
          description: `Insert the ${field.label} input at the caret`,
          aliases: [field.label, "field", "input"],
          icon: <Braces size={16} />,
          action: serializeTemplateMarker({
            kind: "field",
            id: markerId(),
            field_id: field.id,
          }),
        })),
      ]
    : [];
  const dynamicCommands: SlashCommand[] = [
    {
      id: "dynamic:date",
      label: "Current date",
      description: "Insert today's date",
      aliases: ["today", "date"],
      icon: <Calendar size={16} />,
      action: renderTemplateContent("{{date}}", templateContext).content,
    },
    {
      id: "dynamic:time",
      label: "Current time",
      description: "Insert the current time",
      aliases: ["time", "clock"],
      icon: <Calendar size={16} />,
      action: renderTemplateContent("{{time}}", templateContext).content,
    },
    {
      id: "dynamic:datetime",
      label: "Current date and time",
      description: "Insert the current date and time",
      aliases: ["datetime", "timestamp", "now"],
      icon: <Calendar size={16} />,
      action: renderTemplateContent("{{datetime}}", templateContext).content,
    },
    {
      id: "dynamic:space",
      label: "Space name",
      description: "Insert the current space name",
      aliases: ["workspace", "space"],
      icon: <FileText size={16} />,
      action: renderTemplateContent("{{space}}", templateContext).content,
    },
    {
      id: "dynamic:author",
      label: "Author",
      description: "Insert your display name",
      aliases: ["user", "me"],
      icon: <AtSign size={16} />,
      action: renderTemplateContent("{{author}}", templateContext).content,
    },
  ].filter((command) => command.action);

  if (templateCommands.length === 0) {
    return baseCommands
      .flatMap((command) =>
        command.id === "template"
          ? [browseTemplatesCommand]
          : command.id === "date"
            ? [...dynamicCommands, ...authoringCommands]
          : [command],
      );
  }
  return baseCommands.flatMap((command) =>
    command.id === "template"
      ? [browseTemplatesCommand, ...templateCommands]
        : command.id === "date"
        ? [...dynamicCommands, ...authoringCommands]
        : [command],
  );
}

interface SlashCommandPaletteProps {
  query: string;
  position: { top: number; left: number };
  searchable?: boolean;
  templates?: SlashTemplate[];
  templateFields?: SlashTemplateField[];
  templateAuthoring?: boolean;
  templateContext?: TemplateRenderContext;
  onSelect: (command: SlashCommand) => void;
  onClose: () => void;
  /** The editor block or toolbar control that opened the searchable palette. */
  triggerRef?: RefObject<HTMLElement | SVGElement | null>;
}

export function SlashCommandPalette({
  query,
  position,
  searchable = false,
  templates = [],
  templateFields = [],
  templateAuthoring = false,
  templateContext = {},
  onSelect,
  onClose,
  triggerRef,
}: SlashCommandPaletteProps) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [searchQuery, setSearchQuery] = useState(query);
  const listRef = useRef<HTMLDivElement>(null);
  const paletteRef = useRef<HTMLDivElement>(null);
  const commands = getSlashCommandCatalog(templates, templateContext, templateFields, templateAuthoring);
  const activeQuery = searchable ? searchQuery : query;

  const filtered = commands.filter((command) => doesSlashCommandMatchQuery(command, activeQuery));
  const columnsOnly =
    filtered.length > 0 && filtered.every((command) => Boolean(command.columnLayout));
  const groupedCommands = SLASH_COMMAND_GROUPS.map((group) => ({
    group,
    commands: filtered
      .map((command, index) => ({ command, index }))
      .filter(({ command }) => getSlashCommandGroup(command.id) === group),
  })).filter(({ commands }) => commands.length > 0);
  const activeCommand = filtered[selectedIndex];
  const viewportWidth = typeof window === "undefined" ? 1024 : window.innerWidth;
  const viewportHeight = typeof window === "undefined" ? 768 : window.innerHeight;
  const compactViewport = viewportWidth < 640;
  const paletteWidth = Math.min(columnsOnly ? 512 : 384, Math.max(0, viewportWidth - 16));
  const paletteHeight = compactViewport
    ? Math.min(560, Math.max(0, viewportHeight - 32))
    : Math.min(420, Math.max(0, viewportHeight - 16));
  const clampedPosition = {
    top: Math.max(8, Math.min(position.top, viewportHeight - paletteHeight - 8)),
    left: Math.max(8, Math.min(position.left, viewportWidth - paletteWidth - 8)),
  };
  const closeAndRestoreFocus = useOverlayFocusLifecycle({
    open: searchable,
    onClose,
    containerRef: paletteRef,
    triggerRef,
  });

  const renderCommand = ({ command, index }: { command: SlashCommand; index: number }) => (
    <button
      key={command.id}
      type="button"
      id={getSlashCommandOptionId(command.id)}
      role="option"
      aria-selected={index === selectedIndex}
      aria-label={
        command.aliases?.length
          ? `${command.label}. ${command.description}. Aliases: ${command.aliases.join(", ")}.`
          : `${command.label}. ${command.description}.`
      }
      data-command-index={index}
      onClick={() => onSelect(command)}
      className={`rounded-button flex w-full items-center gap-3 px-3 py-2 text-left transition-colors duration-100 ${
        index === selectedIndex
          ? "bg-surface text-text"
          : "text-text-muted hover:bg-surface hover:text-text"
      }`}
    >
      <span className="text-text-muted flex h-8 w-12 shrink-0 items-center justify-center">
        {command.columnLayout ? (
          <ColumnLayoutPreview layout={command.columnLayout} />
        ) : (
          command.icon
        )}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm">{command.label}</span>
        <span className="text-text-muted block truncate text-xs">{command.description}</span>
      </span>
    </button>
  );

  // Reset selection when filter changes
  useEffect(() => {
    setSelectedIndex(0);
  }, [activeQuery]);

  // Arrow/Enter must arbitrate before ProseMirror's keymap sees the event.
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      const palette = paletteRef.current;
      const targetInPalette = Boolean(target && palette?.contains(target));
      const targetInTrigger = Boolean(
        target && !searchable && triggerRef?.current?.contains(target),
      );
      const targetInOtherDialog = Boolean(target?.closest('[role="dialog"]') && !targetInPalette);
      const action = getSlashCommandPaletteScopedKeyboardAction(event.key, event.defaultPrevented, {
        searchable,
        targetInPalette,
        targetInTrigger,
        targetInOtherDialog,
      });
      if (action !== "next" && action !== "previous" && action !== "select") return;

      event.preventDefault();
      event.stopPropagation();
      if (action === "next") {
        setSelectedIndex((index) => Math.min(index + 1, filtered.length - 1));
      } else if (action === "previous") {
        setSelectedIndex((index) => Math.max(index - 1, 0));
      } else if (filtered[selectedIndex]) {
        onSelect(filtered[selectedIndex]);
      }
    };

    document.addEventListener("keydown", handler, true);
    return () => document.removeEventListener("keydown", handler, true);
  }, [filtered, onSelect, searchable, selectedIndex, triggerRef]);

  // Escape stays with the overlay lifecycle / existing editor behavior.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target instanceof Element ? e.target : null;
      const targetInPalette = Boolean(target && paletteRef.current?.contains(target));
      const targetInTrigger = Boolean(
        target && !searchable && triggerRef?.current?.contains(target),
      );
      const targetInOtherDialog = Boolean(target?.closest('[role="dialog"]') && !targetInPalette);
      const action = getSlashCommandPaletteScopedKeyboardAction(e.key, e.defaultPrevented, {
        searchable,
        targetInPalette,
        targetInTrigger,
        targetInOtherDialog,
      });
      if (action === "close") {
        e.preventDefault();
        if (searchable) {
          closeAndRestoreFocus();
        } else {
          onClose();
        }
      }
    };

    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [closeAndRestoreFocus, onClose, searchable, triggerRef]);

  // Scroll selected item into view
  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-command-index="${selectedIndex}"]`,
    );
    el?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  return (
    <>
      <button
        type="button"
        aria-label="Close command menu"
        className={`fixed inset-0 z-[75] cursor-default ${compactViewport ? "bg-black/40" : "bg-transparent"}`}
        onMouseDown={(event) => event.preventDefault()}
        onClick={searchable ? closeAndRestoreFocus : onClose}
      />
      <div
        ref={paletteRef}
        className={`starlane-menu-surface fixed z-[80] overflow-hidden p-0 ${
          compactViewport
            ? "top-1/2 left-1/2 max-h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] max-w-lg -translate-x-1/2 -translate-y-1/2"
            : columnsOnly
              ? "w-[min(32rem,calc(100vw-1rem))]"
              : "w-[min(24rem,calc(100vw-1rem))]"
        }`}
        style={
          compactViewport ? undefined : { top: clampedPosition.top, left: clampedPosition.left }
        }
        role="dialog"
        aria-modal={searchable ? "true" : undefined}
        aria-label="Command menu"
        tabIndex={-1}
      >
        <div className="border-border border-b px-3 py-1">
          {searchable ? (
            <SearchInput
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              autoFocus
              placeholder="Search commands"
              aria-label="Search commands"
              aria-controls="slash-command-options"
              aria-activedescendant={
                activeCommand ? getSlashCommandOptionId(activeCommand.id) : undefined
              }
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
          id="slash-command-options"
          role="listbox"
          aria-label="Available commands"
          className={`${compactViewport ? "max-h-[min(26rem,calc(100dvh-10rem))]" : "max-h-80"} scrollbar-none overflow-y-auto p-1`}
        >
          {filtered.length > 0 ? (
            groupedCommands.map(({ group, commands }) => {
              const columnCommands = commands.filter(({ command }) => command.columnLayout);
              const firstColumnCommandIndex = commands.findIndex(
                ({ command }) => command.columnLayout,
              );
              return (
                <div key={group} role="group" aria-label={group} className="py-1">
                  <div className="text-text-muted px-3 pt-1 pb-1 text-xs font-medium">{group}</div>
                  {commands.map((entry, entryIndex) => {
                    if (!entry.command.columnLayout) return renderCommand(entry);
                    if (entryIndex !== firstColumnCommandIndex) return null;
                    return (
                      <div key="column-layouts">
                        <div className="text-text-muted px-3 pt-1 pb-1 text-[11px] font-medium">
                          Column layouts
                        </div>
                        <div className={columnsOnly ? "grid grid-cols-1 gap-1 sm:grid-cols-2" : ""}>
                          {columnCommands.map(renderCommand)}
                        </div>
                      </div>
                    );
                  })}
                </div>
              );
            })
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
