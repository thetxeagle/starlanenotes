import { Node as TiptapNode } from "@tiptap/core";
import {
  NodeViewContent,
  NodeViewWrapper,
  ReactNodeViewRenderer,
  type Editor,
  type ReactNodeViewProps,
} from "@tiptap/react";
import { TextSelection } from "@tiptap/pm/state";
import {
  ArrowDown,
  ArrowUp,
  BetweenVerticalEnd,
  BetweenVerticalStart,
  Columns2,
  Columns3,
  PanelLeft,
  PanelRight,
  Rows3,
  Trash2,
} from "lucide-react";
import { useRef, useState, type CSSProperties } from "react";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_ACTIVE_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
} from "@/components/ui/control-chrome";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { Input } from "@/components/ui/input";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import {
  SECTION_COLUMN_MARKER,
  SECTION_COLUMN_MARKER_PREFIX,
  SECTION_END_MARKER,
  SECTION_START_MARKER,
  COLUMNS_END_MARKER,
  COLUMNS_START_MARKER,
} from "@/lib/documentContent";

export {
  COLUMNS_END_MARKER,
  COLUMNS_START_MARKER,
  SECTION_COLUMN_MARKER,
  SECTION_COLUMN_MARKER_PREFIX,
  SECTION_END_MARKER,
  SECTION_START_MARKER,
};

export const SECTION_LAYOUTS = [
  "single",
  "two-equal",
  "three-equal",
  "wide-left",
  "wide-right",
] as const;

export type SectionLayout = (typeof SECTION_LAYOUTS)[number];
export type EditorContainerKind = "section" | "columns";
export interface SectionStyle {
  borderColor?: string;
  backgroundColor?: string;
  contentBackgroundColor?: string;
  headerBackgroundColor?: string;
  headerText?: string;
}

/** Convert a visible section label into a stable, CSS-safe selector token. */
export function getSectionLabelSlug(value: unknown): string | undefined {
  const label = typeof value === "string" ? value.trim().toLocaleLowerCase() : "";
  if (!label) return undefined;
  const slug = label
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || undefined;
}

export function normalizeSectionColor(value: unknown): string | undefined {
  const color = typeof value === "string" ? value.trim() : "";
  return /^#[0-9a-f]{6}$/i.test(color) ? color.toLowerCase() : undefined;
}

export const SECTION_LAYOUT_OPTIONS: ReadonlyArray<{
  id: SectionLayout;
  label: string;
  description: string;
  columns: number;
}> = [
  {
    id: "single",
    label: "No columns",
    description: "One full-width content lane",
    columns: 1,
  },
  {
    id: "two-equal",
    label: "Two equal columns",
    description: "Two balanced content lanes",
    columns: 2,
  },
  {
    id: "three-equal",
    label: "Three equal columns",
    description: "Three balanced content lanes",
    columns: 3,
  },
  {
    id: "wide-left",
    label: "Wide left",
    description: "A larger left lane and smaller right lane",
    columns: 2,
  },
  {
    id: "wide-right",
    label: "Wide right",
    description: "A smaller left lane and larger right lane",
    columns: 2,
  },
];

export function normalizeSectionLayout(value: unknown): SectionLayout {
  return SECTION_LAYOUTS.includes(value as SectionLayout)
    ? (value as SectionLayout)
    : "single";
}

export function getSectionColumnCount(layout: SectionLayout): number {
  return SECTION_LAYOUT_OPTIONS.find((option) => option.id === layout)?.columns ?? 1;
}

/**
 * Recover the legacy layout for section files that predate the explicit
 * `layout=` attribute. A column marker is the portable signal that a section
 * has multiple lanes, so preserve that structure instead of collapsing it to
 * the default single-column layout.
 */
export function inferSectionLayoutFromColumnCount(columnCount: number): SectionLayout {
  if (columnCount >= 3) return "three-equal";
  if (columnCount === 2) return "two-equal";
  return "single";
}

export function getSectionGridTemplateColumns(layout: SectionLayout): string {
  switch (layout) {
    case "three-equal":
      return "repeat(3, minmax(0, 1fr))";
    case "wide-left":
      return "minmax(0, 3fr) minmax(0, 2fr)";
    case "wide-right":
      return "minmax(0, 2fr) minmax(0, 3fr)";
    case "two-equal":
      return "repeat(2, minmax(0, 1fr))";
    default:
      return "minmax(0, 1fr)";
  }
}

export function getSectionStartMarker(layout: SectionLayout, style: SectionStyle = {}): string {
  const contentBackground = style.contentBackgroundColor ?? style.backgroundColor;
  const attributes = [
    `layout=${layout}`,
    style.borderColor ? `border=${style.borderColor}` : "",
    contentBackground ? `background=${contentBackground}` : "",
    style.headerBackgroundColor ? `header-bg=${style.headerBackgroundColor}` : "",
    style.headerText?.trim() ? `header=${encodeURIComponent(style.headerText.trim())}` : "",
  ].filter(Boolean).join(" ");
  return attributes ? `<!-- starlane:section ${attributes} -->` : SECTION_START_MARKER;
}

export function parseSectionStartMarker(value: string): SectionLayout | null {
  return parseSectionStartOptions(value)?.layout ?? null;
}

export function parseSectionStartOptions(value: string): { layout: SectionLayout; style: SectionStyle } | null {
  const match = value.trim().match(/^<!--\s*starlane:section(.*?)\s*-->$/i);
  if (!match) return null;
  const attributes = new Map<string, string>();
  for (const token of match[1].trim().split(/\s+/)) {
    const separator = token.indexOf("=");
    if (separator > 0) attributes.set(token.slice(0, separator).toLowerCase(), token.slice(separator + 1));
  }
  const layout = attributes.get("layout");
  const border = attributes.get("border");
  const background = attributes.get("background");
  const headerBackground = attributes.get("header-bg");
  const header = attributes.get("header");
  return match
    ? {
        layout: normalizeSectionLayout(layout),
        style: {
          borderColor: normalizeSectionColor(border),
          backgroundColor: normalizeSectionColor(background),
          contentBackgroundColor: normalizeSectionColor(background),
          headerBackgroundColor: normalizeSectionColor(headerBackground),
          headerText: header
            ? (() => {
                try {
                  return decodeURIComponent(header);
                } catch {
                  return undefined;
                }
              })()
            : undefined,
        },
      }
    : null;
}

export function getColumnsStartMarker(layout: SectionLayout): string {
  const normalized = normalizeSectionLayout(layout);
  return `<!-- starlane:columns layout=${normalized === "single" ? "two-equal" : normalized} -->`;
}

export function parseColumnsStartMarker(value: string): SectionLayout | null {
  const match = value.trim().match(/^<!--\s*starlane:columns(?:\s+layout=([\w-]+))?\s*-->$/i);
  if (!match) return null;
  const layout = normalizeSectionLayout(match[1] ?? "two-equal");
  return layout === "single" ? "two-equal" : layout;
}

export function serializeEditorColumnsMarkdown(
  content: readonly string[],
  layout: SectionLayout = "two-equal",
): string {
  const normalized = normalizeSectionLayout(layout) === "single" ? "two-equal" : layout;
  const expected = getSectionColumnCount(normalized);
  const columns = Array.from({ length: expected }, (_, index) => content[index]?.trim() ?? "");
  return `${getColumnsStartMarker(normalized)}\n\n${columns
    .map((column, index) => `${index > 0 ? `\n\n${getSectionColumnMarker(index)}\n\n` : ""}${column}`)
    .join("")}\n\n${COLUMNS_END_MARKER}`;
}

export function serializeEditorSectionMarkdown(
  content: string | readonly string[],
  layout: SectionLayout = "single",
  style: SectionStyle = {},
): string {
  const columns = (Array.isArray(content) ? content : [content]).map((column) => column.trim());
  return `${getSectionStartMarker(layout, style)}\n\n${columns
    .map((column, index) => `${index > 0 ? `\n\n${getSectionColumnMarker(index)}\n\n` : ""}${column}`)
    .join("")}\n\n${SECTION_END_MARKER}`;
}

export function getSectionColumnMarker(index: number): string {
  return `<!-- starlane:column index=${Math.max(1, Math.floor(index) + 1)} -->`;
}

type SectionPlacement = "before" | "after";
type SectionDirection = "up" | "down";

export interface ActiveEditorSection {
  position: number;
  nodeSize: number;
  index: number;
  siblingCount: number;
  layout: SectionLayout;
  style: SectionStyle;
  kind: EditorContainerKind;
}

function resolveSectionPosition(getPos: ReactNodeViewProps["getPos"]): number | null {
  if (typeof getPos !== "function") return null;
  const position = getPos();
  return typeof position === "number" ? position : null;
}

export function getActiveEditorSection(editor: Editor): ActiveEditorSection | null {
  const { $from } = editor.state.selection;
  for (let depth = $from.depth; depth > 0; depth -= 1) {
    const node = $from.node(depth);
    if (node.type.name !== "section" && node.type.name !== "columns") continue;

    const position = $from.before(depth);
    const resolved = editor.state.doc.resolve(position);
    return {
      position,
      nodeSize: node.nodeSize,
      index: resolved.index(),
      siblingCount: resolved.parent.childCount,
      layout: normalizeSectionLayout(node.attrs.layout),
      style: {
        borderColor: normalizeSectionColor(node.attrs.borderColor),
        backgroundColor: normalizeSectionColor(node.attrs.backgroundColor),
        contentBackgroundColor: normalizeSectionColor(node.attrs.contentBackgroundColor),
        headerBackgroundColor: normalizeSectionColor(node.attrs.headerBackgroundColor),
        headerText: typeof node.attrs.headerText === "string" ? node.attrs.headerText : undefined,
      },
      kind: node.type.name,
    };
  }
  return null;
}

function focusSectionTransaction(editor: Editor, position: number) {
  const safePosition = Math.max(0, Math.min(position, editor.state.doc.content.size));
  const transaction = editor.state.tr.setSelection(
    TextSelection.near(editor.state.tr.doc.resolve(safePosition), 1),
  );
  editor.view.dispatch(transaction.scrollIntoView());
  editor.view.focus();
}

export function insertEditorSection(
  editor: Editor,
  position: number,
  kind: EditorContainerKind = "section",
): boolean {
  const columnCount = kind === "columns" ? 2 : 1;
  const columns = Array.from({ length: columnCount }, () =>
    editor.schema.nodes.sectionColumn?.createAndFill(),
  ).filter((column): column is NonNullable<typeof column> => Boolean(column));
  const section = columns.length === columnCount
    ? editor.schema.nodes[kind]?.create(
        { layout: kind === "columns" ? "two-equal" : "single" },
        columns,
      )
    : null;
  if (!section) return false;

  const safePosition = Math.max(0, Math.min(position, editor.state.doc.content.size));
  const transaction = editor.state.tr.insert(safePosition, section);
  editor.view.dispatch(
    transaction
      .setSelection(TextSelection.near(transaction.doc.resolve(safePosition + 3), 1))
      .scrollIntoView(),
  );
  editor.view.focus();
  return true;
}

function createSectionColumn(editor: Editor, content?: readonly import("@tiptap/pm/model").Node[]) {
  const columnType = editor.schema.nodes.sectionColumn;
  if (!columnType) return null;
  if (content && content.length > 0) return columnType.create(null, content);
  return columnType.createAndFill();
}

function getSectionColumnBlocks(
  columns: readonly import("@tiptap/pm/model").Node[],
): import("@tiptap/pm/model").Node[] {
  return columns.flatMap((column) =>
    column.type.name === "sectionColumn"
      ? Array.from({ length: column.childCount }, (_, index) => column.child(index))
      : [column],
  );
}

function distributeSectionBlocks(
  editor: Editor,
  blocks: readonly import("@tiptap/pm/model").Node[],
  columnCount: number,
): import("@tiptap/pm/model").Node[] {
  const meaningfulBlocks = blocks.filter(
    (block) => !(block.type.name === "paragraph" && block.content.size === 0),
  );
  if (meaningfulBlocks.length === 0) {
    return Array.from({ length: columnCount }, () => createSectionColumn(editor)).filter(
      (column): column is import("@tiptap/pm/model").Node => Boolean(column),
    );
  }

  const baseSize = Math.floor(meaningfulBlocks.length / columnCount);
  const remainder = meaningfulBlocks.length % columnCount;
  let offset = 0;
  return Array.from({ length: columnCount }, (_, index) => {
    const size = baseSize + (index < remainder ? 1 : 0);
    const column = createSectionColumn(editor, meaningfulBlocks.slice(offset, offset + size));
    offset += size;
    return column;
  }).filter((column): column is import("@tiptap/pm/model").Node => Boolean(column));
}

export function setEditorSectionLayout(
  editor: Editor,
  getPos: ReactNodeViewProps["getPos"],
  layout: SectionLayout,
): boolean {
  const position = resolveSectionPosition(getPos);
  if (position == null) return false;

  const section = editor.state.doc.nodeAt(position);
  if (!section || (section.type.name !== "section" && section.type.name !== "columns")) return false;
  if (section.type.name === "columns" && layout === "single") return false;

  const sectionChildren = Array.from({ length: section.childCount }, (_, index) =>
    section.child(index),
  );
  const currentColumns =
    sectionChildren.length > 0 &&
    sectionChildren.every((child) => child.type.name === "sectionColumn")
      ? sectionChildren
      : [createSectionColumn(editor, sectionChildren)].filter(
          (column): column is NonNullable<typeof column> => Boolean(column),
        );
  const targetCount = getSectionColumnCount(layout);
  const nextColumns =
    currentColumns.length === 1 && targetCount > 1
      ? distributeSectionBlocks(editor, getSectionColumnBlocks(currentColumns), targetCount)
      : currentColumns.slice(0, targetCount);

  if (currentColumns.length > targetCount) {
    const mergedBlocks = currentColumns
      .slice(targetCount - 1)
      .flatMap((column) =>
        column.type.name === "sectionColumn"
          ? Array.from({ length: column.childCount }, (_, index) => column.child(index))
          : [column],
      );
    const mergedColumn = createSectionColumn(editor, mergedBlocks);
    if (!mergedColumn) return false;
    nextColumns[targetCount - 1] = mergedColumn;
  }

  while (nextColumns.length < targetCount) {
    const column = createSectionColumn(editor);
    if (!column) return false;
    nextColumns.push(column);
  }

  const nextSection = section.type.create(
    { ...section.attrs, layout },
    nextColumns,
    section.marks,
  );
  const transaction = editor.state.tr.replaceWith(
    position,
    position + section.nodeSize,
    nextSection,
  );
  editor.view.dispatch(
    transaction
      .setSelection(TextSelection.near(transaction.doc.resolve(position + 3), 1))
      .scrollIntoView(),
  );
  editor.view.focus();
  return true;
}

export function setEditorSectionStyle(
  editor: Editor,
  getPos: ReactNodeViewProps["getPos"],
  style: SectionStyle,
): boolean {
  const position = resolveSectionPosition(getPos);
  if (position == null) return false;
  const section = editor.state.doc.nodeAt(position);
  if (!section || section.type.name !== "section") return false;
  const transaction = editor.state.tr.setNodeMarkup(position, undefined, {
    ...section.attrs,
    borderColor: normalizeSectionColor(style.borderColor) ?? null,
    backgroundColor: normalizeSectionColor(style.backgroundColor) ?? null,
    contentBackgroundColor: normalizeSectionColor(style.contentBackgroundColor ?? style.backgroundColor) ?? null,
    headerBackgroundColor: normalizeSectionColor(style.headerBackgroundColor) ?? null,
    headerText: style.headerText?.trim() || null,
  });
  editor.view.dispatch(transaction);
  return true;
}

function insertSiblingSection(
  editor: Editor,
  getPos: ReactNodeViewProps["getPos"],
  nodeSize: number,
  placement: SectionPlacement,
): boolean {
  const position = resolveSectionPosition(getPos);
  if (position == null) return false;
  const node = editor.state.doc.nodeAt(position);
  const kind = node?.type.name === "columns" ? "columns" : "section";
  return insertEditorSection(
    editor,
    placement === "before" ? position : position + nodeSize,
    kind,
  );
}

function addSectionBlock(
  editor: Editor,
  getPos: ReactNodeViewProps["getPos"],
  nodeSize: number,
): boolean {
  const position = resolveSectionPosition(getPos);
  const paragraph = editor.schema.nodes.paragraph?.createAndFill();
  if (position == null || !paragraph) return false;

  const insertAt = position + nodeSize - 2;
  const transaction = editor.state.tr.insert(insertAt, paragraph);
  editor.view.dispatch(
    transaction
      .setSelection(TextSelection.near(transaction.doc.resolve(insertAt + 1), 1))
      .scrollIntoView(),
  );
  editor.view.focus();
  return true;
}

function moveSection(
  editor: Editor,
  getPos: ReactNodeViewProps["getPos"],
  direction: SectionDirection,
): boolean {
  const position = resolveSectionPosition(getPos);
  if (position == null) return false;

  const section = editor.state.doc.nodeAt(position);
  const resolved = editor.state.doc.resolve(position);
  const parent = resolved.parent;
  const index = resolved.index();
  if (!section || (section.type.name !== "section" && section.type.name !== "columns")) return false;

  if (direction === "up") {
    if (index === 0) return false;
    const previous = parent.child(index - 1);
    const destination = position - previous.nodeSize;
    const transaction = editor.state.tr
      .delete(position, position + section.nodeSize)
      .insert(destination, section);
    editor.view.dispatch(
      transaction
        .setSelection(TextSelection.near(transaction.doc.resolve(destination + 2), 1))
        .scrollIntoView(),
    );
    editor.view.focus();
    return true;
  }

  if (index >= parent.childCount - 1) return false;
  const next = parent.child(index + 1);
  const destination = position + next.nodeSize;
  const transaction = editor.state.tr
    .delete(position, position + section.nodeSize)
    .insert(destination, section);
  editor.view.dispatch(
    transaction
      .setSelection(TextSelection.near(transaction.doc.resolve(destination + 2), 1))
      .scrollIntoView(),
  );
  editor.view.focus();
  return true;
}

function deleteSection(
  editor: Editor,
  getPos: ReactNodeViewProps["getPos"],
  nodeSize: number,
): boolean {
  const position = resolveSectionPosition(getPos);
  if (position == null) return false;

  const transaction =
    editor.state.doc.childCount === 1
      ? editor.state.tr.replaceWith(
          position,
          position + nodeSize,
          editor.schema.nodes.section.create(
            { layout: "single" },
            editor.schema.nodes.sectionColumn.create(
              null,
              editor.schema.nodes.paragraph.create(),
            ),
          ),
        )
      : editor.state.tr.delete(position, position + nodeSize);
  editor.view.dispatch(transaction.scrollIntoView());
  focusSectionTransaction(editor, Math.min(position + 1, transaction.doc.content.size));
  return true;
}

export function insertActiveEditorSection(
  editor: Editor,
  section: ActiveEditorSection,
  placement: SectionPlacement,
): boolean {
  return insertSiblingSection(
    editor,
    () => section.position,
    section.nodeSize,
    placement,
  );
}

export function moveActiveEditorSection(
  editor: Editor,
  section: ActiveEditorSection,
  direction: SectionDirection,
): boolean {
  return moveSection(editor, () => section.position, direction);
}

export function addBlockToActiveEditorSection(
  editor: Editor,
  section: ActiveEditorSection,
): boolean {
  return addSectionBlock(editor, () => section.position, section.nodeSize);
}

export function deleteActiveEditorSection(
  editor: Editor,
  section: ActiveEditorSection,
): boolean {
  return deleteSection(editor, () => section.position, section.nodeSize);
}

export function setActiveEditorSectionLayout(
  editor: Editor,
  section: ActiveEditorSection,
  layout: SectionLayout,
): boolean {
  return setEditorSectionLayout(editor, () => section.position, layout);
}

export function setActiveEditorSectionStyle(
  editor: Editor,
  section: ActiveEditorSection,
  style: SectionStyle,
): boolean {
  return setEditorSectionStyle(editor, () => section.position, style);
}

function SectionAction({
  danger = false,
  disabled = false,
  icon,
  label,
  onRun,
}: {
  danger?: boolean;
  disabled?: boolean;
  icon: React.ReactNode;
  label: string;
  onRun: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          disabled={disabled}
          onPointerDown={(event) => {
            event.preventDefault();
            event.stopPropagation();
          }}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onRun();
          }}
          className={`${CONTROL_CHROME_ACTION_CLASS} editor-section__action flex items-center justify-center ${
            danger ? "text-danger hover:bg-danger/10 hover:text-danger" : ""
          }`}
        >
          {icon}
        </button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function ColumnLayoutPreview({ layout }: { layout: SectionLayout }) {
  const columns = getSectionColumnCount(layout);
  return (
    <span
      aria-hidden="true"
      data-layout-preview={layout}
      className="editor-section-layout-preview"
    >
      {Array.from({ length: columns }, (_, index) => (
        <span key={index} />
      ))}
    </span>
  );
}

function SectionLayoutAction({
  editor,
  getPos,
  layout,
  columnsBlock,
}: {
  editor: Editor;
  getPos: ReactNodeViewProps["getPos"];
  layout: SectionLayout;
  columnsBlock?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const layoutMenuRef = useRef<HTMLDivElement | null>(null);
  const handleLayoutMenuKeyDown = useMenuKeyboardNavigation(layoutMenuRef);
  const section = editor.state.doc.nodeAt(resolveSectionPosition(getPos) ?? -1);
  const storedBorderColor = normalizeSectionColor(section?.attrs.borderColor);
  const storedBackgroundColor = normalizeSectionColor(
    section?.attrs.contentBackgroundColor ?? section?.attrs.backgroundColor,
  );
  const storedHeaderBackgroundColor = normalizeSectionColor(section?.attrs.headerBackgroundColor);
  const storedHeaderText = typeof section?.attrs.headerText === "string" ? section.attrs.headerText : "";
  const borderColor = storedBorderColor ?? "#334155";
  const backgroundColor = storedBackgroundColor ?? "#111827";
  const headerBackgroundColor = storedHeaderBackgroundColor ?? "#1f2937";
  const current = SECTION_LAYOUT_OPTIONS.find((option) => option.id === layout);
  const layoutOptions = columnsBlock
    ? SECTION_LAYOUT_OPTIONS.filter((option) => option.id !== "single")
    : SECTION_LAYOUT_OPTIONS;
  const icon =
    layout === "three-equal" ? (
      <Columns3 size={16} />
    ) : layout === "wide-left" ? (
      <PanelLeft size={16} />
    ) : layout === "wide-right" ? (
      <PanelRight size={16} />
    ) : layout === "two-equal" ? (
      <Columns2 size={16} />
    ) : (
      <Rows3 size={16} />
    );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <button
              type="button"
              aria-label={`${columnsBlock ? "Column block" : "Section"} layout: ${current?.label ?? "No columns"}`}
              className={`${CONTROL_CHROME_ACTION_CLASS} editor-section__action flex items-center justify-center ${
                layout !== "single" ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : ""
              }`}
              onPointerDown={(event) => event.stopPropagation()}
              onClick={(event) => event.stopPropagation()}
            >
              {icon}
            </button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent>{columnsBlock ? "Column block layout" : "Section layout"}</TooltipContent>
      </Tooltip>
      <PopoverContent
        align="end"
        sideOffset={8}
        className="w-[min(22rem,calc(100vw-2rem))] p-0"
        aria-label={`Choose ${columnsBlock ? "column block" : "section"} layout`}
        onPointerDown={(event) => event.stopPropagation()}
      >
        <div className="border-border border-b px-3 py-2.5">
          <h2 className="text-sm font-semibold">
            {columnsBlock ? "Column block layout" : "Section layout"}
          </h2>
          <p className="text-text-muted mt-0.5 text-xs">Choose up to three content columns.</p>
        </div>
        <div
          ref={layoutMenuRef}
          className="grid gap-1 p-2"
          role="menu"
          aria-label={columnsBlock ? "Column block layouts" : "Section layouts"}
          onKeyDown={handleLayoutMenuKeyDown}
        >
          {layoutOptions.map((option) => {
            const selected = option.id === layout;
            return (
              <button
                key={option.id}
                type="button"
                role="menuitemradio"
                aria-checked={selected}
                className={`rounded-button focus-ring flex min-h-12 items-center gap-3 border px-3 py-2 text-left transition-colors duration-100 ${
                  selected
                    ? "border-primary bg-primary/10 text-text"
                    : "border-transparent text-text hover:border-border hover:bg-surface-raised"
                }`}
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  setEditorSectionLayout(editor, getPos, option.id);
                  setOpen(false);
                }}
              >
                <ColumnLayoutPreview layout={option.id} />
                <span className="min-w-0">
                  <span className="block text-sm font-medium">{option.label}</span>
                  <span className="text-text-muted block text-xs">{option.description}</span>
                </span>
              </button>
            );
          })}
        </div>
        {!columnsBlock && <div className="border-border grid gap-2 border-t p-3">
          <p className="text-text-muted text-xs font-medium">Section styling</p>
          <label className="text-text-muted flex items-center justify-between gap-3 text-xs">
            Border color
            <input
              type="color"
              value={borderColor}
              aria-label="Section border color"
              onChange={(event) =>
                setEditorSectionStyle(editor, getPos, {
                  borderColor: event.target.value,
                  backgroundColor: storedBackgroundColor,
                  contentBackgroundColor: storedBackgroundColor,
                  headerBackgroundColor: storedHeaderBackgroundColor,
                  headerText: storedHeaderText,
                })
              }
            />
          </label>
          <label className="text-text-muted flex items-center justify-between gap-3 text-xs">
            Background color
            <input
              type="color"
              value={backgroundColor}
              aria-label="Section background color"
              onChange={(event) =>
                setEditorSectionStyle(editor, getPos, {
                  borderColor: storedBorderColor,
                  backgroundColor: event.target.value,
                  contentBackgroundColor: event.target.value,
                  headerBackgroundColor: storedHeaderBackgroundColor,
                  headerText: storedHeaderText,
                })
              }
            />
          </label>
          <label className="text-text-muted flex items-center justify-between gap-3 text-xs">
            Header background
            <input
              type="color"
              value={headerBackgroundColor}
              aria-label="Section header background color"
              onChange={(event) =>
                setEditorSectionStyle(editor, getPos, {
                  borderColor: storedBorderColor,
                  contentBackgroundColor: storedBackgroundColor,
                  headerBackgroundColor: event.target.value,
                  headerText: storedHeaderText,
                })
              }
            />
          </label>
          <label className="grid gap-1 text-xs">
            <span className="text-text-muted">Header text</span>
            <Input
              type="text"
              value={storedHeaderText}
              placeholder="Optional section header"
              aria-label="Section header text"
              className="h-8 bg-bg px-2 text-xs"
              onChange={(event) =>
                setEditorSectionStyle(editor, getPos, {
                  borderColor: storedBorderColor,
                  contentBackgroundColor: storedBackgroundColor,
                  headerBackgroundColor: storedHeaderBackgroundColor,
                  headerText: event.target.value,
                })
              }
            />
          </label>
          <button
            type="button"
            className="rounded-button border-border text-text-muted hover:bg-surface-raised border px-2 py-1 text-xs"
            onClick={() => setEditorSectionStyle(editor, getPos, {})}
          >
            Reset section colors
          </button>
        </div>}
      </PopoverContent>
    </Popover>
  );
}

function EditorSectionNodeView({ editor, getPos, node }: ReactNodeViewProps) {
  const position = resolveSectionPosition(getPos);
  const resolved = position == null ? null : editor.state.doc.resolve(position);
  const index = resolved?.index() ?? 0;
  const siblingCount = resolved?.parent.childCount ?? 1;
  const editable = editor.isEditable;
  const columnsBlock = node.type.name === "columns";
  const containerLabel = columnsBlock ? "column block" : "section";
  const layout = normalizeSectionLayout(node.attrs.layout);
  const layoutLabel = SECTION_LAYOUT_OPTIONS.find((option) => option.id === layout)?.label ?? layout;
  const borderColor = normalizeSectionColor(node.attrs.borderColor);
  const backgroundColor = normalizeSectionColor(
    node.attrs.contentBackgroundColor ?? node.attrs.backgroundColor,
  );
  const headerBackgroundColor = normalizeSectionColor(node.attrs.headerBackgroundColor);
  const headerText = typeof node.attrs.headerText === "string" ? node.attrs.headerText : "";
  const plainWritingSection =
    layout === "single" &&
    !borderColor &&
    !backgroundColor &&
    !headerBackgroundColor &&
    !headerText.trim();

  return (
    <NodeViewWrapper
      as="section"
      data-starlane-section={columnsBlock ? undefined : "true"}
      data-starlane-columns={columnsBlock ? "true" : undefined}
      data-layout={layout}
      data-plain-writing-section={plainWritingSection ? "true" : undefined}
      data-border-color={borderColor}
      data-content-background-color={backgroundColor}
      data-header-background-color={headerBackgroundColor}
      data-section-label={headerText || undefined}
      data-section-label-slug={getSectionLabelSlug(headerText)}
      className="editor-section"
      style={{
        display: "grid",
        gridTemplateColumns: "var(--section-grid-template-columns, minmax(0, 1fr))",
        ...(borderColor ? { borderColor } : {}),
        ...(backgroundColor ? { backgroundColor } : {}),
        "--section-grid-template-columns": getSectionGridTemplateColumns(layout),
      } as CSSProperties}
    >
      {editable && (
        <TooltipProvider delayDuration={300}>
          <div
            contentEditable={false}
            role="toolbar"
            aria-label={`${columnsBlock ? "Column block" : "Section"} controls`}
            data-section-controls="true"
            className={`${CONTROL_CHROME_RAIL_CLASS} editor-section__toolbar`}
          >
            <SectionAction
              label={`Add ${containerLabel} above`}
              icon={<BetweenVerticalStart size={16} />}
              onRun={() => void insertSiblingSection(editor, getPos, node.nodeSize, "before")}
            />
            <SectionAction
              label={`Move ${containerLabel} up`}
              disabled={index === 0}
              icon={<ArrowUp size={16} />}
              onRun={() => void moveSection(editor, getPos, "up")}
            />
            <SectionLayoutAction
              editor={editor}
              getPos={getPos}
              layout={layout}
              columnsBlock={columnsBlock}
            />
            <SectionAction
              label={`Move ${containerLabel} down`}
              disabled={index >= siblingCount - 1}
              icon={<ArrowDown size={16} />}
              onRun={() => void moveSection(editor, getPos, "down")}
            />
            <SectionAction
              label={`Add ${containerLabel} below`}
              icon={<BetweenVerticalEnd size={16} />}
              onRun={() => void insertSiblingSection(editor, getPos, node.nodeSize, "after")}
            />
            <SectionAction
              danger
              label={`Delete ${containerLabel}`}
              icon={<Trash2 size={16} />}
              onRun={() => void deleteSection(editor, getPos, node.nodeSize)}
            />
          </div>
        </TooltipProvider>
      )}
      {headerText && (
        <div
          contentEditable={false}
          className="editor-section__header"
          style={headerBackgroundColor ? { backgroundColor: headerBackgroundColor } : undefined}
        >
          {headerText}
        </div>
      )}
      <NodeViewContent
        className="editor-section__content"
        data-layout={layout}
        aria-label={`${layoutLabel} ${containerLabel} content`}
        style={{
          // The section owns the grid. Keeping this content mount transparent
          // makes each sectionColumn a real grid item in the editor DOM.
          display: "contents",
          "--section-grid-template-columns": getSectionGridTemplateColumns(layout),
        } as CSSProperties}
      />
    </NodeViewWrapper>
  );
}

export const EditorSection = TiptapNode.create({
  name: "section",
  group: "block",
  content: "sectionColumn{1,3}",
  selectable: false,
  defining: true,
  isolating: true,
  addAttributes() {
    return {
      layout: {
        default: "single",
        parseHTML: (element) => normalizeSectionLayout(element.getAttribute("data-layout")),
        renderHTML: (attributes) => ({
          "data-layout": normalizeSectionLayout(attributes.layout),
        }),
      },
      borderColor: {
        default: null,
        parseHTML: (element) => normalizeSectionColor(element.getAttribute("data-border-color")),
        renderHTML: (attributes) => {
          const value = normalizeSectionColor(attributes.borderColor);
          return value ? { "data-border-color": value } : {};
        },
      },
      backgroundColor: {
        default: null,
        parseHTML: (element) => normalizeSectionColor(element.getAttribute("data-background-color")),
        renderHTML: (attributes) => {
          const value = normalizeSectionColor(attributes.backgroundColor);
          return value ? { "data-background-color": value } : {};
        },
      },
      contentBackgroundColor: {
        default: null,
        parseHTML: (element) => normalizeSectionColor(element.getAttribute("data-content-background-color")),
        renderHTML: (attributes) => {
          const value = normalizeSectionColor(attributes.contentBackgroundColor);
          return value ? { "data-content-background-color": value } : {};
        },
      },
      headerBackgroundColor: {
        default: null,
        parseHTML: (element) => normalizeSectionColor(element.getAttribute("data-header-background-color")),
        renderHTML: (attributes) => {
          const value = normalizeSectionColor(attributes.headerBackgroundColor);
          return value ? { "data-header-background-color": value } : {};
        },
      },
      headerText: {
        default: null,
        parseHTML: (element) =>
          element.getAttribute("data-section-label") || element.getAttribute("data-header-text") || null,
        renderHTML: (attributes) =>
          typeof attributes.headerText === "string" && attributes.headerText.trim()
            ? {
                "data-header-text": attributes.headerText.trim(),
                "data-section-label": attributes.headerText.trim(),
                "data-section-label-slug": getSectionLabelSlug(attributes.headerText),
              }
            : {},
      },
    };
  },
  parseHTML() {
    return [{ tag: "section[data-starlane-section]" }];
  },
  renderHTML({ HTMLAttributes }) {
    // `addAttributes().renderHTML` emits portable `data-*` attributes. Do not
    // read only the internal attribute names here, or serialization will
    // overwrite every explicit layout with the single-column default.
    const layout = normalizeSectionLayout(
      HTMLAttributes["data-layout"] ?? HTMLAttributes.layout,
    );
    return [
      "section",
      {
        ...HTMLAttributes,
        "data-starlane-section": "true",
        "data-layout": layout,
        ...(normalizeSectionColor(HTMLAttributes.borderColor) ? { "data-border-color": normalizeSectionColor(HTMLAttributes.borderColor) } : {}),
        ...(normalizeSectionColor(HTMLAttributes.backgroundColor) ? { "data-background-color": normalizeSectionColor(HTMLAttributes.backgroundColor) } : {}),
        ...(normalizeSectionColor(HTMLAttributes.contentBackgroundColor) ? { "data-content-background-color": normalizeSectionColor(HTMLAttributes.contentBackgroundColor) } : {}),
        ...(normalizeSectionColor(HTMLAttributes.headerBackgroundColor) ? { "data-header-background-color": normalizeSectionColor(HTMLAttributes.headerBackgroundColor) } : {}),
        ...(typeof HTMLAttributes.headerText === "string" && HTMLAttributes.headerText.trim() ? { "data-header-text": HTMLAttributes.headerText.trim() } : {}),
        ...(typeof HTMLAttributes.headerText === "string" && HTMLAttributes.headerText.trim()
          ? {
              "data-section-label": HTMLAttributes.headerText.trim(),
              "data-section-label-slug": getSectionLabelSlug(HTMLAttributes.headerText),
            }
          : {}),
      },
      0,
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(EditorSectionNodeView, {
      stopEvent: ({ event }) =>
        event.target instanceof Element &&
        Boolean(event.target.closest("[data-section-controls='true']")),
    });
  },
});

export const EditorColumns = TiptapNode.create({
  name: "columns",
  group: "block",
  content: "sectionColumn{2,3}",
  selectable: false,
  defining: true,
  isolating: true,
  addAttributes() {
    return {
      layout: {
        default: "two-equal",
        parseHTML: (element) => {
          const layout = normalizeSectionLayout(element.getAttribute("data-layout"));
          return layout === "single" ? "two-equal" : layout;
        },
        renderHTML: (attributes) => ({
          "data-layout":
            normalizeSectionLayout(attributes.layout) === "single"
              ? "two-equal"
              : normalizeSectionLayout(attributes.layout),
        }),
      },
    };
  },
  parseHTML() {
    return [{ tag: "section[data-starlane-columns]" }];
  },
  renderHTML({ HTMLAttributes }) {
    const normalized = normalizeSectionLayout(
      HTMLAttributes["data-layout"] ?? HTMLAttributes.layout,
    );
    const layout = normalized === "single" ? "two-equal" : normalized;
    return [
      "section",
      {
        ...HTMLAttributes,
        "data-starlane-columns": "true",
        "data-layout": layout,
      },
      0,
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(EditorSectionNodeView, {
      stopEvent: ({ event }) =>
        event.target instanceof Element &&
        Boolean(event.target.closest("[data-section-controls='true']")),
    });
  },
});

export const EditorSectionColumn = TiptapNode.create({
  name: "sectionColumn",
  content: "block+",
  selectable: false,
  defining: true,
  isolating: true,
  parseHTML() {
    return [{ tag: "div[data-starlane-section-column]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      {
        ...HTMLAttributes,
        "data-starlane-section-column": "true",
        class: "editor-section__column",
      },
      0,
    ];
  },
});
