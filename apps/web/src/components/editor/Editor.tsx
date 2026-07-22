import {
  lazy,
  Suspense,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ComponentType,
} from "react";
import { useEditor, EditorContent, type Editor as TipTapEditor } from "@tiptap/react";
import { getMarkRange, Mark, mergeAttributes, Node as TiptapNode } from "@tiptap/core";
import { DOMSerializer, type Node as ProseMirrorNode } from "prosemirror-model";
import { EditorZoomContext } from "@/components/layout/AppLayout";
import { useAuth } from "@/hooks/useAuth";
import {
  formatSmartDate,
  SlashCommandPalette,
  type SlashCommand,
  type SlashTemplate,
} from "@/components/editor/SlashCommands";
import { Input } from "@/components/ui/input";
import type { TemplateRenderContext } from "@/lib/templates";
import { dataClient, type DocumentRecord, type LocalAssetContext, type SpaceMemberRecord } from "@/lib/dataClient";
import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Link from "@tiptap/extension-link";
import ImageExt from "@tiptap/extension-image";
import { Table } from "@tiptap/extension-table";
import TableRow from "@tiptap/extension-table-row";
import TableHeader from "@tiptap/extension-table-header";
import TableCell from "@tiptap/extension-table-cell";
import Highlight from "@tiptap/extension-highlight";
import Typography from "@tiptap/extension-typography";
import Underline from "@tiptap/extension-underline";
import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight";
import { common, createLowlight } from "lowlight";
import bash from "highlight.js/lib/languages/bash";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import powershell from "highlight.js/lib/languages/powershell";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import yaml from "highlight.js/lib/languages/yaml";

interface ExcalidrawComponentProps {
  initialData?: unknown;
  onChange?: (elements: readonly unknown[], appState: unknown, files: unknown) => void;
}

const LazyExcalidraw = lazy(async (): Promise<{ default: ComponentType<ExcalidrawComponentProps> }> => {
  const module = await import("@excalidraw/excalidraw");
  return { default: module.Excalidraw as ComponentType<ExcalidrawComponentProps> };
});

const BookmarkCard = TiptapNode.create({
  name: "bookmarkCard",
  group: "block",
  atom: true,
  selectable: true,
  addAttributes() {
    return {
      href: { default: "" },
      title: { default: "Bookmark" },
      description: { default: "" },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-bookmark-card]" }];
  },
  renderHTML({ HTMLAttributes }) {
    const href = String(HTMLAttributes.href || "");
    const title = String(HTMLAttributes.title || "Bookmark");
    const description = String(HTMLAttributes.description || "");
    const domain = getBookmarkDomain(href);
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-bookmark-card": "true",
        class: "my-3 rounded-card border border-border bg-surface p-4",
      }),
      [
        "a",
        { href, target: "_blank", rel: "noreferrer", class: "block text-sm font-semibold text-primary" },
        title,
      ],
      description
        ? ["p", { class: "mt-2 text-sm leading-5 text-text" }, description]
        : ["p", { class: "mt-2 break-all text-xs text-text-muted" }, href],
      ["span", { class: "mt-3 inline-flex rounded-button border border-border bg-surface-raised px-2 py-1 text-xs text-text-muted" }, domain],
    ];
  },
});

const ExcalidrawCanvas = TiptapNode.create({
  name: "excalidrawCanvas",
  group: "block",
  atom: true,
  selectable: true,
  addAttributes() {
    return {
      filename: {
        default: "untitled.excalidraw",
        parseHTML: (element) => element.getAttribute("filename") || "untitled.excalidraw",
      },
      scene: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-scene") || "",
      },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-excalidraw-canvas]" }];
  },
  renderHTML({ HTMLAttributes }) {
    const filename = String(HTMLAttributes.filename || "untitled.excalidraw");
    const scene = String(HTMLAttributes.scene || "");
    return renderExcalidrawCanvasSpec(filename, scene, HTMLAttributes);
  },
});
import {
  ArrowDown,
  ArrowUp,
  Bookmark,
  Bold,
  Check,
  Code,
  Command,
  ExternalLink,
  FileCode,
  FileText,
  GripVertical,
  Heading,
  Highlighter,
  Image as ImageIcon,
  IndentDecrease,
  IndentIncrease,
  Italic,
  List,
  ListChecks,
  ListOrdered,
  Link as LinkIcon,
  Pencil,
  Plus,
  Quote,
  Redo2,
  RemoveFormatting,
  Rows3,
  Search,
  Settings2,
  Strikethrough,
  Tag,
  Trash2,
  Undo2,
  Underline as UnderlineIcon,
  Unlink,
  Upload,
  Columns3,
  X,
} from "lucide-react";

const lowlight = createLowlight(common);
lowlight.register("bash", bash);
lowlight.register("shell", bash);
lowlight.register("sh", bash);
lowlight.register("javascript", javascript);
lowlight.register("js", javascript);
lowlight.register("json", json);
lowlight.register("markdown", markdown);
lowlight.register("md", markdown);
lowlight.register("powershell", powershell);
lowlight.register("ps1", powershell);
lowlight.register("python", python);
lowlight.register("py", python);
lowlight.register("rust", rust);
lowlight.register("rs", rust);
lowlight.register("sql", sql);
lowlight.register("typescript", typescript);
lowlight.register("ts", typescript);
lowlight.register("tsx", typescript);
lowlight.register("yaml", yaml);
lowlight.register("yml", yaml);

export const EDITOR_DANGER_MENU_ITEM_CLASS =
  "rounded-button block w-full px-2 py-1.5 text-left text-xs text-danger transition-colors duration-100 hover:bg-danger/10 disabled:opacity-40";

interface EditorProps {
  content: string;
  onChange: (content: string) => void;
  readOnly?: boolean;
  plainMode?: boolean;
  showBlockChrome?: boolean;
  showMobileCommandButton?: boolean;
  templates?: SlashTemplate[];
  templateContext?: TemplateRenderContext;
  assetContext?: LocalAssetContext;
  spaceId?: string;
  onEditorReady?: (editor: TipTapEditor) => void;
}

export { type TipTapEditor };

interface SlashState {
  query: string;
  from: number;
  to: number;
  position: { top: number; left: number };
  searchable: boolean;
}

interface BlockMenuState {
  position: { top: number; left: number };
  helperPosition: { top: number; left: number };
  open: boolean;
  canMoveUp: boolean;
  canMoveDown: boolean;
  label: string;
  isCodeBlock: boolean;
  codeLanguage: string;
}

interface ContextMenuState {
  position: { top: number; left: number };
  hasSelection: boolean;
  inTable: boolean;
  hasBlock: boolean;
  canMoveBlockUp: boolean;
  canMoveBlockDown: boolean;
}

interface FloatingMenuState {
  position: { top: number; left: number };
}

interface LinkMenuState {
  position: { top: number; left: number };
  from: number;
  to: number;
  text: string;
  href: string;
}

type LinkDialogState = LinkMenuState;

interface UploadMenuState {
  preferImage: boolean;
  position: { top: number; left: number };
}

interface ExcalidrawSceneData {
  elements: readonly unknown[];
  appState?: Record<string, unknown>;
  files?: Record<string, unknown>;
}

interface ExcalidrawDialogState {
  position: number;
  filename: string;
  scene: ExcalidrawSceneData;
  fileBacked: boolean;
}

interface TagDialogState {
  text: string;
  from: number;
  to: number;
  position: { top: number; left: number };
}

interface BookmarkDialogState {
  from: number;
  to: number;
  href: string;
  title: string;
  description: string;
  loading?: boolean;
  error?: string;
  position: { top: number; left: number };
}

interface WikiLinkDialogState {
  from: number;
  to: number;
  position: { top: number; left: number };
  documents: DocumentRecord[];
  query: string;
  loading: boolean;
  error: boolean;
}
interface MentionDialogState { from: number; to: number; position: { top: number; left: number }; members: SpaceMemberRecord[]; query: string; loading: boolean; error: boolean; }

interface TopLevelBlock {
  node: ProseMirrorNode;
  from: number;
  to: number;
  index: number;
}

type MobileToolbarActionId =
  | "undo"
  | "redo"
  | "link"
  | "bookmark"
  | "upload"
  | "tag"
  | "heading"
  | "bold"
  | "italic"
  | "underline"
  | "strike"
  | "highlight"
  | "code"
  | "codeBlock"
  | "quote"
  | "bullet"
  | "numbered"
  | "task"
  | "indent"
  | "outdent"
  | "commands";

const MOBILE_TOOLBAR_STORAGE_KEY = "starlane_mobile_editor_toolbar";
export const MOBILE_EDITOR_TOOLBAR_BASE_CLASS =
  "mobile-editor-toolbar fixed z-[35] flex items-center gap-2 transition-[bottom] will-change-[bottom]";
export const MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS =
  "left-0 right-0 bg-surface/95 px-2 py-1 shadow-card duration-0";
export const MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS =
  "left-3 right-3 transition-[bottom] duration-150 ease-out";
export const MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS =
  "scrollbar-none border-border bg-surface flex h-11 flex-1 items-center gap-1 overflow-x-auto border px-1";
export const MOBILE_EDITOR_TOOLBAR_ACTION_CLASS =
  "rounded-button flex h-9 w-9 shrink-0 items-center justify-center transition-colors duration-100";
export const MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS =
  "rounded-button border-border bg-surface text-text-muted shadow-card active:bg-surface-raised flex h-11 w-11 shrink-0 items-center justify-center border transition-colors duration-100";

const DEFAULT_MOBILE_TOOLBAR_ACTIONS: MobileToolbarActionId[] = [
  "undo",
  "redo",
  "link",
  "bookmark",
  "upload",
  "tag",
  "heading",
  "bold",
  "italic",
  "underline",
  "strike",
  "highlight",
  "code",
  "codeBlock",
  "quote",
  "bullet",
  "numbered",
  "task",
  "indent",
  "outdent",
];

const ALL_MOBILE_TOOLBAR_ACTIONS: MobileToolbarActionId[] = [
  ...DEFAULT_MOBILE_TOOLBAR_ACTIONS,
  "commands",
];

const MOBILE_TOOLBAR_ACTION_SET = new Set<MobileToolbarActionId>(ALL_MOBILE_TOOLBAR_ACTIONS);

const CODE_LANGUAGE_OPTIONS = [
  { value: "", label: "Plain text" },
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
  { value: "yaml", label: "YAML" },
];

const EDITOR_IMAGE_EXTENSIONS = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "svg",
  "heic",
  "heif",
]);
const DEFAULT_UPLOAD_LIMIT_MB = 30;

const EDITOR_FONT_OPTIONS = [
  { value: "", label: "Default" },
  { value: "Inter, system-ui, sans-serif", label: "Inter" },
  {
    value: "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
    label: "System",
  },
  { value: "Georgia, 'Times New Roman', serif", label: "Serif" },
  { value: "'Iowan Old Style', 'Palatino Linotype', Palatino, serif", label: "Book Serif" },
  { value: "'JetBrains Mono', 'SFMono-Regular', Consolas, monospace", label: "Mono" },
  { value: "Arial, Helvetica, sans-serif", label: "Arial" },
  { value: "Verdana, Geneva, sans-serif", label: "Verdana" },
];

export interface MobileKeyboardViewportMeasurement {
  layoutHeight: number;
  visualHeight: number;
  visualTop: number;
  visualWidth: number;
}

export interface MobileKeyboardViewportState {
  baseline: number;
  width: number;
}

export function getMobileKeyboardInsetMeasurement(
  measurement: MobileKeyboardViewportMeasurement,
  previous: MobileKeyboardViewportState,
): { inset: number; state: MobileKeyboardViewportState } {
  const widthChanged =
    previous.width === 0 || Math.abs(measurement.visualWidth - previous.width) > 80;
  const baseline = Math.max(
    widthChanged ? 0 : previous.baseline,
    measurement.layoutHeight,
    measurement.visualHeight + measurement.visualTop,
  );
  const rawInset = baseline - measurement.visualHeight - measurement.visualTop;
  return {
    inset: rawInset > 80 ? Math.round(rawInset) : 0,
    state: {
      baseline,
      width: measurement.visualWidth,
    },
  };
}

export function getMobileToolbarPlacement(keyboardInset: number): {
  keyboardOpen: boolean;
  toolbarBottom: string;
  menuBottom: string;
  reason: "keyboard-overlap" | "safe-area";
} {
  if (keyboardInset > 0) {
    return {
      keyboardOpen: true,
      toolbarBottom: `${keyboardInset}px`,
      menuBottom: `calc(${keyboardInset}px + 3.25rem)`,
      reason: "keyboard-overlap",
    };
  }

  return {
    keyboardOpen: false,
    toolbarBottom: "max(0.75rem, calc(env(safe-area-inset-bottom) + 0.75rem))",
    menuBottom: "max(4.75rem, calc(env(safe-area-inset-bottom) + 4.75rem))",
    reason: "safe-area",
  };
}

const FontFamily = Mark.create({
  name: "fontFamily",

  addAttributes() {
    return {
      fontFamily: {
        default: null,
        parseHTML: (element) => element.style.fontFamily || null,
        renderHTML: (attributes) =>
          attributes.fontFamily ? { style: `font-family: ${attributes.fontFamily}` } : {},
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[style*=font-family]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return ["span", mergeAttributes(HTMLAttributes), 0];
  },
});

const ContentTag = Mark.create({
  name: "contentTag",

  addAttributes() {
    return {
      tagName: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-tag"),
        renderHTML: (attributes) =>
          attributes.tagName
            ? {
                "data-tag": attributes.tagName,
              }
            : {},
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-type='content-tag']" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-type": "content-tag",
        class: "editor-content-tag",
      }),
      0,
    ];
  },
});

function getConfiguredUploadLimitMb(serverLimitMb?: number): number {
  if (Number.isFinite(serverLimitMb) && serverLimitMb && serverLimitMb > 0) {
    return Math.min(Math.floor(serverLimitMb), 1024);
  }

  const raw = import.meta.env.VITE_MAX_UPLOAD_SIZE_MB;
  const parsed = typeof raw === "string" ? Number.parseInt(raw, 10) : Number.NaN;
  if (!Number.isFinite(parsed) || parsed <= 0) return DEFAULT_UPLOAD_LIMIT_MB;
  return Math.min(parsed, 1024);
}

export function formatEditorUploadLimitWarning(fileName: string, uploadLimitMb: number): string {
  return `${fileName} is larger than the current ${uploadLimitMb} MB upload limit.`;
}

export function EditorUploadWarning({
  message,
  onDismiss,
}: {
  message: string;
  onDismiss: () => void;
}) {
  return (
    <div
      className="mt-3 flex items-start justify-between gap-3 rounded-input border border-warning/30 bg-warning/10 px-3 py-2 text-sm text-warning"
      role="alert"
    >
      <span>{message}</span>
      <button
        type="button"
        onClick={onDismiss}
        className="rounded-input p-1 text-warning hover:bg-warning/10"
        aria-label="Dismiss upload warning"
      >
        <X size={14} />
      </button>
    </div>
  );
}

function selectionIsInLastTableRow(editor: TipTapEditor) {
  const { $from } = editor.state.selection;

  for (let depth = $from.depth; depth > 0; depth--) {
    const node = $from.node(depth);
    if (node.type.name !== "tableRow") continue;

    const table = $from.node(depth - 1);
    if (table.type.name !== "table") return false;

    const rowIndex = $from.index(depth - 1);
    return rowIndex === table.childCount - 1;
  }

  return false;
}

function getCurrentTopLevelBlock(editor: TipTapEditor): TopLevelBlock | null {
  const { $from } = editor.state.selection;
  if ($from.depth === 0) return null;

  const node = $from.node(1);
  const from = $from.before(1);
  const to = $from.after(1);
  const index = $from.index(0);

  return { node, from, to, index };
}

function getTopLevelBlockAtPos(editor: TipTapEditor, pos: number): TopLevelBlock | null {
  const childCount = editor.state.doc.childCount;
  if (childCount === 0) return null;

  let from = 0;
  const safePos = Math.max(0, Math.min(pos, editor.state.doc.content.size));

  for (let index = 0; index < childCount; index++) {
    const node = editor.state.doc.child(index);
    const to = from + node.nodeSize;
    if (safePos <= to || index === childCount - 1) {
      return { node, from, to, index };
    }
    from = to;
  }

  return null;
}

function getBlockLabel(editor: TipTapEditor): string {
  if (editor.isActive("table")) return "Table";
  if (editor.isActive("heading", { level: 1 })) return "Header 1";
  if (editor.isActive("heading", { level: 2 })) return "Header 2";
  if (editor.isActive("heading", { level: 3 })) return "Header 3";
  if (editor.isActive("heading", { level: 4 })) return "Header 4";
  if (editor.isActive("heading", { level: 5 })) return "Header 5";
  if (editor.isActive("heading", { level: 6 })) return "Header 6";
  if (editor.isActive("codeBlock")) {
    const language = editor.getAttributes("codeBlock").language || "Language";
    return `Code - ${language}`;
  }
  if (editor.isActive("blockquote")) return "Quote";
  if (editor.isActive("bulletList")) return "Bullet list";
  if (editor.isActive("orderedList")) return "Numbered list";
  if (editor.isActive("taskList")) return "Task list";
  return "¶";
}

function getTopLevelBlockElement(
  editor: TipTapEditor,
  block: ReturnType<typeof getCurrentTopLevelBlock>,
): HTMLElement | null {
  if (!block) return null;
  const dom = editor.view.nodeDOM(block.from);
  return dom instanceof HTMLElement ? dom : null;
}

function getContextMenuState(
  editor: TipTapEditor | null,
  position: { top: number; left: number },
  hasSelection = false,
): ContextMenuState {
  const block = editor ? getCurrentTopLevelBlock(editor) : null;
  return {
    position: {
      top: Math.max(8, Math.min(position.top, window.innerHeight - 320)),
      left: Math.max(8, Math.min(position.left, window.innerWidth - 200)),
    },
    hasSelection,
    inTable: editor?.isActive("table") ?? false,
    hasBlock: Boolean(block),
    canMoveBlockUp: Boolean(block && block.index > 0),
    canMoveBlockDown: Boolean(editor && block && block.index < editor.state.doc.childCount - 1),
  };
}

function getBlockMenuPosition(
  editor: TipTapEditor,
  block: ReturnType<typeof getCurrentTopLevelBlock>,
) {
  if (!block) return null;

  const dom = editor.view.nodeDOM(block.from);
  const editorRect = editor.view.dom.getBoundingClientRect();
  let blockRect: DOMRect | null = null;

  if (dom instanceof HTMLElement) {
    blockRect = dom.getBoundingClientRect();
  } else {
    const fallbackPosition = Math.min(block.to - 1, block.from + 1);
    const coords = editor.view.coordsAtPos(fallbackPosition);
    blockRect = {
      top: coords.top,
      bottom: coords.bottom,
      left: coords.left,
      right: coords.right,
      width: Math.max(0, coords.right - coords.left),
      height: Math.max(28, coords.bottom - coords.top),
      x: coords.left,
      y: coords.top,
      toJSON: () => ({}),
    } as DOMRect;
  }

  const top = blockRect.top + Math.max(0, (blockRect.height - 28) / 2);
  const left = editorRect.left - 44;

  return {
    action: {
      top: Math.max(52, top),
      left: Math.max(8, left),
    },
    helper: {
      top: Math.min(blockRect.bottom + 4, window.innerHeight - 38),
      left: Math.max(8, blockRect.left),
    },
  };
}

function moveCurrentBlock(editor: TipTapEditor, direction: "up" | "down") {
  const block = getCurrentTopLevelBlock(editor);
  if (!block) return false;

  const targetIndex = direction === "up" ? block.index - 1 : block.index + 1;
  if (targetIndex < 0 || targetIndex >= editor.state.doc.childCount) return false;

  let targetFrom = 0;
  for (let index = 0; index < targetIndex; index++) {
    targetFrom += editor.state.doc.child(index).nodeSize;
  }

  const target = editor.state.doc.child(targetIndex);
  const transaction = editor.state.tr.delete(block.from, block.to);
  const insertAt =
    direction === "up" ? targetFrom : targetFrom + target.nodeSize - block.node.nodeSize;

  editor.view.dispatch(transaction.insert(insertAt, block.node).scrollIntoView());
  editor.commands.focus(insertAt + 1);
  return true;
}

function moveBlockToDropTarget(
  editor: TipTapEditor,
  block: TopLevelBlock,
  pos: number,
  clientY: number,
) {
  const target = getTopLevelBlockAtPos(editor, pos);
  if (!target) return false;
  if (target.from === block.from && target.to === block.to) return false;
  if (target.from >= block.from && target.to <= block.to) return false;

  const targetDom = editor.view.nodeDOM(target.from);
  let insertBeforeTarget = true;
  if (targetDom instanceof HTMLElement) {
    const rect = targetDom.getBoundingClientRect();
    insertBeforeTarget = clientY < rect.top + rect.height / 2;
  } else {
    insertBeforeTarget = pos <= target.from;
  }

  let insertAt = insertBeforeTarget ? target.from : target.to;
  const deletedSize = block.to - block.from;
  const transaction = editor.state.tr.delete(block.from, block.to);
  if (insertAt > block.from) {
    insertAt -= deletedSize;
  }

  editor.view.dispatch(transaction.insert(insertAt, block.node).scrollIntoView());
  editor.commands.focus(Math.max(1, Math.min(insertAt + 1, editor.state.doc.content.size)));
  return true;
}

function deleteCurrentBlock(editor: TipTapEditor) {
  const block = getCurrentTopLevelBlock(editor);
  if (!block) return false;
  editor.chain().focus().deleteRange({ from: block.from, to: block.to }).run();
  return true;
}

function duplicateCurrentBlock(editor: TipTapEditor) {
  const block = getCurrentTopLevelBlock(editor);
  if (!block) return false;

  const transaction = editor.state.tr.insert(block.to, block.node.copy(block.node.content));
  editor.view.dispatch(transaction.scrollIntoView());
  editor.commands.focus(block.to + 1);
  return true;
}

function getSelectedText(editor: TipTapEditor): string {
  const { from, to, empty } = editor.state.selection;
  if (empty) return "";
  return editor.state.doc.textBetween(from, to, "\n\n", "\n");
}

function getSelectedMarkdown(editor: TipTapEditor): string {
  const { empty } = editor.state.selection;
  if (empty) return "";

  const fragment = editor.state.selection.content().content;
  const serializer = DOMSerializer.fromSchema(editor.state.schema);
  const container = document.createElement("div");
  container.appendChild(serializer.serializeFragment(fragment));
  return htmlToMarkdown(container.innerHTML);
}

function readExcalidrawSceneAttribute(value: string | null | undefined): ExcalidrawSceneData {
  if (!value) return { elements: [], appState: {}, files: {} };
  try {
    return normalizeExcalidrawScene(JSON.parse(decodeURIComponent(value)));
  } catch {
    return { elements: [], appState: {}, files: {} };
  }
}

function writeExcalidrawSceneAttribute(scene: ExcalidrawSceneData): string {
  return encodeURIComponent(JSON.stringify(normalizeExcalidrawScene(scene)));
}

function normalizeExcalidrawScene(value: unknown): ExcalidrawSceneData {
  const record = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  return {
    elements: Array.isArray(record.elements) ? record.elements : [],
    appState:
      record.appState && typeof record.appState === "object"
        ? sanitizeExcalidrawAppState(record.appState as Record<string, unknown>)
        : {},
    files:
      record.files && typeof record.files === "object"
        ? (record.files as Record<string, unknown>)
        : {},
  };
}

function parseExcalidrawSceneJson(value: string): ExcalidrawSceneData {
  try {
    return normalizeExcalidrawScene(JSON.parse(value));
  } catch {
    return { elements: [], appState: {}, files: {} };
  }
}

function serializeExcalidrawSceneJson(scene: ExcalidrawSceneData): string {
  return JSON.stringify(normalizeExcalidrawScene(scene), null, 2);
}

function sanitizeExcalidrawAppState(appState: Record<string, unknown>): Record<string, unknown> {
  const allowedKeys = [
    "currentItemBackgroundColor",
    "currentItemFillStyle",
    "currentItemFontFamily",
    "currentItemFontSize",
    "currentItemOpacity",
    "currentItemRoughness",
    "currentItemStrokeColor",
    "currentItemStrokeStyle",
    "currentItemStrokeWidth",
    "gridSize",
    "name",
    "scrollX",
    "scrollY",
    "theme",
    "viewBackgroundColor",
    "zenModeEnabled",
    "zoom",
  ];
  const sanitized: Record<string, unknown> = {};
  for (const key of allowedKeys) {
    if (key in appState) sanitized[key] = appState[key];
  }
  return sanitized;
}

function clampFloatingPosition(top: number, left: number, width: number, height: number) {
  return {
    top: Math.max(8, Math.min(top, window.innerHeight - height - 8)),
    left: Math.max(8, Math.min(left, window.innerWidth - width - 8)),
  };
}

function getMobileOverlayPosition(width: number, height: number): { top: number; left: number } {
  const viewport = window.visualViewport;
  const viewportTop = viewport?.offsetTop ?? 0;
  const viewportHeight = viewport?.height ?? window.innerHeight;
  const viewportWidth = viewport?.width ?? window.innerWidth;

  return clampFloatingPosition(
    viewportTop + viewportHeight - height - 72,
    Math.max(8, (viewportWidth - width) / 2),
    width,
    height,
  );
}

function getSelectionFloatingPosition(
  editor: TipTapEditor,
  width = 236,
  height = 40,
): { top: number; left: number } {
  const { from, to } = editor.state.selection;
  const start = editor.view.coordsAtPos(from);
  const end = editor.view.coordsAtPos(to);
  const top = Math.min(start.top, end.top) - height - 8;
  const left = (start.left + end.right) / 2 - width / 2;
  return clampFloatingPosition(top, left, width, height);
}

function getRectFloatingPosition(rect: DOMRect, width = 276, height = 40) {
  const top = rect.top - height - 8;
  const left = rect.left + rect.width / 2 - width / 2;
  return clampFloatingPosition(top, left, width, height);
}

function getBookmarkDomain(value: string): string {
  try {
    return new URL(value).hostname.replace(/^www\./i, "") || value;
  } catch {
    return value || "Link";
  }
}

function getActiveTableToolbarPosition(editor: TipTapEditor): { top: number; left: number } | null {
  const node = editor.view.domAtPos(editor.state.selection.from).node;
  const element = node instanceof Element ? node : node.parentElement;
  const table = element?.closest("table");
  if (!table) return null;

  const rect = table.getBoundingClientRect();
  return {
    top: Math.max(8, rect.top - 44),
    left: Math.max(8, Math.min(rect.left, window.innerWidth - 360)),
  };
}

function getLinkRange(
  editor: TipTapEditor,
  pos = editor.state.selection.from,
  href?: string,
): LinkMenuState | null {
  const markType = editor.state.schema.marks.link;
  if (!markType) return null;

  const attempts = [pos, pos + 1, pos - 1].filter(
    (candidate) => candidate >= 0 && candidate <= editor.state.doc.content.size,
  );

  for (const candidate of attempts) {
    const attrs = href ? { href } : editor.getAttributes("link");
    const range = getMarkRange(
      editor.state.doc.resolve(candidate),
      markType,
      attrs.href ? attrs : undefined,
    );
    if (!range) continue;

    const text = editor.state.doc.textBetween(range.from, range.to, "", "");
    const mark = editor.state.doc
      .resolve(Math.min(range.from + 1, editor.state.doc.content.size))
      .marks()
      .find((item) => item.type === markType);
    const rangeHref = href || mark?.attrs.href || attrs.href || "";
    return {
      from: range.from,
      to: range.to,
      text,
      href: rangeHref,
      position: getSelectionFloatingPosition(editor),
    };
  }

  return null;
}

function normalizeLinkHref(href: string): string {
  const trimmed = href.trim();
  if (!trimmed) return "";
  if (/^([a-z][a-z0-9+.-]*:|\/|#)/i.test(trimmed)) return trimmed;
  return `https://${trimmed}`;
}

function getExtension(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot >= 0 ? name.slice(dot + 1).toLowerCase() : "";
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

function insertAttachmentCard(editor: TipTapEditor, name: string, size: number, url?: string) {
  const title = escapeHtml(name);
  const label = url
    ? `<a href="${escapeHtmlAttribute(url)}" class="text-primary underline">${title}</a>`
    : `<strong>${title}</strong>`;
  const html =
    `<div data-type="file-attachment" class="file-attachment-card">` +
    `<p>${label} <em>(${formatFileSize(size)})</em></p>` +
    `</div>`;
  editor.chain().focus().insertContent(html).run();
}

function isEditorImageFile(file: File): boolean {
  const ext = getExtension(file.name);
  return file.type.startsWith("image/") || EDITOR_IMAGE_EXTENSIONS.has(ext);
}

function persistMobileToolbarActions(actions: MobileToolbarActionId[]) {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(MOBILE_TOOLBAR_STORAGE_KEY, JSON.stringify(actions));
}

function normalizeMobileToolbarActions(value: unknown): MobileToolbarActionId[] {
  if (!Array.isArray(value)) return DEFAULT_MOBILE_TOOLBAR_ACTIONS;
  const next: MobileToolbarActionId[] = [];

  for (const item of value) {
    if (item === "file" || item === "image") {
      if (!next.includes("upload")) next.push("upload");
      continue;
    }
    if (typeof item === "string" && MOBILE_TOOLBAR_ACTION_SET.has(item as MobileToolbarActionId)) {
      const action = item as MobileToolbarActionId;
      if (!next.includes(action)) next.push(action);
    }
  }

  if (next.includes("code") && !next.includes("codeBlock")) {
    next.splice(next.indexOf("code") + 1, 0, "codeBlock");
  }

  return next.length > 0 ? next : DEFAULT_MOBILE_TOOLBAR_ACTIONS;
}

function getInitialMobileToolbarActions(): MobileToolbarActionId[] {
  if (typeof window === "undefined") return DEFAULT_MOBILE_TOOLBAR_ACTIONS;

  try {
    const raw = window.localStorage.getItem(MOBILE_TOOLBAR_STORAGE_KEY);
    if (!raw) return DEFAULT_MOBILE_TOOLBAR_ACTIONS;
    const parsed = JSON.parse(raw) as unknown;
    return normalizeMobileToolbarActions(parsed);
  } catch {
    return DEFAULT_MOBILE_TOOLBAR_ACTIONS;
  }
}

function normalizeTagName(value: string): string {
  return value
    .trim()
    .replace(/^#+/, "")
    .replace(/\s+/g, "-")
    .replace(/[^\w-]/g, "")
    .toLowerCase();
}

function contentTagToHtml(tagName: string): string {
  const safeTag = escapeHtmlAttribute(tagName);
  return `<span data-type="content-tag" data-tag="${safeTag}" class="editor-content-tag">#${escapeHtml(
    tagName,
  )}</span>&nbsp;`;
}

export function isCommandPaletteShortcut(
  event: Pick<KeyboardEvent, "altKey" | "code" | "ctrlKey" | "key" | "metaKey">,
) {
  return (
    event.ctrlKey &&
    !event.metaKey &&
    !event.altKey &&
    (event.key === "/" ||
      event.key === "?" ||
      event.code === "Slash" ||
      event.code === "NumpadDivide")
  );
}

function getMobileToolbarAction(action: MobileToolbarActionId, editor: TipTapEditor) {
  switch (action) {
    case "undo":
      return { label: "Undo", icon: <Undo2 size={18} />, active: false };
    case "redo":
      return { label: "Redo", icon: <Redo2 size={18} />, active: false };
    case "link":
      return { label: "Link", icon: <LinkIcon size={18} />, active: editor.isActive("link") };
    case "bookmark":
      return { label: "Bookmark", icon: <Bookmark size={18} />, active: false };
    case "upload":
      return { label: "Upload", icon: <Upload size={18} />, active: false };
    case "tag":
      return { label: "Tag", icon: <Tag size={18} />, active: false };
    case "heading":
      return { label: "Heading", icon: <Heading size={18} />, active: editor.isActive("heading") };
    case "bold":
      return { label: "Bold", icon: <Bold size={18} />, active: editor.isActive("bold") };
    case "italic":
      return { label: "Italic", icon: <Italic size={18} />, active: editor.isActive("italic") };
    case "underline":
      return {
        label: "Underline",
        icon: <UnderlineIcon size={18} />,
        active: editor.isActive("underline"),
      };
    case "strike":
      return {
        label: "Strikethrough",
        icon: <Strikethrough size={18} />,
        active: editor.isActive("strike"),
      };
    case "highlight":
      return {
        label: "Highlight",
        icon: <Highlighter size={18} />,
        active: editor.isActive("highlight"),
      };
    case "code":
      return { label: "Inline code", icon: <Code size={18} />, active: editor.isActive("code") };
    case "codeBlock":
      return {
        label: "Code block",
        icon: <FileCode size={18} />,
        active: editor.isActive("codeBlock"),
      };
    case "quote":
      return { label: "Quote", icon: <Quote size={18} />, active: editor.isActive("blockquote") };
    case "bullet":
      return {
        label: "Bullet list",
        icon: <List size={18} />,
        active: editor.isActive("bulletList"),
      };
    case "numbered":
      return {
        label: "Numbered list",
        icon: <ListOrdered size={18} />,
        active: editor.isActive("orderedList"),
      };
    case "task":
      return {
        label: "Task list",
        icon: <ListChecks size={18} />,
        active: editor.isActive("taskList"),
      };
    case "indent":
      return { label: "Indent list item", icon: <IndentIncrease size={18} />, active: false };
    case "outdent":
      return { label: "Outdent list item", icon: <IndentDecrease size={18} />, active: false };
    case "commands":
      return { label: "Command menu", icon: <Command size={18} />, active: false };
  }
}

async function writeClipboardText(text: string): Promise<void> {
  if (!text) return;
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }

  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "true");
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  document.execCommand("copy");
  textarea.remove();
}

async function readClipboardText(): Promise<string> {
  if (!navigator.clipboard?.readText) return "";
  return navigator.clipboard.readText();
}

export function Editor({
  content,
  onChange,
  readOnly = false,
  plainMode = false,
  showBlockChrome = true,
  showMobileCommandButton = false,
  templates = [],
  templateContext = {},
  assetContext = {},
  spaceId,
  onEditorReady,
}: EditorProps) {
  const { cssVar } = useContext(EditorZoomContext);
  const { config } = useAuth();
  const maxPickedFileMb = getConfiguredUploadLimitMb(config?.max_upload_size_mb);
  const maxPickedFileBytes = maxPickedFileMb * 1024 * 1024;
  const [slashState, setSlashState] = useState<SlashState | null>(null);
  const [blockMenu, setBlockMenu] = useState<BlockMenuState | null>(null);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [selectionMenu, setSelectionMenu] = useState<FloatingMenuState | null>(null);
  const [selectionHeadingOpen, setSelectionHeadingOpen] = useState(false);
  const [selectionFontOpen, setSelectionFontOpen] = useState(false);
  const [linkMenu, setLinkMenu] = useState<LinkMenuState | null>(null);
  const [linkDialog, setLinkDialog] = useState<LinkDialogState | null>(null);
  const [bookmarkDialog, setBookmarkDialog] = useState<BookmarkDialogState | null>(null);
  const [tableToolbarPosition, setTableToolbarPosition] = useState<{ top: number; left: number } | null>(null);
  const [uploadMenu, setUploadMenu] = useState<UploadMenuState | null>(null);
  const [tagDialog, setTagDialog] = useState<TagDialogState | null>(null);
  const [wikiLinkDialog, setWikiLinkDialog] = useState<WikiLinkDialogState | null>(null);
  const [mentionDialog, setMentionDialog] = useState<MentionDialogState | null>(null);
  const [excalidrawDialog, setExcalidrawDialog] = useState<ExcalidrawDialogState | null>(null);
  const [excalidrawStatus, setExcalidrawStatus] = useState<string | null>(null);
  const [excalidrawSaving, setExcalidrawSaving] = useState(false);
  const [uploadWarning, setUploadWarning] = useState<string | null>(null);
  const [mobileKeyboardInset, setMobileKeyboardInset] = useState(0);
  const [mobileToolbarActions, setMobileToolbarActions] = useState<MobileToolbarActionId[]>(
    getInitialMobileToolbarActions,
  );
  const [mobileToolbarCustomizing, setMobileToolbarCustomizing] = useState(false);
  const [mobileHeadingMenuOpen, setMobileHeadingMenuOpen] = useState(false);
  const [languageMenuOpen, setLanguageMenuOpen] = useState(false);
  const slashStateRef = useRef<SlashState | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const selectionHeadingRef = useRef<HTMLDivElement | null>(null);
  const selectionFontRef = useRef<HTMLDivElement | null>(null);
  const draggedToolbarActionRef = useRef<MobileToolbarActionId | null>(null);
  const draggedBlockRef = useRef<TopLevelBlock | null>(null);
  const excalidrawDraftRef = useRef<ExcalidrawSceneData>({
    elements: [],
    appState: {},
    files: {},
  });
  const mobileViewportBaselineRef = useRef(0);
  const mobileViewportWidthRef = useRef(0);
  const editorInstanceRef = useRef<TipTapEditor | null>(null);
  const activeBlockElementRef = useRef<HTMLElement | null>(null);
  // Skip onChange during initial content load to prevent auto-save loop
  const isInitializing = useRef(true);

  const setSlash = (next: SlashState | null) => {
    slashStateRef.current = next;
    setSlashState(next);
  };

  useEffect(() => {
    const closeSelectionDropdowns = (event: MouseEvent) => {
      const target = event.target as Node;
      if (
        selectionHeadingRef.current?.contains(target) ||
        selectionFontRef.current?.contains(target)
      ) {
        return;
      }
      setSelectionHeadingOpen(false);
      setSelectionFontOpen(false);
    };

    document.addEventListener("mousedown", closeSelectionDropdowns);
    return () => document.removeEventListener("mousedown", closeSelectionDropdowns);
  }, []);

  const clearActiveBlockMarker = () => {
    const activeBlock = activeBlockElementRef.current;
    if (!activeBlock) return;
    activeBlock.classList.remove("is-active-editor-block");
    activeBlock.removeAttribute("data-block-label");
    activeBlockElementRef.current = null;
  };

  const syncActiveBlockMarker = (tipTapEditor: TipTapEditor) => {
    if (!showBlockChrome || readOnly || !tipTapEditor.isFocused || tipTapEditor.isActive("table")) {
      clearActiveBlockMarker();
      return;
    }

    const block = getCurrentTopLevelBlock(tipTapEditor);
    const blockElement = getTopLevelBlockElement(tipTapEditor, block);
    if (!blockElement) {
      clearActiveBlockMarker();
      return;
    }

    if (activeBlockElementRef.current && activeBlockElementRef.current !== blockElement) {
      clearActiveBlockMarker();
    }

    blockElement.classList.add("is-active-editor-block");
    blockElement.dataset.blockLabel = getBlockLabel(tipTapEditor);
    activeBlockElementRef.current = blockElement;
  };

  const updateBlockMenu = (tipTapEditor: TipTapEditor, open = false) => {
    if (!showBlockChrome || readOnly || !tipTapEditor.isFocused || tipTapEditor.isActive("table")) {
      setBlockMenu(null);
      return;
    }

    const block = getCurrentTopLevelBlock(tipTapEditor);
    const position = getBlockMenuPosition(tipTapEditor, block);
    if (!block || !position) {
      setBlockMenu(null);
      return;
    }

    setBlockMenu({
      open,
      position: position.action,
      helperPosition: position.helper,
      canMoveUp: block.index > 0,
      canMoveDown: block.index < tipTapEditor.state.doc.childCount - 1,
      label: getBlockLabel(tipTapEditor),
      isCodeBlock: tipTapEditor.isActive("codeBlock"),
      codeLanguage: tipTapEditor.getAttributes("codeBlock").language || "",
    });
  };

  const openSearchableCommandPalette = useCallback(
    (tipTapEditor: TipTapEditor) => {
      if (readOnly) {
        return false;
      }

      const { from, to, empty } = tipTapEditor.state.selection;
      const coords = tipTapEditor.view.coordsAtPos(empty ? from : to);
      const mobilePalettePosition = showMobileCommandButton
        ? getMobileOverlayPosition(384, 340)
        : null;
      setSlash({
        query: "",
        from,
        to,
        position: mobilePalettePosition ?? {
          top: Math.min(coords.bottom + 6, window.innerHeight - 340),
          left: Math.max(8, Math.min(coords.left, window.innerWidth - 280)),
        },
        searchable: true,
      });
      setSelectionMenu(null);
      setLinkMenu(null);
      setContextMenu(null);
      setUploadMenu(null);
      return true;
    },
    [readOnly, showMobileCommandButton],
  );

  const openUploadMenu = useCallback(
    (
      preferImage: boolean,
      tipTapEditor?: TipTapEditor | null,
      overridePosition?: { top: number; left: number },
    ) => {
      if (readOnly) return;
      const activeEditor = tipTapEditor ?? editorInstanceRef.current;
      const fallbackPosition = showMobileCommandButton
        ? getMobileOverlayPosition(320, 180)
        : {
            top: Math.max(64, window.innerHeight / 2 - 120),
            left: Math.max(8, window.innerWidth / 2 - 160),
          };
      const position = showMobileCommandButton
        ? fallbackPosition
        : (overridePosition ??
          (activeEditor ? getSelectionFloatingPosition(activeEditor, 320, 180) : fallbackPosition));

      setUploadMenu({
        preferImage,
        position: clampFloatingPosition(position.top, position.left, 320, 180),
      });
      setSlash(null);
      setSelectionMenu(null);
      setLinkMenu(null);
      setLinkDialog(null);
      setTagDialog(null);
      setContextMenu(null);
    },
    [readOnly, showMobileCommandButton],
  );

  const openExcalidrawDialog = async (canvasElement: HTMLElement) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || readOnly) return;
    const position = activeEditor.view.posAtDOM(canvasElement, 0);
    const node = activeEditor.state.doc.nodeAt(position);
    if (!node || node.type.name !== "excalidrawCanvas") return;
    const filename = normalizeExcalidrawFilename(String(node.attrs.filename || ""));
    let scene = readExcalidrawSceneAttribute(String(node.attrs.scene || ""));
    let fileBacked = false;
    setExcalidrawStatus(null);
    if (assetContext.document_id) {
      try {
        const storedScene = await dataClient.readDocumentTextAsset(filename, assetContext);
        if (storedScene) {
          scene = parseExcalidrawSceneJson(storedScene);
          fileBacked = true;
        }
      } catch {
        setExcalidrawStatus("Could not read the drawing file. Opening the embedded fallback.");
      }
    }
    excalidrawDraftRef.current = scene;
    setExcalidrawDialog({
      position,
      filename,
      scene,
      fileBacked,
    });
    setSlash(null);
    setSelectionMenu(null);
    setLinkMenu(null);
    setLinkDialog(null);
    setTagDialog(null);
    setUploadMenu(null);
    setContextMenu(null);
  };

  const saveExcalidrawDialog = async () => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !excalidrawDialog) return;
    const node = activeEditor.state.doc.nodeAt(excalidrawDialog.position);
    if (!node || node.type.name !== "excalidrawCanvas") {
      setExcalidrawDialog(null);
      return;
    }
    const normalizedFilename = normalizeExcalidrawFilename(excalidrawDialog.filename);
    setExcalidrawSaving(true);
    setExcalidrawStatus(null);
    const sceneAttribute = writeExcalidrawSceneAttribute(excalidrawDraftRef.current);
    let fileBacked = false;
    if (assetContext.document_id) {
      try {
        await dataClient.saveDocumentTextAsset({
          name: normalizedFilename,
          mime_type: "application/vnd.excalidraw+json",
          text: serializeExcalidrawSceneJson(excalidrawDraftRef.current),
          space_id: assetContext.space_id,
          document_id: assetContext.document_id,
        });
        fileBacked = true;
      } catch {
        setExcalidrawSaving(false);
        setExcalidrawStatus("Drawing file save failed. No document changes were written.");
        return;
      }
    }
    const transaction = activeEditor.state.tr.setNodeMarkup(excalidrawDialog.position, undefined, {
      ...node.attrs,
      filename: normalizedFilename,
      scene: sceneAttribute,
    });
    activeEditor.view.dispatch(transaction);
    activeEditor.commands.focus(excalidrawDialog.position + 1);
    setExcalidrawSaving(false);
    if (fileBacked) setExcalidrawStatus(null);
    setExcalidrawDialog(null);
  };

  const chooseUploadSource = (preferImage: boolean) => {
    const input = preferImage ? imageInputRef.current : fileInputRef.current;
    input?.click();
    setUploadMenu(null);
  };

  const updateSelectionMenu = (tipTapEditor: TipTapEditor) => {
    if (showMobileCommandButton) {
      setSelectionMenu(null);
      return;
    }

    if (tipTapEditor.state.selection.empty) {
      setSelectionMenu(null);
      const activeLink = getLinkRange(tipTapEditor, tipTapEditor.state.selection.from);
      if (activeLink && !linkDialog && !uploadMenu && !tagDialog) {
        setLinkMenu(activeLink);
      } else if (!activeLink) {
        setLinkMenu(null);
      }
      return;
    }

    if (readOnly) {
      setSelectionMenu(null);
      return;
    }

    setSelectionMenu({ position: getSelectionFloatingPosition(tipTapEditor, 520, 40) });
    setLinkMenu(null);
    setUploadMenu(null);
    setTagDialog(null);
  };

  const openTagDialog = (tipTapEditor: TipTapEditor) => {
    if (readOnly) return;
    const { from, to, empty } = tipTapEditor.state.selection;
    const position = showMobileCommandButton
      ? getMobileOverlayPosition(288, 132)
      : getSelectionFloatingPosition(tipTapEditor, 288, 132);
    setSlash(null);
    setSelectionMenu(null);
    setLinkMenu(null);
    setLinkDialog(null);
    setUploadMenu(null);
    setTagDialog({
      from,
      to,
      text: empty ? "" : normalizeTagName(getSelectedText(tipTapEditor)),
      position: clampFloatingPosition(position.top, position.left, 288, 132),
    });
  };

  const applyTagDialog = () => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !tagDialog) return;
    const tagName = normalizeTagName(tagDialog.text);
    if (!tagName) return;

    const safeFrom = Math.max(0, Math.min(tagDialog.from, activeEditor.state.doc.content.size));
    const safeTo = Math.max(0, Math.min(tagDialog.to, activeEditor.state.doc.content.size));
    activeEditor
      .chain()
      .focus()
      .deleteRange({ from: Math.min(safeFrom, safeTo), to: Math.max(safeFrom, safeTo) })
      .insertContent(contentTagToHtml(tagName))
      .run();
    setTagDialog(null);
  };

  const openLinkDialog = (
    tipTapEditor: TipTapEditor,
    override?: Partial<LinkDialogState> & { position?: { top: number; left: number } },
  ) => {
    const activeLink = getLinkRange(tipTapEditor, tipTapEditor.state.selection.from);
    const from = override?.from ?? activeLink?.from ?? tipTapEditor.state.selection.from;
    const to = override?.to ?? activeLink?.to ?? tipTapEditor.state.selection.to;
    const text =
      override?.text ??
      activeLink?.text ??
      (tipTapEditor.state.selection.empty ? "" : getSelectedText(tipTapEditor));
    const href = override?.href ?? activeLink?.href ?? "";
    const position =
      override?.position ??
      activeLink?.position ??
      getSelectionFloatingPosition(tipTapEditor, 288, 180);

    setSelectionMenu(null);
    setLinkMenu(null);
    setContextMenu(null);
    setUploadMenu(null);
    setTagDialog(null);
    setLinkDialog({
      from,
      to,
      text,
      href,
      position: clampFloatingPosition(position.top, position.left, 288, 180),
    });
  };

  const updateTableToolbar = (tipTapEditor: TipTapEditor) => {
    if (readOnly || !tipTapEditor.isFocused || !tipTapEditor.isActive("table")) {
      setTableToolbarPosition(null);
      return;
    }
    setTableToolbarPosition(getActiveTableToolbarPosition(tipTapEditor));
  };

  const openBookmarkDialog = (tipTapEditor: TipTapEditor) => {
    if (readOnly) return;
    const { from, to, empty } = tipTapEditor.state.selection;
    const selectedText = empty ? "" : getSelectedText(tipTapEditor).trim();
    const position = showMobileCommandButton
      ? getMobileOverlayPosition(320, 268)
      : getSelectionFloatingPosition(tipTapEditor, 320, 268);
    setSlash(null);
    setSelectionMenu(null);
    setLinkMenu(null);
    setLinkDialog(null);
    setUploadMenu(null);
    setTagDialog(null);
    setBookmarkDialog({
      from,
      to,
      href: "",
      title: selectedText,
      description: "",
      position: clampFloatingPosition(position.top, position.left, 320, 268),
    });
  };

  const applyBookmarkDialog = () => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !bookmarkDialog) return;
    const href = normalizeLinkHref(bookmarkDialog.href);
    if (!href) return;
    const title = bookmarkDialog.title.trim() || getBookmarkDomain(href);
    const description = bookmarkDialog.description.trim();
    const from = Math.max(0, Math.min(bookmarkDialog.from, activeEditor.state.doc.content.size));
    const to = Math.max(0, Math.min(bookmarkDialog.to, activeEditor.state.doc.content.size));

    activeEditor
      .chain()
      .focus()
      .deleteRange({ from: Math.min(from, to), to: Math.max(from, to) })
      .insertContent({ type: "bookmarkCard", attrs: { href, title, description } })
      .run();
    setBookmarkDialog(null);
  };

  const applyLinkDialog = () => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !linkDialog) return;

    const href = normalizeLinkHref(linkDialog.href);
    const text = linkDialog.text.trim() || href;
    if (!href || !text) return;

    const range = {
      from: Math.max(0, Math.min(linkDialog.from, activeEditor.state.doc.content.size)),
      to: Math.max(0, Math.min(linkDialog.to, activeEditor.state.doc.content.size)),
    };
    const safeFrom = Math.min(range.from, range.to);
    const safeTo = Math.max(range.from, range.to);
    const linkTo = safeFrom + text.length;

    activeEditor
      .chain()
      .focus()
      .deleteRange({ from: safeFrom, to: safeTo })
      .insertContent(text)
      .run();
    activeEditor
      .chain()
      .focus()
      .setTextSelection({
        from: safeFrom,
        to: Math.min(linkTo, activeEditor.state.doc.content.size),
      })
      .setLink({ href, class: "text-primary underline cursor-pointer" })
      .setTextSelection(Math.min(linkTo, activeEditor.state.doc.content.size))
      .run();

    setLinkDialog(null);
  };

  const openWikiLinkDialog = (range: { from: number; to: number }, position: { top: number; left: number }) => {
    if (!spaceId) return;
    setWikiLinkDialog({
      from: range.from,
      to: range.to,
      position: clampFloatingPosition(position.top, position.left, 288, 260),
      documents: [],
      query: "",
      loading: true,
      error: false,
    });
    void dataClient
      .listDocuments(spaceId, { include_nested: true })
      .then((documents) =>
        setWikiLinkDialog((current) => current ? { ...current, documents, loading: false } : current),
      )
      .catch(() => setWikiLinkDialog((current) => current ? { ...current, loading: false, error: true } : current));
  };

  const insertWikiLink = (document: DocumentRecord) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !wikiLinkDialog) return;
    const safeFrom = Math.max(0, Math.min(wikiLinkDialog.from, activeEditor.state.doc.content.size));
    const safeTo = Math.max(0, Math.min(wikiLinkDialog.to, activeEditor.state.doc.content.size));
    activeEditor
      .chain()
      .focus()
      .deleteRange({ from: Math.min(safeFrom, safeTo), to: Math.max(safeFrom, safeTo) })
      .insertContent(`[[${document.title}|${document.id}]]`)
      .run();
    setWikiLinkDialog(null);
  };
  const openMentionDialog = (range: { from: number; to: number }, position: { top: number; left: number }) => {
    if (!spaceId) return;
    setMentionDialog({ from: range.from, to: range.to, position: clampFloatingPosition(position.top, position.left, 288, 260), members: [], query: "", loading: true, error: false });
    void dataClient.listSpaceMembers(spaceId).then((members) => setMentionDialog((current) => current ? { ...current, members, loading: false } : current)).catch(() => setMentionDialog((current) => current ? { ...current, loading: false, error: true } : current));
  };
  const insertMention = (member: SpaceMemberRecord) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !mentionDialog) return;
    activeEditor.chain().focus().deleteRange({ from: Math.min(mentionDialog.from, mentionDialog.to), to: Math.max(mentionDialog.from, mentionDialog.to) }).insertContent(`@[${member.display_name}](${member.user_id})`).run();
    setMentionDialog(null);
  };

  const removeLink = (state: LinkMenuState) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor) return;

    activeEditor
      .chain()
      .focus()
      .setTextSelection({ from: state.from, to: state.to })
      .unsetLink()
      .run();
    setLinkMenu(null);
  };

  const handlePickedFiles = async (fileList: FileList | null, preferImage = false) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || readOnly || !fileList?.length) return;

    for (const file of Array.from(fileList)) {
      if (file.size > maxPickedFileBytes) {
        setUploadWarning(formatEditorUploadLimitWarning(file.name, maxPickedFileMb));
        continue;
      }
      setUploadWarning(null);
      const dataUrl = await readFileAsDataUrl(file);
      const asset = await dataClient.saveBrowserAsset({
        name: file.name,
        mime_type: file.type || undefined,
        size: file.size,
        url: dataUrl,
        ...assetContext,
      });
      if (preferImage || isEditorImageFile(file)) {
        activeEditor.chain().focus().setImage({ src: asset.url, alt: file.name }).run();
      } else {
        insertAttachmentCard(activeEditor, asset.name, asset.size, asset.url);
      }
    }
  };

  const toggleMobileToolbarAction = (action: MobileToolbarActionId) => {
    setMobileToolbarActions((current) => {
      const next = current.includes(action)
        ? current.filter((item) => item !== action)
        : [...current, action];
      const normalized = next.length > 0 ? next : DEFAULT_MOBILE_TOOLBAR_ACTIONS;
      persistMobileToolbarActions(normalized);
      return normalized;
    });
  };

  const moveMobileToolbarAction = (action: MobileToolbarActionId, direction: "up" | "down") => {
    setMobileToolbarActions((current) => {
      const index = current.indexOf(action);
      if (index < 0) return current;
      const targetIndex = direction === "up" ? index - 1 : index + 1;
      if (targetIndex < 0 || targetIndex >= current.length) return current;
      const next = [...current];
      [next[index], next[targetIndex]] = [next[targetIndex], next[index]];
      persistMobileToolbarActions(next);
      return next;
    });
  };

  const reorderMobileToolbarAction = (
    draggedAction: MobileToolbarActionId | null,
    targetAction: MobileToolbarActionId,
  ) => {
    if (!draggedAction || draggedAction === targetAction) return;
    setMobileToolbarActions((current) => {
      const fromIndex = current.indexOf(draggedAction);
      const toIndex = current.indexOf(targetAction);
      if (fromIndex < 0 || toIndex < 0) return current;
      const next = [...current];
      const [moved] = next.splice(fromIndex, 1);
      next.splice(toIndex, 0, moved);
      persistMobileToolbarActions(next);
      return next;
    });
  };

  const applySlashCommand = (command: SlashCommand) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !slashState) return;

    const insertedFrom = slashState.from;
    const selectedRange = { from: slashState.from, to: slashState.to };
    const hasSelection = selectedRange.from !== selectedRange.to;

    const insertMarkdown = (markdown: string) => {
      if (hasSelection) {
        activeEditor
          .chain()
          .focus()
          .deleteRange(selectedRange)
          .insertContent(markdownToHtml(markdown))
          .run();
        return;
      }

      activeEditor.chain().focus().insertContent(markdownToHtml(markdown)).run();
    };

    if (command.id === "paragraph") {
      activeEditor.chain().focus().setTextSelection(selectedRange).setParagraph().run();
    } else if (/^h[1-6]$/.test(command.id)) {
      const level = Number(command.id.slice(1)) as 1 | 2 | 3 | 4 | 5 | 6;
      activeEditor.chain().focus().setTextSelection(selectedRange).toggleHeading({ level }).run();
    } else if (command.id === "bullet") {
      activeEditor.chain().focus().setTextSelection(selectedRange).toggleBulletList().run();
    } else if (command.id === "numbered") {
      activeEditor.chain().focus().setTextSelection(selectedRange).toggleOrderedList().run();
    } else if (command.id === "task") {
      activeEditor.chain().focus().setTextSelection(selectedRange).toggleTaskList().run();
    } else if (command.id === "quote") {
      activeEditor.chain().focus().setTextSelection(selectedRange).toggleBlockquote().run();
    } else if (command.id === "code") {
      activeEditor.chain().focus().setTextSelection(selectedRange).toggleCodeBlock().run();
    } else if (command.id === "divider") {
      activeEditor.chain().focus().setHorizontalRule().run();
    } else if (command.id === "table") {
      if (hasSelection) {
        activeEditor.chain().focus().deleteRange(selectedRange).run();
      }
      activeEditor.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run();
    } else if (command.id === "date") {
      insertMarkdown(formatSmartDate(new Date()));
    } else if (command.id === "wiki-link") {
      openWikiLinkDialog(selectedRange, slashState.position);
      setSlash(null);
      return;
    } else if (command.id === "mention") {
      openMentionDialog(selectedRange, slashState.position);
      setSlash(null);
      return;
    } else if (command.id === "link") {
      openLinkDialog(activeEditor, {
        from: slashState.from,
        to: slashState.to,
        position: slashState.position,
      });
    } else if (command.id === "bookmark") {
      activeEditor.chain().focus().setTextSelection(selectedRange).run();
      openBookmarkDialog(activeEditor);
      setSlash(null);
      return;
    } else if (command.id === "upload" || command.id === "image" || command.id === "file") {
      activeEditor.chain().focus().setTextSelection(selectedRange).run();
      openUploadMenu(command.id !== "file", activeEditor, slashState.position);
      return;
    } else if (command.action) {
      insertMarkdown(command.action);
    }

    if (command.id !== "link" && command.cursorOffset != null) {
      const target = Math.min(
        insertedFrom + command.cursorOffset,
        activeEditor.state.doc.content.size,
      );
      activeEditor.chain().focus().setTextSelection(Math.max(1, target)).run();
    }
    setSlash(null);
  };

  const copySelection = async (format: "text" | "markdown") => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor) return;
    const text =
      format === "markdown" ? getSelectedMarkdown(activeEditor) : getSelectedText(activeEditor);
    await writeClipboardText(text);
    setContextMenu(null);
  };

  const pasteClipboard = async (format: "markdown" | "plain") => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || readOnly) return;
    const text = await readClipboardText();
    if (!text) {
      setContextMenu(null);
      return;
    }

    if (format === "markdown") {
      activeEditor.chain().focus().insertContent(markdownToHtml(text)).run();
    } else {
      activeEditor.chain().focus().insertContent(text).run();
    }
    setContextMenu(null);
  };

  const runToolbarAction = (action: MobileToolbarActionId) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || readOnly) return;

    const chain = activeEditor.chain().focus();
    switch (action) {
      case "undo":
        activeEditor.chain().focus().undo().run();
        break;
      case "redo":
        activeEditor.chain().focus().redo().run();
        break;
      case "link":
        openLinkDialog(activeEditor);
        break;
      case "bookmark":
        openBookmarkDialog(activeEditor);
        break;
      case "upload":
        openUploadMenu(true, activeEditor);
        break;
      case "tag":
        openTagDialog(activeEditor);
        break;
      case "heading":
        setMobileHeadingMenuOpen((current) => !current);
        break;
      case "bold":
        chain.toggleBold().run();
        break;
      case "italic":
        chain.toggleItalic().run();
        break;
      case "underline":
        chain.toggleUnderline().run();
        break;
      case "strike":
        chain.toggleStrike().run();
        break;
      case "highlight":
        chain.toggleHighlight().run();
        break;
      case "code":
        chain.toggleCode().run();
        break;
      case "codeBlock":
        chain.toggleCodeBlock().run();
        break;
      case "quote":
        chain.toggleBlockquote().run();
        break;
      case "bullet":
        chain.toggleBulletList().run();
        break;
      case "numbered":
        chain.toggleOrderedList().run();
        break;
      case "task":
        chain.toggleTaskList().run();
        break;
      case "indent":
        if (activeEditor.isActive("codeBlock")) {
          activeEditor.chain().focus().insertContent("  ").run();
        } else {
          chain.sinkListItem("listItem").run();
        }
        break;
      case "outdent":
        if (activeEditor.isActive("codeBlock")) {
          const { $from } = activeEditor.state.selection;
          const textBefore = $from.parent.textBetween(0, $from.parentOffset, "\n", "\n");
          const lineStartOffset = textBefore.lastIndexOf("\n") + 1;
          const blockLineStart = $from.start() + lineStartOffset;
          const lineText = $from.parent
            .textBetween(lineStartOffset, $from.parent.content.size, "\n", "\n")
            .split("\n")[0];
          if (lineText.startsWith("  ")) {
            activeEditor.chain().focus().deleteRange({ from: blockLineStart, to: blockLineStart + 2 }).run();
          } else if (lineText.startsWith("\t")) {
            activeEditor.chain().focus().deleteRange({ from: blockLineStart, to: blockLineStart + 1 }).run();
          }
        } else {
          chain.liftListItem("listItem").run();
        }
        break;
      case "commands":
        openSearchableCommandPalette(activeEditor);
        break;
    }
  };

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        codeBlock: false,
        heading: { levels: [1, 2, 3, 4, 5, 6] },
        link: false,
        underline: false,
      }),
      Placeholder.configure({
        placeholder: showBlockChrome
          ? "Start writing, or press Ctrl+/ for commands..."
          : "Start writing, then use Commands above the keyboard...",
      }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Link.configure({
        autolink: false,
        openOnClick: false,
        HTMLAttributes: { class: "text-primary underline cursor-pointer" },
      }),
      ImageExt.configure({
        HTMLAttributes: { class: "rounded-card max-w-full" },
      }),
      Table.configure({ resizable: true }),
      TableRow,
      TableHeader,
      TableCell,
      Highlight.configure({ multicolor: true }),
      Typography,
      Underline,
      FontFamily,
      ContentTag,
      CodeBlockLowlight.configure({ lowlight }),
      BookmarkCard,
      ExcalidrawCanvas,
    ],
    content: markdownToHtml(content),
    editable: !readOnly,
    editorProps: {
      attributes: {
        class: [
          "tiptap-editor outline-none min-h-[300px]",
          plainMode ? "font-mono text-sm" : "font-sans text-body",
        ].join(" "),
        spellcheck: "true",
      },
      handleKeyDown: (_view, event): boolean => {
        const activeEditor = editorInstanceRef.current;

        if (event.key === "Escape") {
          setSlash(null);
          setContextMenu(null);
          setBlockMenu(null);
          setSelectionMenu(null);
          setLinkMenu(null);
          setLinkDialog(null);
          setBookmarkDialog(null);
          setUploadMenu(null);
          setTagDialog(null);
          setMobileHeadingMenuOpen(false);
        }

        if (event.key === "Tab" && activeEditor?.isActive("codeBlock")) {
          event.preventDefault();
          activeEditor.chain().focus().insertContent("  ").run();
          return true;
        }

        if (isCommandPaletteShortcut(event)) {
          event.preventDefault();
          if (activeEditor && !activeEditor.isDestroyed) {
            openSearchableCommandPalette(activeEditor);
          }
          return true;
        }

        if (slashStateRef.current && (event.key === " " || event.key === "Tab")) {
          setSlash(null);
        }

        if (event.key !== "Enter" || event.shiftKey || !activeEditor?.isActive("table"))
          return false;
        if (!selectionIsInLastTableRow(activeEditor)) return false;

        event.preventDefault();
        return activeEditor.chain().focus().addRowAfter().run();
      },
      handleDOMEvents: {
        pointerover: (view, event) => {
          if (!(event.target instanceof Element)) return false;
          const anchor = event.target.closest("a[href]");
          if (!anchor || !view.dom.contains(anchor)) return false;
          if (linkDialog || uploadMenu || tagDialog) return false;

          const activeEditor = editorInstanceRef.current;
          const href = anchor.getAttribute("href") || "";
          const pos = view.posAtDOM(anchor, 0);
          const range = activeEditor ? getLinkRange(activeEditor, pos, href) : null;
          if (!range) return false;

          setLinkMenu({
            ...range,
            href,
            position: getRectFloatingPosition(anchor.getBoundingClientRect()),
          });
          return false;
        },
        click: (view, event) => {
          if (!(event.target instanceof Element)) return false;
          const anchor = event.target.closest("a[href]");
          if (!anchor || !view.dom.contains(anchor)) return false;

          event.preventDefault();
          const activeEditor = editorInstanceRef.current;
          const href = anchor.getAttribute("href") || "";
          const pos = view.posAtDOM(anchor, 0);
          const range = activeEditor ? getLinkRange(activeEditor, pos, href) : null;
          const rect = anchor.getBoundingClientRect();

          if (range) {
            setSlash(null);
            setContextMenu(null);
            setSelectionMenu(null);
            setLinkDialog(null);
            setUploadMenu(null);
            setLinkMenu({
              ...range,
              href,
              position: getRectFloatingPosition(rect),
            });
            return true;
          }

          window.open(href, "_blank", "noopener,noreferrer");
          return true;
        },
        contextmenu: (view, event) => {
          if (!showBlockChrome) return false;
          event.preventDefault();
          setSlash(null);
          setBlockMenu(null);
          setSelectionMenu(null);
          setLinkMenu(null);
          setUploadMenu(null);
          setTagDialog(null);
          const activeEditor = editorInstanceRef.current;
          const position = view.posAtCoords({ left: event.clientX, top: event.clientY });
          if (activeEditor && position && view.state.selection.empty) {
            activeEditor.chain().focus().setTextSelection(position.pos).run();
          } else {
            view.focus();
          }
          setContextMenu(
            getContextMenuState(
              activeEditor,
              { top: event.clientY, left: event.clientX },
              !view.state.selection.empty,
            ),
          );
          return true;
        },
        dragover: (_view, event) => {
          if (!draggedBlockRef.current || readOnly || !showBlockChrome) return false;
          event.preventDefault();
          if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
          return true;
        },
        drop: (view, event) => {
          const dragged = draggedBlockRef.current;
          if (!dragged || readOnly || !showBlockChrome) return false;

          const position = view.posAtCoords({ left: event.clientX, top: event.clientY });
          if (!position) return false;

          event.preventDefault();
          draggedBlockRef.current = null;
          setBlockMenu(null);
          const activeEditor = editorInstanceRef.current;
          if (!activeEditor) return true;
          moveBlockToDropTarget(activeEditor, dragged, position.pos, event.clientY);
          return true;
        },
      },
    },
    onCreate: ({ editor }) => {
      editorInstanceRef.current = editor;
      onEditorReady?.(editor);
      // Allow onChange after a short delay (lets initial content settle)
      setTimeout(() => {
        isInitializing.current = false;
      }, 100);
    },
    onDestroy: () => {
      clearActiveBlockMarker();
      editorInstanceRef.current = null;
    },
    onUpdate: ({ editor }) => {
      if (isInitializing.current) return;
      syncActiveBlockMarker(editor);
      updateBlockMenu(editor, blockMenu?.open ?? false);
      updateTableToolbar(editor);
      const html = editor.getHTML();
      const md = htmlToMarkdown(html);
      onChange(md);
    },
    onSelectionUpdate: ({ editor }) => {
      syncActiveBlockMarker(editor);
      updateBlockMenu(editor, blockMenu?.open ?? false);
      updateSelectionMenu(editor);
      updateTableToolbar(editor);
      setContextMenu(null);
    },
    onFocus: ({ editor }) => {
      syncActiveBlockMarker(editor);
      updateBlockMenu(editor, blockMenu?.open ?? false);
      updateTableToolbar(editor);
    },
    onBlur: () => {
      window.setTimeout(() => {
        setBlockMenu(null);
        setSelectionMenu(null);
        setLinkMenu(null);
        setTableToolbarPosition(null);
        clearActiveBlockMarker();
      }, 120);
    },
  });

  useEffect(() => {
    if (!editor || editor.isDestroyed || readOnly) return;

    const handleDocumentKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !isCommandPaletteShortcut(event)) return;
      const target = event.target;
      if (
        target instanceof Element &&
        (target.closest(".starlane-menu-surface") || target.closest("[role='dialog']"))
      ) {
        return;
      }

      event.preventDefault();
      event.stopPropagation();
      editor.chain().focus().run();
      openSearchableCommandPalette(editor);
    };

    document.addEventListener("keydown", handleDocumentKeyDown, true);
    return () => document.removeEventListener("keydown", handleDocumentKeyDown, true);
  }, [editor, openSearchableCommandPalette, readOnly]);

  useEffect(() => {
    if (!showMobileCommandButton) {
      setMobileKeyboardInset(0);
      return;
    }

    let frame = 0;
    let settleTimer: number | null = null;

    const measureKeyboardInset = () => {
      const viewport = window.visualViewport;
      const layoutHeight = Math.max(window.innerHeight, document.documentElement.clientHeight);
      const result = getMobileKeyboardInsetMeasurement(
        {
          layoutHeight,
          visualHeight: viewport?.height ?? layoutHeight,
          visualTop: viewport?.offsetTop ?? 0,
          visualWidth: viewport?.width ?? window.innerWidth,
        },
        {
          baseline: mobileViewportBaselineRef.current,
          width: mobileViewportWidthRef.current,
        },
      );
      mobileViewportBaselineRef.current = result.state.baseline;
      mobileViewportWidthRef.current = result.state.width;
      setMobileKeyboardInset((currentInset) =>
        Math.abs(currentInset - result.inset) > 2 ? result.inset : currentInset,
      );
    };

    const scheduleMeasure = () => {
      if (frame) window.cancelAnimationFrame(frame);
      if (settleTimer) window.clearTimeout(settleTimer);

      frame = window.requestAnimationFrame(() => {
        measureKeyboardInset();
        settleTimer = window.setTimeout(measureKeyboardInset, 48);
      });
    };

    scheduleMeasure();
    window.visualViewport?.addEventListener("resize", scheduleMeasure);
    window.visualViewport?.addEventListener("scroll", scheduleMeasure);
    window.addEventListener("resize", scheduleMeasure);
    window.addEventListener("focusin", scheduleMeasure);
    window.addEventListener("focusout", scheduleMeasure);

    return () => {
      if (frame) window.cancelAnimationFrame(frame);
      if (settleTimer) window.clearTimeout(settleTimer);
      window.visualViewport?.removeEventListener("resize", scheduleMeasure);
      window.visualViewport?.removeEventListener("scroll", scheduleMeasure);
      window.removeEventListener("resize", scheduleMeasure);
      window.removeEventListener("focusin", scheduleMeasure);
      window.removeEventListener("focusout", scheduleMeasure);
    };
  }, [showMobileCommandButton]);

  // Update content from outside (e.g., initial load)
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;

    const currentHtml = editor.getHTML();
    const newHtml = markdownToHtml(content);

    // Only update if the content actually changed (avoid cursor reset)
    if (currentHtml !== newHtml && !editor.isFocused) {
      editor.commands.setContent(newHtml);
    }
  }, [content, editor]);

  // Update editable state
  useEffect(() => {
    if (editor && !editor.isDestroyed) {
      editor.setEditable(!readOnly);
    }
  }, [readOnly, editor]);

  useEffect(() => {
    if (!blockMenu?.isCodeBlock) setLanguageMenuOpen(false);
  }, [blockMenu?.isCodeBlock]);

  if (!editor) return null;

  const activeHeadingLevel =
    ([1, 2, 3, 4, 5, 6] as const).find((level) => editor.isActive("heading", { level })) || "";
  const activeFontFamily = editor.getAttributes("fontFamily").fontFamily || "";
  const activeFontLabel =
    EDITOR_FONT_OPTIONS.find((option) => option.value === activeFontFamily)?.label || "Default";
  const mobileToolbarPlacement = getMobileToolbarPlacement(mobileKeyboardInset);
  const mobileKeyboardOpen = mobileToolbarPlacement.keyboardOpen;
  const mobileToolbarBottom = mobileToolbarPlacement.toolbarBottom;
  const mobileMenuBottom = mobileToolbarPlacement.menuBottom;
  const tableActions = [
    { label: "Add row above", icon: <ArrowUp size={14} />, run: () => editor.chain().focus().addRowBefore().run() },
    { label: "Add row below", icon: <ArrowDown size={14} />, run: () => editor.chain().focus().addRowAfter().run() },
    { label: "Add column left", icon: <ArrowUp size={14} className="rotate-[-90deg]" />, run: () => editor.chain().focus().addColumnBefore().run() },
    { label: "Add column right", icon: <ArrowDown size={14} className="rotate-[-90deg]" />, run: () => editor.chain().focus().addColumnAfter().run() },
    { label: "Delete row", icon: <Rows3 size={14} />, run: () => editor.chain().focus().deleteRow().run() },
    { label: "Delete column", icon: <Columns3 size={14} />, run: () => editor.chain().focus().deleteColumn().run() },
    { label: "Delete table", icon: <Trash2 size={14} />, destructive: true, run: () => editor.chain().focus().deleteTable().run() },
  ];

  const applySelectionHeading = (level: "" | 1 | 2 | 3 | 4 | 5 | 6) => {
    if (level) {
      editor.chain().focus().toggleHeading({ level }).run();
    } else {
      editor.chain().focus().setParagraph().run();
    }
    setSelectionHeadingOpen(false);
    window.setTimeout(() => updateSelectionMenu(editor), 0);
  };

  const applyMobileHeading = (level: "" | 1 | 2 | 3 | 4 | 5 | 6) => {
    if (level) {
      editor.chain().focus().setHeading({ level }).run();
    } else {
      editor.chain().focus().setParagraph().run();
    }
    setMobileHeadingMenuOpen(false);
  };

  const applySelectionFont = (fontFamily: string) => {
    if (fontFamily) {
      editor.chain().focus().setMark("fontFamily", { fontFamily }).run();
    } else {
      editor.chain().focus().unsetMark("fontFamily").run();
    }
    setSelectionFontOpen(false);
    window.setTimeout(() => updateSelectionMenu(editor), 0);
  };

  return (
    <div
      onClick={(event) => {
        const target = event.target as HTMLElement;
        const canvasElement = target.closest("[data-excalidraw-canvas]");
        if (canvasElement instanceof HTMLElement) {
          event.preventDefault();
          void openExcalidrawDialog(canvasElement);
        }
      }}
      onKeyDown={(event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        const target = event.target as HTMLElement;
        const canvasElement = target.closest("[data-excalidraw-canvas]");
        if (canvasElement instanceof HTMLElement) {
          event.preventDefault();
          void openExcalidrawDialog(canvasElement);
        }
      }}
      style={
        {
          "--editor-zoom": cssVar,
          fontSize: `calc(1em * var(--editor-zoom))`,
          paddingBottom: showMobileCommandButton
            ? mobileKeyboardOpen
              ? "3.75rem"
              : "calc(env(safe-area-inset-bottom) + 4.75rem)"
            : undefined,
          scrollPaddingBottom: showMobileCommandButton
            ? mobileKeyboardOpen
              ? "3.75rem"
              : "calc(env(safe-area-inset-bottom) + 4.75rem)"
            : undefined,
        } as React.CSSProperties
      }
    >
      <EditorContent editor={editor} />
      {tableToolbarPosition && !readOnly && (
        <div
          className={showMobileCommandButton ? "fixed left-3 right-3 z-[60]" : "fixed z-[60]"}
          style={
            showMobileCommandButton
              ? { bottom: mobileMenuBottom }
              : { top: tableToolbarPosition.top, left: tableToolbarPosition.left }
          }
          role="toolbar"
          aria-label="Table controls"
        >
          <div className={showMobileCommandButton ? "flex items-center gap-1 overflow-x-auto border-t border-border bg-surface px-2 py-1.5" : "starlane-menu-surface flex flex-wrap items-center gap-1 p-1.5"}>
            {!showMobileCommandButton && <span className="px-1 text-xs font-medium text-text-muted">Table</span>}
            {tableActions.map((action) => (
              <button
                key={action.label}
                type="button"
                title={action.label}
                aria-label={action.label}
                onPointerDown={(event) => event.preventDefault()}
                onClick={() => {
                  action.run();
                  window.requestAnimationFrame(() => updateTableToolbar(editor));
                }}
                className={`rounded-button flex h-8 shrink-0 items-center justify-center gap-1 text-xs transition-colors duration-100 ${
                  showMobileCommandButton ? "w-8" : "px-2"
                } ${
                  action.destructive
                    ? "text-danger hover:bg-danger/10"
                    : "text-text-muted hover:bg-surface-raised hover:text-text"
                }`}
              >
                {action.icon}
                {!showMobileCommandButton && <span className="sm:hidden">{action.label.replace("Add ", "").replace("Delete ", "")}</span>}
              </button>
            ))}
          </div>
        </div>
      )}
      {uploadWarning ? (
        <EditorUploadWarning message={uploadWarning} onDismiss={() => setUploadWarning(null)} />
      ) : null}
      {excalidrawDialog && (
        <div
          className="fixed inset-0 z-[95] flex items-center justify-center bg-black/40 p-4"
          role="dialog"
          aria-modal="true"
          aria-label="Edit Excalidraw canvas"
        >
          <div className="flex h-[min(52rem,calc(100dvh-2rem))] w-[min(96rem,calc(100vw-2rem))] flex-col overflow-hidden rounded-card border border-border bg-surface shadow-card">
            <div className="flex shrink-0 items-center justify-between gap-3 border-b border-border px-3 py-2">
              <div className="min-w-0 flex-1">
                <h2 className="text-sm font-medium text-text">Excalidraw canvas</h2>
                <label className="mt-2 block text-xs text-text-muted" htmlFor="excalidraw-filename">
                  Drawing filename
                </label>
                <Input
                  id="excalidraw-filename"
                  value={excalidrawDialog.filename}
                  onChange={(event) =>
                    setExcalidrawDialog((current) =>
                      current ? { ...current, filename: event.target.value } : current,
                    )
                  }
                  onBlur={() =>
                    setExcalidrawDialog((current) =>
                      current
                        ? { ...current, filename: normalizeExcalidrawFilename(current.filename) }
                        : current,
                    )
                  }
                  disabled={excalidrawSaving}
                  className="mt-1 h-8 w-full max-w-md px-2 text-xs"
                  placeholder="untitled.excalidraw"
                  autoComplete="off"
                />
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  type="button"
                  onClick={() => {
                    if (!excalidrawSaving) setExcalidrawDialog(null);
                  }}
                  disabled={excalidrawSaving}
                  className="rounded-button border border-border px-3 py-1.5 text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={() => void saveExcalidrawDialog()}
                  disabled={excalidrawSaving}
                  className="rounded-button bg-primary px-3 py-1.5 text-sm font-medium text-white transition-opacity duration-100 disabled:opacity-50"
                >
                  {excalidrawSaving ? "Saving…" : "Save"}
                </button>
              </div>
            </div>
            {(excalidrawStatus || excalidrawDialog.fileBacked) && (
              <div className="border-b border-border px-3 py-2 text-xs text-text-muted">
                {excalidrawStatus ??
                  "This drawing is stored as a separate .excalidraw file attached to the document."}
              </div>
            )}
            <div className="min-h-0 flex-1 bg-bg">
              <Suspense
                fallback={
                  <div className="flex h-full items-center justify-center text-sm text-text-muted">
                    Loading drawing editor…
                  </div>
                }
              >
                <LazyExcalidraw
                  initialData={excalidrawDialog.scene}
                  onChange={(elements, appState, files) => {
                    excalidrawDraftRef.current = normalizeExcalidrawScene({
                      elements,
                      appState,
                      files,
                    });
                  }}
                />
              </Suspense>
            </div>
          </div>
        </div>
      )}
      {slashState && (
        <SlashCommandPalette
          query={slashState.query}
          position={slashState.position}
          searchable={slashState.searchable}
          templates={templates}
          templateContext={templateContext}
          onSelect={applySlashCommand}
          onClose={() => setSlash(null)}
        />
      )}
      {selectionMenu && !readOnly && !showMobileCommandButton && (
        <div
          className="starlane-menu-surface fixed z-[70] flex max-w-[calc(100vw-1rem)] items-center gap-1 overflow-visible p-1"
          style={{ top: selectionMenu.position.top, left: selectionMenu.position.left }}
          aria-label="Text formatting"
        >
          {[
            {
              label: "Bold",
              active: editor.isActive("bold"),
              icon: <Bold size={14} />,
              action: () => editor.chain().focus().toggleBold().run(),
            },
            {
              label: "Italic",
              active: editor.isActive("italic"),
              icon: <Italic size={14} />,
              action: () => editor.chain().focus().toggleItalic().run(),
            },
            {
              label: "Underline",
              active: editor.isActive("underline"),
              icon: <UnderlineIcon size={14} />,
              action: () => editor.chain().focus().toggleUnderline().run(),
            },
            {
              label: "Strikethrough",
              active: editor.isActive("strike"),
              icon: <Strikethrough size={14} />,
              action: () => editor.chain().focus().toggleStrike().run(),
            },
            {
              label: "Inline code",
              active: editor.isActive("code"),
              icon: <Code size={14} />,
              action: () => editor.chain().focus().toggleCode().run(),
            },
            {
              label: "Highlight",
              active: editor.isActive("highlight"),
              icon: <Highlighter size={14} />,
              action: () => editor.chain().focus().toggleHighlight().run(),
            },
            {
              label: "Bullet list",
              active: editor.isActive("bulletList"),
              icon: <List size={14} />,
              action: () => editor.chain().focus().toggleBulletList().run(),
            },
            {
              label: "Numbered list",
              active: editor.isActive("orderedList"),
              icon: <ListOrdered size={14} />,
              action: () => editor.chain().focus().toggleOrderedList().run(),
            },
            {
              label: "Quote",
              active: editor.isActive("blockquote"),
              icon: <Quote size={14} />,
              action: () => editor.chain().focus().toggleBlockquote().run(),
            },
          ].map((item) => (
            <button
              key={item.label}
              type="button"
              title={item.label}
              aria-label={item.label}
              aria-pressed={item.active}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => {
                item.action();
                window.setTimeout(() => updateSelectionMenu(editor), 0);
              }}
              className={`rounded-button flex h-7 w-7 items-center justify-center transition-colors duration-100 ${
                item.active
                  ? "bg-primary/15 text-primary"
                  : "text-text-muted hover:bg-surface-raised hover:text-text"
              }`}
            >
              {item.icon}
            </button>
          ))}
          <div className="bg-border mx-1 h-5 w-px" />
          <div className="relative" ref={selectionHeadingRef}>
            <button
              type="button"
              title="Heading"
              aria-label="Heading"
              aria-haspopup="menu"
              aria-expanded={selectionHeadingOpen}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => {
                setSelectionFontOpen(false);
                setSelectionHeadingOpen((prev) => !prev);
              }}
              className="rounded-button border-border bg-bg text-text-muted hover:bg-surface-raised hover:text-text flex h-7 min-w-16 items-center justify-between gap-1 border px-2 text-xs"
            >
              <span>{activeHeadingLevel ? `H${activeHeadingLevel}` : "Text"}</span>
              <ArrowDown size={12} />
            </button>
            {selectionHeadingOpen && (
              <div
                className="starlane-menu-surface absolute left-0 top-8 z-[90] w-32 p-1"
                role="menu"
                aria-label="Heading"
              >
                {[
                  { value: "", label: "Text" },
                  { value: 1, label: "Heading 1" },
                  { value: 2, label: "Heading 2" },
                  { value: 3, label: "Heading 3" },
                  { value: 4, label: "Heading 4" },
                  { value: 5, label: "Heading 5" },
                  { value: 6, label: "Heading 6" },
                ].map((option) => (
                  <button
                    key={option.label}
                    type="button"
                    role="menuitemradio"
                    aria-checked={activeHeadingLevel === option.value}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() =>
                      applySelectionHeading(option.value as "" | 1 | 2 | 3 | 4 | 5 | 6)
                    }
                    className={`rounded-button flex w-full items-center justify-between px-2 py-1.5 text-left text-xs ${
                      activeHeadingLevel === option.value
                        ? "bg-primary/15 text-primary"
                        : "text-text-muted hover:bg-surface-raised hover:text-text"
                    }`}
                  >
                    <span>{option.label}</span>
                    {activeHeadingLevel === option.value && <Check size={12} />}
                  </button>
                ))}
              </div>
            )}
          </div>
          <div className="relative" ref={selectionFontRef}>
            <button
              type="button"
              title="Font"
              aria-label="Font"
              aria-haspopup="menu"
              aria-expanded={selectionFontOpen}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => {
                setSelectionHeadingOpen(false);
                setSelectionFontOpen((prev) => !prev);
              }}
              className="rounded-button border-border bg-bg text-text-muted hover:bg-surface-raised hover:text-text flex h-7 min-w-24 max-w-32 items-center justify-between gap-1 border px-2 text-xs"
            >
              <span className="truncate">{activeFontLabel}</span>
              <ArrowDown size={12} />
            </button>
            {selectionFontOpen && (
              <div
                className="starlane-menu-surface absolute left-0 top-8 z-[90] w-44 p-1"
                role="menu"
                aria-label="Font"
              >
                {EDITOR_FONT_OPTIONS.map((option) => (
                  <button
                    key={option.value || "default"}
                    type="button"
                    role="menuitemradio"
                    aria-checked={activeFontFamily === option.value}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => applySelectionFont(option.value)}
                    className={`rounded-button flex w-full items-center justify-between px-2 py-1.5 text-left text-xs ${
                      activeFontFamily === option.value
                        ? "bg-primary/15 text-primary"
                        : "text-text-muted hover:bg-surface-raised hover:text-text"
                    }`}
                  >
                    <span className="truncate">{option.label}</span>
                    {activeFontFamily === option.value && <Check size={12} />}
                  </button>
                ))}
              </div>
            )}
          </div>
          <div className="bg-border mx-1 h-5 w-px" />
          <button
            type="button"
            title="Link"
            aria-label="Link"
            aria-pressed={editor.isActive("link")}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => openLinkDialog(editor)}
            className={`rounded-button flex h-7 w-7 items-center justify-center transition-colors duration-100 ${
              editor.isActive("link")
                ? "bg-primary/15 text-primary"
                : "text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
          >
            <LinkIcon size={14} />
          </button>
          <button
            type="button"
            title="Code block"
            aria-label="Code block"
            aria-pressed={editor.isActive("codeBlock")}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => {
              editor.chain().focus().toggleCodeBlock().run();
              window.setTimeout(() => updateSelectionMenu(editor), 0);
            }}
            className={`rounded-button flex h-7 w-7 items-center justify-center transition-colors duration-100 ${
              editor.isActive("codeBlock")
                ? "bg-primary/15 text-primary"
                : "text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
          >
            <FileCode size={14} />
          </button>
          <button
            type="button"
            title="Upload"
            aria-label="Upload"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => openUploadMenu(true, editor)}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
          >
            <Upload size={14} />
          </button>
          <button
            type="button"
            title="Indent"
            aria-label="Indent"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => {
              editor.chain().focus().sinkListItem("listItem").run();
              window.setTimeout(() => updateSelectionMenu(editor), 0);
            }}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
          >
            <IndentIncrease size={14} />
          </button>
          <button
            type="button"
            title="Outdent"
            aria-label="Outdent"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => {
              editor.chain().focus().liftListItem("listItem").run();
              window.setTimeout(() => updateSelectionMenu(editor), 0);
            }}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
          >
            <IndentDecrease size={14} />
          </button>
          <button
            type="button"
            title="Clear formatting"
            aria-label="Clear formatting"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => {
              editor.chain().focus().unsetAllMarks().clearNodes().run();
              window.setTimeout(() => updateSelectionMenu(editor), 0);
            }}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
          >
            <RemoveFormatting size={14} />
          </button>
          <button
            type="button"
            title="Command menu"
            aria-label="Command menu"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => openSearchableCommandPalette(editor)}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
          >
            <Command size={14} />
          </button>
        </div>
      )}
      {uploadMenu && !readOnly && (
        <>
          <button
            type="button"
            aria-label="Close upload menu"
            className="fixed inset-0 z-[75] cursor-default bg-transparent"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setUploadMenu(null)}
          />
          <div
            className="starlane-menu-surface fixed z-[80] w-[min(20rem,calc(100vw-1rem))] overflow-hidden p-0"
            style={{ top: uploadMenu.position.top, left: uploadMenu.position.left }}
            role="dialog"
            aria-label="Insert file or image"
          >
            <div className="border-border flex items-center gap-2 border-b px-3">
              <Search size={16} className="text-text-muted shrink-0" />
              <span className="text-text-muted flex h-10 flex-1 items-center text-sm">
                Insert file or image
              </span>
            </div>
            <div className="p-1">
              {[
                {
                  id: "image",
                  label: "Choose image",
                  description: `Insert an image asset up to ${maxPickedFileMb} MB`,
                  icon: <ImageIcon size={16} />,
                  preferImage: true,
                },
                {
                  id: "file",
                  label: "Choose file",
                  description: `Attach a vault file up to ${maxPickedFileMb} MB`,
                  icon: <FileText size={16} />,
                  preferImage: false,
                },
              ]
                .sort(
                  (a, b) =>
                    Number(b.preferImage === uploadMenu.preferImage) -
                    Number(a.preferImage === uploadMenu.preferImage),
                )
                .map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => chooseUploadSource(item.preferImage)}
                    className="rounded-button text-text-muted hover:bg-surface hover:text-text flex w-full items-center gap-3 px-3 py-2 text-left transition-colors duration-100"
                  >
                    <span className="text-text-muted flex h-7 w-7 shrink-0 items-center justify-center">
                      {item.icon}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm">{item.label}</span>
                      <span className="text-text-muted block truncate text-xs">
                        {item.description}
                      </span>
                    </span>
                  </button>
                ))}
            </div>
            <div className="border-border text-text-muted flex items-center gap-2 border-t px-3 py-1.5 text-[10px]">
              <Upload size={12} />
              <span>Native picker opens after selection</span>
            </div>
          </div>
        </>
      )}
      {tagDialog && !readOnly && (
        <>
          <button
            type="button"
            aria-label="Close tag dialog"
            className="fixed inset-0 z-[75] cursor-default"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setTagDialog(null)}
          />
          <div
            role="dialog"
            aria-label="Insert tag"
            className="starlane-menu-surface fixed z-[80] w-72 space-y-3 p-3"
            style={{ top: tagDialog.position.top, left: tagDialog.position.left }}
          >
            <div>
              <label htmlFor="editor-tag-name" className="text-text-muted mb-1 block text-[11px]">
                Tag
              </label>
              <Input
                id="editor-tag-name"
                value={tagDialog.text}
                onChange={(event) =>
                  setTagDialog((prev) => (prev ? { ...prev, text: event.target.value } : prev))
                }
                onKeyDown={(event) => {
                  if (event.key === "Enter") applyTagDialog();
                  if (event.key === "Escape") setTagDialog(null);
                }}
                className="h-8 px-2 text-xs"
                placeholder="project-notes"
                autoFocus
              />
              <p className="text-text-muted mt-1 text-[10px]">
                Inserts as #{normalizeTagName(tagDialog.text || "tag")}
              </p>
            </div>
            <div className="flex justify-end gap-1">
              <button
                type="button"
                title="Cancel"
                aria-label="Cancel"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => setTagDialog(null)}
                className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 items-center justify-center transition-colors duration-100"
              >
                <X size={14} />
              </button>
              <button
                type="button"
                title="Insert tag"
                aria-label="Insert tag"
                onMouseDown={(event) => event.preventDefault()}
                onClick={applyTagDialog}
                className="rounded-button bg-primary text-on-primary flex h-8 w-8 items-center justify-center transition-opacity duration-100 hover:opacity-90"
              >
                <Check size={14} />
              </button>
            </div>
          </div>
        </>
      )}
      {linkMenu && (
        <>
          <button
            type="button"
            aria-label="Close link menu"
            className="fixed inset-0 z-[65] cursor-default"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setLinkMenu(null)}
          />
          <div
            className="starlane-menu-surface fixed z-[70] flex max-w-[min(20rem,calc(100vw-1rem))] items-center gap-1 p-1"
            style={{ top: linkMenu.position.top, left: linkMenu.position.left }}
            aria-label="Link actions"
          >
            <span className="text-text-muted max-w-44 truncate px-2 text-xs">{linkMenu.href}</span>
            <button
              type="button"
              title="Open link"
              aria-label="Open link"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => {
                window.open(linkMenu.href, "_blank", "noopener,noreferrer");
                setLinkMenu(null);
              }}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
            >
              <ExternalLink size={14} />
            </button>
            {!readOnly && (
              <>
                <button
                  type="button"
                  title="Edit link"
                  aria-label="Edit link"
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => openLinkDialog(editor, linkMenu)}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
                >
                  <Pencil size={14} />
                </button>
                <button
                  type="button"
                  title="Remove link"
                  aria-label="Remove link"
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => removeLink(linkMenu)}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
                >
                  <Unlink size={14} />
                </button>
              </>
            )}
          </div>
        </>
      )}
      {linkDialog && (
        <>
          <button
            type="button"
            aria-label="Close link dialog"
            className="fixed inset-0 z-[75] cursor-default"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setLinkDialog(null)}
          />
          <div
            role="dialog"
            aria-label="Edit link"
            className="starlane-menu-surface fixed z-[80] w-72 space-y-3 p-3"
            style={{ top: linkDialog.position.top, left: linkDialog.position.left }}
          >
            <div>
              <label htmlFor="editor-link-text" className="text-text-muted mb-1 block text-[11px]">
                Display text
              </label>
              <Input
                id="editor-link-text"
                value={linkDialog.text}
                onChange={(event) =>
                  setLinkDialog((prev) => (prev ? { ...prev, text: event.target.value } : prev))
                }
                className="h-8 px-2 text-xs"
                autoFocus
              />
            </div>
            <div>
              <label htmlFor="editor-link-url" className="text-text-muted mb-1 block text-[11px]">
                URL
              </label>
              <Input
                id="editor-link-url"
                value={linkDialog.href}
                onChange={(event) =>
                  setLinkDialog((prev) => (prev ? { ...prev, href: event.target.value } : prev))
                }
                onKeyDown={(event) => {
                  if (event.key === "Enter") applyLinkDialog();
                  if (event.key === "Escape") setLinkDialog(null);
                }}
                className="h-8 px-2 text-xs"
                placeholder="https://example.com"
              />
            </div>
            <div className="flex justify-end gap-1">
              <button
                type="button"
                title="Cancel"
                aria-label="Cancel"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => setLinkDialog(null)}
                className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 items-center justify-center transition-colors duration-100"
              >
                <X size={14} />
              </button>
              <button
                type="button"
                title="Apply"
                aria-label="Apply"
                onMouseDown={(event) => event.preventDefault()}
                onClick={applyLinkDialog}
                className="rounded-button bg-primary text-on-primary flex h-8 w-8 items-center justify-center transition-opacity duration-100 hover:opacity-90"
              >
                <Check size={14} />
              </button>
            </div>
          </div>
        </>
      )}
      {bookmarkDialog && (
        <>
          <button
            type="button"
            aria-label="Close bookmark dialog"
            className="fixed inset-0 z-[75] cursor-default"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setBookmarkDialog(null)}
          />
          <div
            role="dialog"
            aria-label="Insert bookmark"
            className="starlane-menu-surface fixed z-[80] w-[min(20rem,calc(100vw-1.5rem))] space-y-3 p-3"
            style={{ top: bookmarkDialog.position.top, left: bookmarkDialog.position.left }}
          >
            <div>
              <label htmlFor="editor-bookmark-url" className="text-text-muted mb-1 block text-[11px]">
                Link URL
              </label>
              <Input
                id="editor-bookmark-url"
                value={bookmarkDialog.href}
                onChange={(event) =>
                  setBookmarkDialog((current) => {
                    if (!current) return current;
                    const href = event.target.value;
                    return {
                      ...current,
                      href,
                      title: current.title || getBookmarkDomain(normalizeLinkHref(href)),
                    };
                  })
                }
                className="h-8 px-2 text-xs"
                placeholder="https://example.com"
                autoFocus
              />
              <button
                type="button"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => {
                  const href = bookmarkDialog.href.trim();
                  if (!href) return;
                  setBookmarkDialog((current) => current ? { ...current, loading: true, error: undefined } : current);
                  void dataClient.fetchBookmarkMetadata(href).then((metadata) => {
                    setBookmarkDialog((current) => current ? { ...current, href: metadata.url, title: current.title || metadata.title, description: current.description || metadata.description || "", loading: false } : current);
                  }).catch((error: unknown) => {
                    setBookmarkDialog((current) => current ? { ...current, loading: false, error: error instanceof Error ? error.message : "Could not fetch bookmark metadata" } : current);
                  });
                }}
                disabled={!bookmarkDialog.href.trim() || bookmarkDialog.loading}
                className="rounded-button mt-2 border border-border px-2 py-1 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text disabled:opacity-40"
              >
                {bookmarkDialog.loading ? "Fetching preview…" : "Fetch preview"}
              </button>
            </div>
            <div>
              <label htmlFor="editor-bookmark-title" className="text-text-muted mb-1 block text-[11px]">
                Title
              </label>
              <Input
                id="editor-bookmark-title"
                value={bookmarkDialog.title}
                onChange={(event) =>
                  setBookmarkDialog((current) => current ? { ...current, title: event.target.value } : current)
                }
                className="h-8 px-2 text-xs"
                placeholder="Example site"
              />
            </div>
            <div>
              <label htmlFor="editor-bookmark-description" className="text-text-muted mb-1 block text-[11px]">
                Description <span className="font-normal">(optional)</span>
              </label>
              <Input
                id="editor-bookmark-description"
                value={bookmarkDialog.description}
                onChange={(event) =>
                  setBookmarkDialog((current) => current ? { ...current, description: event.target.value } : current)
                }
                onKeyDown={(event) => {
                  if (event.key === "Enter") applyBookmarkDialog();
                  if (event.key === "Escape") setBookmarkDialog(null);
                }}
                className="h-8 px-2 text-xs"
                placeholder="A short note about this link"
              />
            </div>
            <p className="text-text-muted text-[10px]">Bookmark details stay local to the page; the editor does not fetch remote link metadata.</p>
            {bookmarkDialog.error && <p className="text-danger text-xs">{bookmarkDialog.error}</p>}
            <div className="flex justify-end gap-1">
              <button
                type="button"
                title="Cancel"
                aria-label="Cancel"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => setBookmarkDialog(null)}
                className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 items-center justify-center transition-colors duration-100"
              >
                <X size={14} />
              </button>
              <button
                type="button"
                title="Insert bookmark"
                aria-label="Insert bookmark"
                onMouseDown={(event) => event.preventDefault()}
                onClick={applyBookmarkDialog}
                disabled={!normalizeLinkHref(bookmarkDialog.href)}
                className="rounded-button bg-primary text-on-primary flex h-8 w-8 items-center justify-center transition-opacity duration-100 hover:opacity-90 disabled:opacity-40"
              >
                <Check size={14} />
              </button>
            </div>
          </div>
        </>
      )}
      {wikiLinkDialog && (
        <>
          <button
            type="button"
            aria-label="Close wiki link dialog"
            className="fixed inset-0 z-[75] cursor-default"
            onClick={() => setWikiLinkDialog(null)}
          />
          <div
            role="dialog"
            aria-label="Link to document"
            className="starlane-menu-surface fixed z-[80] w-72 space-y-2 p-3"
            style={{ top: wikiLinkDialog.position.top, left: wikiLinkDialog.position.left }}
          >
            <label htmlFor="editor-wiki-link-search" className="text-text-muted block text-[11px]">
              Link to document
            </label>
            <Input
              id="editor-wiki-link-search"
              value={wikiLinkDialog.query}
              onChange={(event) => setWikiLinkDialog((current) => current ? { ...current, query: event.target.value } : current)}
              className="h-8 px-2 text-xs"
              placeholder="Search documents"
              autoFocus
            />
            <div className="max-h-40 overflow-y-auto">
              {wikiLinkDialog.loading ? (
                <p className="px-2 py-1 text-xs text-text-muted">Loading documents...</p>
              ) : wikiLinkDialog.error ? (
                <p className="px-2 py-1 text-xs text-text-muted">Unable to load documents.</p>
              ) : (
                wikiLinkDialog.documents
                  .filter((document) => document.title.toLowerCase().includes(wikiLinkDialog.query.trim().toLowerCase()))
                  .map((document) => (
                    <button
                      key={document.id}
                      type="button"
                      className="w-full rounded-input px-2 py-1.5 text-left text-sm text-text hover:bg-surface-raised hover:text-primary"
                      onClick={() => insertWikiLink(document)}
                    >
                      {document.title}
                    </button>
                  ))
              )}
            </div>
          </div>
        </>
      )}
      {mentionDialog && (
        <><button type="button" aria-label="Close mention dialog" className="fixed inset-0 z-[75] cursor-default" onClick={() => setMentionDialog(null)} /><div role="dialog" aria-label="Mention a member" className="starlane-menu-surface fixed z-[80] w-72 space-y-2 p-3" style={{ top: mentionDialog.position.top, left: mentionDialog.position.left }}><label className="text-text-muted block text-[11px]">Mention a member</label><Input value={mentionDialog.query} onChange={(event) => setMentionDialog((current) => current ? { ...current, query: event.target.value } : current)} className="h-8 px-2 text-xs" placeholder="Search members" autoFocus /><div className="max-h-40 overflow-y-auto">{mentionDialog.loading ? <p className="px-2 py-1 text-xs text-text-muted">Loading members...</p> : mentionDialog.error ? <p className="px-2 py-1 text-xs text-text-muted">Unable to load members.</p> : mentionDialog.members.filter((member) => `${member.display_name} ${member.email}`.toLowerCase().includes(mentionDialog.query.trim().toLowerCase())).map((member) => <button key={member.user_id} type="button" className="w-full rounded-input px-2 py-1.5 text-left text-sm text-text hover:bg-surface-raised hover:text-primary" onClick={() => insertMention(member)}>{member.display_name}<span className="ml-2 text-xs text-text-muted">{member.email}</span></button>)}</div></div></>
      )}
      {showMobileCommandButton && !readOnly && (
        <div
          className={`${MOBILE_EDITOR_TOOLBAR_BASE_CLASS} ${
            mobileKeyboardOpen
              ? MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS
              : MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS
          }`}
          style={{ bottom: mobileToolbarBottom }}
          aria-label="Mobile editor toolbar"
        >
          <div
            className={`${MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS} ${
              mobileKeyboardOpen ? "rounded-input" : "rounded-button shadow-card"
            }`}
          >
            {mobileToolbarActions.map((action) => {
              const meta = getMobileToolbarAction(action, editor);
              return (
                <button
                  key={action}
                  type="button"
                  title={meta.label}
                  aria-label={meta.label}
                  aria-pressed={meta.active}
                  onPointerDown={(event) => event.preventDefault()}
                  onClick={() => runToolbarAction(action)}
                  className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                    meta.active
                      ? "bg-primary/15 text-primary"
                      : "text-text-muted active:bg-surface-raised hover:bg-surface-raised hover:text-text"
                  }`}
                >
                  {meta.icon}
                </button>
              );
            })}
          </div>
          <button
            type="button"
            title="Customize toolbar"
            aria-label="Customize toolbar"
            onPointerDown={(event) => event.preventDefault()}
            onClick={() => setMobileToolbarCustomizing(true)}
            className={MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS}
          >
            <Settings2 size={16} />
          </button>
          <button
            type="button"
            title="Open command menu"
            aria-label="Open command menu"
            onPointerDown={(event) => event.preventDefault()}
            onClick={() => {
              editor.chain().focus().run();
              window.setTimeout(() => openSearchableCommandPalette(editor), 0);
            }}
            className={MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS}
          >
            <Command size={16} />
          </button>
        </div>
      )}
      {showMobileCommandButton && mobileHeadingMenuOpen && !readOnly && (
        <>
          <button
            type="button"
            aria-label="Close heading menu"
            className="fixed inset-0 z-[75] cursor-default"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setMobileHeadingMenuOpen(false)}
          />
          <div
            className="starlane-menu-surface fixed right-3 z-[80] w-44 p-1"
            style={{ bottom: mobileMenuBottom }}
            role="menu"
            aria-label="Heading level"
          >
            {[
              { value: "", label: "Text" },
              { value: 1, label: "Heading 1" },
              { value: 2, label: "Heading 2" },
              { value: 3, label: "Heading 3" },
              { value: 4, label: "Heading 4" },
              { value: 5, label: "Heading 5" },
              { value: 6, label: "Heading 6" },
            ].map((option) => (
              <button
                key={option.label}
                type="button"
                role="menuitemradio"
                aria-checked={activeHeadingLevel === option.value}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => applyMobileHeading(option.value as "" | 1 | 2 | 3 | 4 | 5 | 6)}
                className={`rounded-button flex w-full items-center justify-between px-2 py-2 text-left text-sm ${
                  activeHeadingLevel === option.value
                    ? "bg-primary/15 text-primary"
                    : "text-text-muted hover:bg-surface-raised hover:text-text"
                }`}
              >
                <span>{option.label}</span>
                {activeHeadingLevel === option.value && <Check size={13} />}
              </button>
            ))}
          </div>
        </>
      )}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        className="hidden"
        onChange={(event) => {
          void handlePickedFiles(event.target.files);
          event.target.value = "";
        }}
      />
      <input
        ref={imageInputRef}
        type="file"
        accept="image/*"
        multiple
        className="hidden"
        onChange={(event) => {
          void handlePickedFiles(event.target.files, true);
          event.target.value = "";
        }}
      />
      {showMobileCommandButton && mobileToolbarCustomizing && !readOnly && (
        <>
          <button
            type="button"
            aria-label="Close toolbar customization"
            className="fixed inset-0 z-[75] cursor-default"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setMobileToolbarCustomizing(false)}
          />
          <div
            role="dialog"
            aria-label="Customize mobile toolbar"
            className="starlane-menu-surface fixed left-3 right-3 z-[80] max-h-[min(28rem,calc(100vh-7rem))] overflow-hidden p-0"
            style={{ bottom: mobileMenuBottom }}
          >
            <div className="border-border flex items-center justify-between border-b px-3 py-2">
              <div>
                <div className="text-text text-sm font-medium">Toolbar</div>
                <div className="text-text-muted text-xs">Choose quick actions</div>
              </div>
              <button
                type="button"
                title="Done"
                aria-label="Done"
                onClick={() => setMobileToolbarCustomizing(false)}
                className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 items-center justify-center"
              >
                <Check size={15} />
              </button>
            </div>
            <div className="scrollbar-none max-h-80 overflow-y-auto p-2">
              <div className="text-text-muted px-2 pb-1 text-[10px] uppercase tracking-wide">
                Current actions
              </div>
              <div className="space-y-1">
                {mobileToolbarActions.map((action, index) => {
                  const meta = getMobileToolbarAction(action, editor);
                  return (
                    <div
                      key={action}
                      draggable
                      onDragStart={() => {
                        draggedToolbarActionRef.current = action;
                      }}
                      onDragOver={(event) => event.preventDefault()}
                      onDrop={() => {
                        reorderMobileToolbarAction(draggedToolbarActionRef.current, action);
                        draggedToolbarActionRef.current = null;
                      }}
                      onDragEnd={() => {
                        draggedToolbarActionRef.current = null;
                      }}
                      className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center gap-2 px-2 py-2 text-sm"
                    >
                      <span className="text-text-muted flex h-7 w-7 cursor-grab items-center justify-center">
                        <GripVertical size={14} />
                      </span>
                      <span className="flex h-7 w-7 items-center justify-center">{meta.icon}</span>
                      <span className="min-w-0 flex-1 truncate">{meta.label}</span>
                      <button
                        type="button"
                        title="Move up"
                        aria-label={`Move ${meta.label} up`}
                        disabled={index === 0}
                        onClick={() => moveMobileToolbarAction(action, "up")}
                        className="rounded-button text-text-muted hover:bg-surface hover:text-text flex h-7 w-7 items-center justify-center disabled:opacity-30"
                      >
                        <ArrowUp size={13} />
                      </button>
                      <button
                        type="button"
                        title="Move down"
                        aria-label={`Move ${meta.label} down`}
                        disabled={index === mobileToolbarActions.length - 1}
                        onClick={() => moveMobileToolbarAction(action, "down")}
                        className="rounded-button text-text-muted hover:bg-surface hover:text-text flex h-7 w-7 items-center justify-center disabled:opacity-30"
                      >
                        <ArrowDown size={13} />
                      </button>
                      <button
                        type="button"
                        title="Remove"
                        aria-label={`Remove ${meta.label}`}
                        onClick={() => toggleMobileToolbarAction(action)}
                        className="rounded-button text-text-muted hover:bg-surface hover:text-text flex h-7 w-7 items-center justify-center"
                      >
                        <X size={13} />
                      </button>
                    </div>
                  );
                })}
              </div>
              <div className="text-text-muted mt-3 px-2 pb-1 text-[10px] uppercase tracking-wide">
                Available actions
              </div>
              <div className="space-y-1">
                {ALL_MOBILE_TOOLBAR_ACTIONS.filter(
                  (action) => action !== "commands" && !mobileToolbarActions.includes(action),
                ).map((action) => {
                  const meta = getMobileToolbarAction(action, editor);
                  return (
                    <button
                      key={action}
                      type="button"
                      onClick={() => toggleMobileToolbarAction(action)}
                      className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center gap-3 px-2 py-2 text-left text-sm"
                    >
                      <span className="border-border flex h-5 w-5 items-center justify-center rounded-full border">
                        <Plus size={12} />
                      </span>
                      <span className="flex h-7 w-7 items-center justify-center">{meta.icon}</span>
                      <span>{meta.label}</span>
                    </button>
                  );
                })}
              </div>
            </div>
          </div>
        </>
      )}
      {blockMenu && (
        <div
          className="fixed z-40 flex items-start gap-1"
          style={{ top: blockMenu.position.top, left: blockMenu.position.left }}
        >
          <button
            type="button"
            draggable={!readOnly}
            title="Drag block or open actions"
            onMouseDown={(event) => event.preventDefault()}
            onDragStart={(event) => {
              const block = getCurrentTopLevelBlock(editor);
              if (!block) return;
              draggedBlockRef.current = block;
              event.dataTransfer.effectAllowed = "move";
              event.dataTransfer.setData("application/x-starlane-block", block.index.toString());
              setBlockMenu((prev) => (prev ? { ...prev, open: false } : prev));
            }}
            onDragEnd={() => {
              draggedBlockRef.current = null;
            }}
            onClick={() => setBlockMenu((prev) => (prev ? { ...prev, open: !prev.open } : prev))}
            className="rounded-button border-border bg-surface text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 cursor-grab items-center justify-center border opacity-80 shadow-sm transition-colors duration-100 focus:opacity-100 active:cursor-grabbing"
            aria-label="Block actions"
          >
            <GripVertical size={13} />
          </button>
          {blockMenu.open && (
            <>
              <button
                type="button"
                aria-label="Close block menu"
                className="fixed inset-0 -z-10 cursor-default"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => setBlockMenu((prev) => (prev ? { ...prev, open: false } : prev))}
              />
              <div className="starlane-menu-surface w-44">
                {[
                  ["Command menu...", () => openSearchableCommandPalette(editor)],
                  ["Move up", () => moveCurrentBlock(editor, "up"), blockMenu.canMoveUp],
                  ["Move down", () => moveCurrentBlock(editor, "down"), blockMenu.canMoveDown],
                  ["Duplicate block", () => duplicateCurrentBlock(editor)],
                  ["Delete block", () => deleteCurrentBlock(editor)],
                ].map(([label, action, enabled = true]) => (
                  <button
                    key={label as string}
                    type="button"
                    onMouseDown={(event) => event.preventDefault()}
                    disabled={!enabled}
                    onClick={() => {
                      (action as () => void)();
                      setBlockMenu((prev) => (prev ? { ...prev, open: false } : prev));
                    }}
                    className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                  >
                    {label as string}
                  </button>
                ))}
              </div>
            </>
          )}
        </div>
      )}
      {blockMenu?.isCodeBlock && (
        <div
          className="fixed z-[60]"
          style={{ top: blockMenu.helperPosition.top, left: blockMenu.helperPosition.left }}
        >
          <button
            type="button"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setLanguageMenuOpen((current) => !current)}
            className="rounded-button border-border bg-surface text-text-muted hover:bg-surface-raised hover:text-text shadow-card flex h-7 items-center gap-2 border px-2 text-xs transition-colors duration-100"
            aria-haspopup="menu"
            aria-expanded={languageMenuOpen}
          >
            {CODE_LANGUAGE_OPTIONS.find((option) => option.value === blockMenu.codeLanguage)
              ?.label ?? "Language"}
            <span aria-hidden="true">⌄</span>
          </button>
          {languageMenuOpen && (
            <>
              <button
                type="button"
                aria-label="Close language menu"
                className="fixed inset-0 -z-10 cursor-default"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => setLanguageMenuOpen(false)}
              />
              <div
                role="menu"
                className="starlane-menu-surface scrollbar-none mt-1 max-h-64 w-44 overflow-y-auto"
              >
                {CODE_LANGUAGE_OPTIONS.map((option) => (
                  <button
                    key={option.value || "plain"}
                    type="button"
                    role="menuitemradio"
                    aria-checked={blockMenu.codeLanguage === option.value}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => {
                      editor
                        .chain()
                        .focus()
                        .updateAttributes("codeBlock", { language: option.value })
                        .run();
                      setLanguageMenuOpen(false);
                      window.setTimeout(() => updateBlockMenu(editor, blockMenu.open), 0);
                    }}
                    className={`rounded-input block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 ${
                      blockMenu.codeLanguage === option.value
                        ? "bg-surface text-text"
                        : "text-text-muted hover:bg-surface hover:text-text"
                    }`}
                  >
                    {option.label}
                  </button>
                ))}
              </div>
            </>
          )}
        </div>
      )}
      {contextMenu && (
        <>
          <button
            type="button"
            aria-label="Close editor context menu"
            className="fixed inset-0 z-40 cursor-default"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setContextMenu(null)}
          />
          <div
            role="menu"
            aria-label="Editor context menu"
            className="starlane-menu-surface fixed z-50 w-48"
            style={{ top: contextMenu.position.top, left: contextMenu.position.left }}
          >
            <button
              type="button"
              role="menuitem"
              disabled={!contextMenu.hasSelection}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => void copySelection("text")}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
            >
              Copy
            </button>
            <button
              type="button"
              role="menuitem"
              disabled={!contextMenu.hasSelection}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => void copySelection("markdown")}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
            >
              Copy as Markdown
            </button>
            {contextMenu.hasSelection && (
              <>
                <div className="bg-border my-1 h-px" />
                {[
                  ["Bold", () => editor.chain().focus().toggleBold().run()],
                  ["Italic", () => editor.chain().focus().toggleItalic().run()],
                  ["Inline code", () => editor.chain().focus().toggleCode().run()],
                  ["Highlight", () => editor.chain().focus().toggleHighlight().run()],
                  ["Link...", () => openLinkDialog(editor)],
                ].map(([label, action]) => (
                  <button
                    key={label as string}
                    type="button"
                    role="menuitem"
                    disabled={readOnly}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => {
                      (action as () => void)();
                      setContextMenu(null);
                    }}
                    className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                  >
                    {label as string}
                  </button>
                ))}
              </>
            )}
            <div className="bg-border my-1 h-px" />
            {contextMenu.inTable && (
              <>
                <button
                  type="button"
                  role="menuitem"
                  disabled={readOnly}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    editor.chain().focus().addRowAfter().run();
                    setContextMenu(null);
                  }}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                >
                  Add row below
                </button>
                <button
                  type="button"
                  role="menuitem"
                  disabled={readOnly}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    editor.chain().focus().addRowBefore().run();
                    setContextMenu(null);
                  }}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                >
                  Add row above
                </button>
                <button
                  type="button"
                  role="menuitem"
                  disabled={readOnly}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    editor.chain().focus().addColumnAfter().run();
                    setContextMenu(null);
                  }}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                >
                  Add column right
                </button>
                <button
                  type="button"
                  role="menuitem"
                  disabled={readOnly}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    editor.chain().focus().addColumnBefore().run();
                    setContextMenu(null);
                  }}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                >
                  Add column left
                </button>
                <button
                  type="button"
                  role="menuitem"
                  disabled={readOnly}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    editor.chain().focus().deleteRow().run();
                    setContextMenu(null);
                  }}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                >
                  Delete row
                </button>
                <button
                  type="button"
                  role="menuitem"
                  disabled={readOnly}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    editor.chain().focus().deleteColumn().run();
                    setContextMenu(null);
                  }}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                >
                  Delete column
                </button>
                <button
                  type="button"
                  role="menuitem"
                  disabled={readOnly}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    editor.chain().focus().deleteTable().run();
                    setContextMenu(null);
                  }}
                  className={EDITOR_DANGER_MENU_ITEM_CLASS}
                >
                  Delete table
                </button>
                <div className="bg-border my-1 h-px" />
              </>
            )}
            {contextMenu.hasBlock && !contextMenu.inTable && (
              <>
                {[
                  ["Command menu...", () => openSearchableCommandPalette(editor)],
                  ["Move up", () => moveCurrentBlock(editor, "up"), contextMenu.canMoveBlockUp],
                  [
                    "Move down",
                    () => moveCurrentBlock(editor, "down"),
                    contextMenu.canMoveBlockDown,
                  ],
                ].map(([label, action, enabled = true]) => (
                  <button
                    key={label as string}
                    type="button"
                    role="menuitem"
                    disabled={readOnly || !enabled}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => {
                      (action as () => void)();
                      setContextMenu(null);
                    }}
                    className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                  >
                    {label as string}
                  </button>
                ))}
                <div className="bg-border my-1 h-px" />
                <button
                  type="button"
                  role="menuitem"
                  disabled={readOnly}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    duplicateCurrentBlock(editor);
                    setContextMenu(null);
                  }}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                >
                  Duplicate block
                </button>
                <button
                  type="button"
                  role="menuitem"
                  disabled={readOnly}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    deleteCurrentBlock(editor);
                    setContextMenu(null);
                  }}
                  className={EDITOR_DANGER_MENU_ITEM_CLASS}
                >
                  Delete block
                </button>
                <div className="bg-border my-1 h-px" />
              </>
            )}
            <button
              type="button"
              role="menuitem"
              disabled={readOnly}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => void pasteClipboard("markdown")}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
            >
              Paste as Markdown
            </button>
            <button
              type="button"
              role="menuitem"
              disabled={readOnly}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => void pasteClipboard("plain")}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
            >
              Paste Plain Text
            </button>
          </div>
        </>
      )}
    </div>
  );
}

/**
 * Simple markdown -> HTML conversion for TipTap.
 * Handles the most common markdown patterns.
 */
export function markdownToHtml(md: string): string {
  if (!md || !md.trim()) return "<p></p>";

  const blocks = splitMarkdownBlocks(md.replace(/\r\n?/g, "\n"));
  const result: string[] = [];

  for (let blockIndex = 0; blockIndex < blocks.length; blockIndex += 1) {
    const rawBlock = blocks[blockIndex];
    const trimmed = rawBlock.trim();
    if (!trimmed) continue;

    const bookmarkMatch = trimmed.match(
      /^\[bookmark:([^\]]+)\]\((https?:\/\/[^\s)]+)(?:\s+"([^"]*)")?\)$/i,
    );
    if (bookmarkMatch) {
      const title = escapeHtmlAttribute(bookmarkMatch[1].trim());
      const href = escapeHtmlAttribute(bookmarkMatch[2]);
      const description = escapeHtmlAttribute(bookmarkMatch[3]?.trim() || "");
      result.push(`<div data-bookmark-card="true" href="${href}" title="${title}" description="${description}"></div>`);
      continue;
    }

    const excalidrawMatch = trimmed.match(/^!\[\[excalidraw:([^\]\n]+)\]\]$/i);
    if (excalidrawMatch) {
      const filename = escapeHtmlAttribute(normalizeExcalidrawFilename(excalidrawMatch[1]));
      const nextBlock = blocks[blockIndex + 1]?.trim() ?? "";
      const sceneMatch = nextBlock.match(/^```excalidraw-json\n([\s\S]*?)\n?```$/i);
      const scene = sceneMatch
        ? writeExcalidrawSceneAttribute(parseExcalidrawSceneJson(sceneMatch[1]))
        : "";
      if (sceneMatch) blockIndex += 1;
      result.push(renderExcalidrawCanvasHtml(filename, scene));
      continue;
    }

    // Code block. Keep raw text until now so blank lines and indentation survive.
    const codeMatch = trimmed.match(/^```([\w-]*)\n([\s\S]*?)\n?```$/);
    if (codeMatch) {
      const lang = codeMatch[1] || "";
      result.push(
        `<pre><code${lang ? ` class="language-${escapeHtmlAttribute(lang)}"` : ""}>${highlightedCodeToHtml(
          codeMatch[2],
          lang,
        )}</code></pre>`,
      );
      continue;
    }

    // Headings
    const headingMatch = trimmed.match(/^(#{1,6})\s+(.+)$/m);
    if (headingMatch && trimmed.split("\n").length === 1) {
      const level = headingMatch[1].length;
      result.push(`<h${level}>${inlineFormat(escapeHtml(headingMatch[2]))}</h${level}>`);
      continue;
    }

    // Blockquote
    if (trimmed.startsWith("> ")) {
      const quoteContent = trimmed
        .split("\n")
        .map((l) => l.replace(/^>\s?/, ""))
        .join("<br>");
      result.push(`<blockquote><p>${inlineFormat(escapeHtml(quoteContent))}</p></blockquote>`);
      continue;
    }

    // Horizontal rule
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
      result.push("<hr>");
      continue;
    }

    // Table
    if (isMarkdownTable(trimmed)) {
      result.push(markdownTableToHtml(trimmed));
      continue;
    }

    // Lists
    if (/^\s*(?:[-*]|\d+\.)\s/.test(trimmed)) {
      result.push(markdownListToHtml(trimmed));
      continue;
    }

    // Paragraph (default)
    const lines = trimmed.split("\n");
    const paraLines = lines.map((l) => inlineFormat(escapeHtml(l))).join("<br>");
    result.push(`<p>${paraLines}</p>`);
  }

  return result.join("") || "<p></p>";
}

function splitMarkdownBlocks(md: string): string[] {
  const blocks: string[] = [];
  const lines = md.split("\n");
  let current: string[] = [];
  let inFence = false;

  const flush = () => {
    if (current.length === 0) return;
    blocks.push(current.join("\n"));
    current = [];
  };

  for (const line of lines) {
    if (/^```/.test(line.trim())) {
      current.push(line);
      inFence = !inFence;
      continue;
    }

    if (!inFence && line.trim() === "") {
      flush();
      continue;
    }

    current.push(line);
  }

  flush();
  return blocks;
}

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function escapeHtmlAttribute(text: string): string {
  return escapeHtml(text).replace(/"/g, "&quot;");
}

function readExcalidrawPreviewElements(scene: string): Array<Record<string, unknown>> {
  const parsedScene = readExcalidrawSceneAttribute(scene);
  return parsedScene.elements.filter(
    (element): element is Record<string, unknown> =>
      Boolean(element) &&
      typeof element === "object" &&
      (element as Record<string, unknown>).isDeleted !== true,
  );
}

function readFiniteNumber(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function sanitizeSvgColor(value: unknown, fallback: string): string {
  if (typeof value !== "string") return fallback;
  const trimmed = value.trim();
  if (/^#[0-9a-f]{3,8}$/i.test(trimmed)) return trimmed;
  if (/^(transparent|currentColor|black|white|red|green|blue|gray|grey)$/i.test(trimmed)) return trimmed;
  return fallback;
}

function getExcalidrawPreviewBounds(elements: Array<Record<string, unknown>>): {
  minX: number;
  minY: number;
  width: number;
  height: number;
} {
  if (elements.length === 0) return { minX: 0, minY: 0, width: 800, height: 320 };

  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;

  for (const element of elements) {
    const x = readFiniteNumber(element.x);
    const y = readFiniteNumber(element.y);
    const width = Math.max(1, readFiniteNumber(element.width, 1));
    const height = Math.max(1, readFiniteNumber(element.height, 1));
    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x + width);
    maxY = Math.max(maxY, y + height);
  }

  if (!Number.isFinite(minX) || !Number.isFinite(minY) || !Number.isFinite(maxX) || !Number.isFinite(maxY)) {
    return { minX: 0, minY: 0, width: 800, height: 320 };
  }

  const padding = 24;
  return {
    minX: minX - padding,
    minY: minY - padding,
    width: Math.max(80, maxX - minX + padding * 2),
    height: Math.max(80, maxY - minY + padding * 2),
  };
}

function renderExcalidrawElementSpec(element: Record<string, unknown>): [string, Record<string, string>, ...unknown[]] {
  const type = String(element.type || "rectangle");
  const x = readFiniteNumber(element.x);
  const y = readFiniteNumber(element.y);
  const width = Math.max(1, readFiniteNumber(element.width, 120));
  const height = Math.max(1, readFiniteNumber(element.height, 80));
  const stroke = sanitizeSvgColor(element.strokeColor, "currentColor");
  const background = sanitizeSvgColor(element.backgroundColor, "transparent");
  const strokeWidth = String(Math.max(1, readFiniteNumber(element.strokeWidth, 2)));
  const opacity = String(Math.max(0.1, Math.min(1, readFiniteNumber(element.opacity, 100) / 100)));

  if (type === "ellipse") {
    return [
      "ellipse",
      {
        cx: String(x + width / 2),
        cy: String(y + height / 2),
        rx: String(width / 2),
        ry: String(height / 2),
        fill: background,
        stroke,
        "stroke-width": strokeWidth,
        opacity,
      },
    ];
  }

  if (type === "diamond") {
    return [
      "polygon",
      {
        points: `${x + width / 2},${y} ${x + width},${y + height / 2} ${x + width / 2},${y + height} ${x},${y + height / 2}`,
        fill: background,
        stroke,
        "stroke-width": strokeWidth,
        opacity,
      },
    ];
  }

  if (type === "line" || type === "arrow") {
    return [
      "line",
      {
        x1: String(x),
        y1: String(y),
        x2: String(x + width),
        y2: String(y + height),
        fill: "none",
        stroke,
        "stroke-width": strokeWidth,
        "stroke-linecap": "round",
        opacity,
      },
    ];
  }

  if (type === "text") {
    const fontSize = Math.max(10, readFiniteNumber(element.fontSize, 24));
    return [
      "text",
      {
        x: String(x),
        y: String(y + fontSize),
        fill: stroke,
        "font-size": String(fontSize),
        "font-family": "Inter, sans-serif",
        opacity,
      },
      String(element.text || ""),
    ];
  }

  return [
    "rect",
    {
      x: String(x),
      y: String(y),
      width: String(width),
      height: String(height),
      rx: "4",
      fill: background,
      stroke,
      "stroke-width": strokeWidth,
      opacity,
    },
  ];
}

function renderExcalidrawPreviewSpec(scene: string): [string, Record<string, string>, ...unknown[]] {
  const elements = readExcalidrawPreviewElements(scene);
  if (elements.length === 0) {
    return [
      "div",
      {
        "data-excalidraw-empty-preview": "true",
        class:
          "flex h-full w-full items-center justify-center bg-surface-raised px-3 text-center text-xs leading-5 text-text-muted",
      },
      "No thumbnail yet. Open drawing to edit.",
    ];
  }

  const bounds = getExcalidrawPreviewBounds(elements);
  return [
    "svg",
    {
      "data-excalidraw-preview": "true",
      class: "h-full w-full text-text",
      viewBox: `${bounds.minX} ${bounds.minY} ${bounds.width} ${bounds.height}`,
      preserveAspectRatio: "xMidYMid meet",
      role: "img",
      "aria-label": "Excalidraw drawing preview",
    },
    ...elements.slice(0, 80).map((element) => renderExcalidrawElementSpec(element)),
  ];
}

function renderExcalidrawElementHtml(element: Record<string, unknown>): string {
  const spec = renderExcalidrawElementSpec(element);
  const [tag, attrs, ...children] = spec;
  const attributes = Object.entries(attrs)
    .map(([key, value]) => ` ${key}="${escapeHtmlAttribute(value)}"`)
    .join("");
  const body = children.map((child) => escapeHtml(String(child))).join("");
  return body ? `<${tag}${attributes}>${body}</${tag}>` : `<${tag}${attributes} />`;
}

function renderExcalidrawPreviewHtml(scene: string): string {
  const elements = readExcalidrawPreviewElements(scene);
  if (elements.length === 0) {
    return '<div data-excalidraw-empty-preview="true" class="flex h-full w-full items-center justify-center bg-surface-raised px-3 text-center text-xs leading-5 text-text-muted">No thumbnail yet. Open drawing to edit.</div>';
  }

  const bounds = getExcalidrawPreviewBounds(elements);
  const children = elements.slice(0, 80).map((element) => renderExcalidrawElementHtml(element)).join("");
  return `<svg data-excalidraw-preview="true" class="h-full w-full text-text" viewBox="${escapeHtmlAttribute(
    `${bounds.minX} ${bounds.minY} ${bounds.width} ${bounds.height}`,
  )}" preserveAspectRatio="xMidYMid meet" role="img" aria-label="Excalidraw drawing preview">${children}</svg>`;
}

function renderExcalidrawCanvasSpec(
  filename: string,
  scene: string,
  htmlAttributes: Record<string, unknown> = {},
): [string, ...unknown[]] {
  return [
    "div",
    mergeAttributes(htmlAttributes, {
      "data-excalidraw-canvas": "true",
      "data-scene": scene || undefined,
      filename,
      role: "button",
      tabindex: "0",
      "aria-label": `Open drawing ${filename}`,
      "data-excalidraw-thumbnail": "true",
      class: "my-3 w-full max-w-4xl cursor-pointer overflow-hidden rounded-card border border-border bg-bg",
    }),
    [
      "div",
      { class: "aspect-video w-full" },
      renderExcalidrawPreviewSpec(scene),
    ],
  ] as [string, ...unknown[]];
}

function renderExcalidrawCanvasHtml(filename: string, scene = ""): string {
  const sceneAttr = scene ? ` data-scene="${escapeHtmlAttribute(scene)}"` : "";
  return [
    `<div data-excalidraw-canvas="true" filename="${escapeHtmlAttribute(
      filename,
    )}"${sceneAttr} data-excalidraw-thumbnail="true" role="button" tabindex="0" aria-label="Open drawing ${escapeHtmlAttribute(
      filename,
    )}" class="my-3 w-full max-w-4xl cursor-pointer overflow-hidden rounded-card border border-border bg-bg">`,
    `<div class="aspect-video w-full">${renderExcalidrawPreviewHtml(
      scene,
    )}</div>`,
    "</div>",
  ].join("");
}

function sanitizeFontFamily(value: string): string {
  return value.replace(/[^\w\s"',.-]/g, "").trim();
}

function highlightedCodeToHtml(code: string, language: string): string {
  if (!language) return escapeHtml(code);

  try {
    const tree = lowlight.highlight(language, code) as {
      children?: Array<{
        type?: string;
        value?: string;
        tagName?: string;
        properties?: { className?: string[] };
        children?: unknown[];
      }>;
    };
    return (tree.children || []).map(lowlightNodeToHtml).join("");
  } catch {
    return escapeHtml(code);
  }
}

function lowlightNodeToHtml(node: unknown): string {
  const typed = node as {
    type?: string;
    value?: string;
    tagName?: string;
    properties?: { className?: string[] };
    children?: unknown[];
  };
  if (typed.type === "text") return escapeHtml(typed.value || "");
  if (typed.type !== "element") return "";

  const tag = typed.tagName || "span";
  const className = typed.properties?.className?.join(" ") || "";
  const attrs = className ? ` class="${escapeHtmlAttribute(className)}"` : "";
  const children = (typed.children || []).map(lowlightNodeToHtml).join("");
  return `<${tag}${attrs}>${children}</${tag}>`;
}

function isMarkdownTable(block: string): boolean {
  const lines = block
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  if (lines.length < 2) return false;
  if (!lines[0].startsWith("|") || !lines[0].endsWith("|")) return false;
  return /^\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?$/.test(lines[1]);
}

type MarkdownListKind = "ul" | "ol" | "task";

type MarkdownListItem = {
  checked?: boolean;
  children: MarkdownListItem[];
  content: string;
  kind: MarkdownListKind;
};

function markdownListToHtml(block: string): string {
  const root: MarkdownListItem = { children: [], content: "", kind: "ul" };
  const stack: Array<{ indent: number; item: MarkdownListItem }> = [{ indent: -1, item: root }];

  for (const line of block.split("\n")) {
    if (!line.trim()) continue;

    const match = line.match(/^(\s*)((?:[-*])|(?:\d+\.))\s+(.+)$/);
    if (!match) continue;

    const indent = match[1].replace(/\t/g, "    ").length;
    const marker = match[2];
    let content = match[3];
    let checked: boolean | undefined;

    if (/^\[[xX]\]\s+/.test(content)) {
      checked = true;
      content = content.replace(/^\[[xX]\]\s+/, "");
    } else if (/^\[ \]\s+/.test(content)) {
      checked = false;
      content = content.replace(/^\[ \]\s+/, "");
    }

    const item: MarkdownListItem = {
      checked,
      children: [],
      content,
      kind: checked === undefined ? (/^\d+\.$/.test(marker) ? "ol" : "ul") : "task",
    };

    while (stack.length > 1 && indent <= stack[stack.length - 1].indent) {
      stack.pop();
    }

    stack[stack.length - 1].item.children.push(item);
    stack.push({ indent, item });
  }

  return renderMarkdownListGroups(root.children);
}

function renderMarkdownListGroups(items: MarkdownListItem[]): string {
  let html = "";
  let index = 0;

  while (index < items.length) {
    const kind = items[index].kind;
    const group: MarkdownListItem[] = [];

    while (index < items.length && items[index].kind === kind) {
      group.push(items[index]);
      index++;
    }

    const body = group.map(renderMarkdownListItem).join("");
    if (kind === "task") {
      html += `<ul data-type="taskList">${body}</ul>`;
    } else {
      html += `<${kind}>${body}</${kind}>`;
    }
  }

  return html;
}

function renderMarkdownListItem(item: MarkdownListItem): string {
  const content = inlineFormat(escapeHtml(item.content));
  const children = item.children.length ? renderMarkdownListGroups(item.children) : "";

  if (item.kind === "task") {
    return `<li data-type="taskItem" data-checked="${item.checked ? "true" : "false"}"><label><input type="checkbox"${item.checked ? " checked" : ""}><span>${content}</span></label>${children}</li>`;
  }

  return `<li><p>${content}</p>${children}</li>`;
}

function markdownTableToHtml(block: string): string {
  const lines = block
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  const rows = lines
    .filter((_, index) => index !== 1)
    .map((line) =>
      line
        .replace(/^\|/, "")
        .replace(/\|$/, "")
        .split("|")
        .map((cell) => inlineFormat(escapeHtml(cell.trim()))),
    );

  const [header = [], ...bodyRows] = rows;
  const head = `<tr>${header.map((cell) => `<th>${cell}</th>`).join("")}</tr>`;
  const body = bodyRows
    .map((row) => `<tr>${row.map((cell) => `<td>${cell}</td>`).join("")}</tr>`)
    .join("");

  return `<table><thead>${head}</thead><tbody>${body}</tbody></table>`;
}

/** Apply inline markdown formatting */
function sanitizeRenderedUrl(value: string, kind: "link" | "image"): string | null {
  const decoded = value.replace(/&amp;/g, "&").trim();
  const allowed = kind === "link"
    ? /^(https?:|mailto:|starlane:)/i
    : /^(https?:|starlane:|data:image\/(?:png|gif|jpe?g|webp);base64,)/i;
  return allowed.test(decoded) ? escapeHtmlAttribute(decoded) : null;
}

function inlineFormat(text: string): string {
  return (
    text
      // Preserved font-family spans. The content is escaped before this point, so only
      // allow a constrained style value back through.
      .replace(
        /&lt;span style="font-family:\s*([^"]+)"&gt;([\s\S]*?)&lt;\/span&gt;/g,
        (_match, fontFamily, inner) =>
          `<span style="font-family: ${escapeHtmlAttribute(sanitizeFontFamily(fontFamily))}">${inner}</span>`,
      )
      // Bold + italic
      .replace(/\*\*\*(.+?)\*\*\*/g, "<strong><em>$1</em></strong>")
      // Bold
      .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
      // Italic
      .replace(/\*(.+?)\*/g, "<em>$1</em>")
      // Strikethrough
      .replace(/~~(.+?)~~/g, "<s>$1</s>")
      // Inline code
      .replace(/`([^`]+)`/g, "<code>$1</code>")
      // Images
      .replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_match, alt, url) => {
        const safeUrl = sanitizeRenderedUrl(url, "image");
        return safeUrl ? `<img src="${safeUrl}" alt="${alt}" />` : alt;
      })
      // Links
      .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_match, label, url) => {
        const safeUrl = sanitizeRenderedUrl(url, "link");
        return safeUrl ? `<a href="${safeUrl}" class="text-primary underline">${label}</a>` : label;
      })
      // Content tags
      .replace(/(^|[\s(])#([A-Za-z][\w-]{0,63})(?=\b)/g, (_match, prefix, tagName) => {
        const safeTag = escapeHtmlAttribute(tagName);
        return `${prefix}<span data-type="content-tag" data-tag="${safeTag}" class="editor-content-tag">#${safeTag}</span>`;
      })
  );
}

/**
 * Convert TipTap HTML back to markdown for storage.
 * Uses DOMParser for safe HTML parsing (no innerHTML on live DOM).
 */
export function htmlToMarkdown(html: string): string {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, "text/html");
  return nodeToMarkdown(doc.body).trim();
}

export function serializeBookmarkCardDirective(
  title: string | null,
  href: string | null,
  description?: string | null,
): string {
  const safeDescription = description?.replace(/"/g, "'").trim();
  return `[bookmark:${title || "Bookmark"}](${href || ""}${safeDescription ? ` "${safeDescription}"` : ""})`;
}

export function serializeExcalidrawDirective(filename: string | null, scene?: string | null): string {
  const directive = `![[excalidraw:${normalizeExcalidrawFilename(filename || "")}]]`;
  if (!scene) return directive;
  const parsedScene = readExcalidrawSceneAttribute(scene);
  if (parsedScene.elements.length === 0 && Object.keys(parsedScene.files ?? {}).length === 0) {
    return directive;
  }
  return `${directive}\n\n\`\`\`excalidraw-json\n${JSON.stringify(parsedScene, null, 2)}\n\`\`\``;
}

function normalizeExcalidrawFilename(value: string): string {
  const cleaned = value
    .trim()
    .replace(/[\\/:*?"<>|]/g, "-")
    .replace(/\s+/g, " ")
    .replace(/^[.\-\s]+/, "")
    .slice(0, 120);
  if (!cleaned) return "untitled.excalidraw";
  return /\.excalidraw$/i.test(cleaned) ? cleaned : `${cleaned}.excalidraw`;
}

function nodeToMarkdown(node: Node): string {
  const parts: string[] = [];

  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      parts.push(child.textContent || "");
      continue;
    }

    if (child.nodeType !== Node.ELEMENT_NODE) continue;
    const el = child as HTMLElement;
    const tag = el.tagName.toLowerCase();

    if (tag === "div" && el.hasAttribute("data-bookmark-card")) {
      parts.push(
        `${serializeBookmarkCardDirective(
          el.getAttribute("title"),
          el.getAttribute("href"),
          el.getAttribute("description"),
        )}\n\n`,
      );
      continue;
    }

    if (tag === "div" && el.hasAttribute("data-excalidraw-canvas")) {
      parts.push(
        `${serializeExcalidrawDirective(
          el.getAttribute("filename"),
          el.getAttribute("data-scene"),
        )}\n\n`,
      );
      continue;
    }

    switch (tag) {
      case "p":
        parts.push(inlineToMd(el) + "\n\n");
        break;
      case "h1":
        parts.push(`# ${inlineToMd(el)}\n\n`);
        break;
      case "h2":
        parts.push(`## ${inlineToMd(el)}\n\n`);
        break;
      case "h3":
        parts.push(`### ${inlineToMd(el)}\n\n`);
        break;
      case "h4":
        parts.push(`#### ${inlineToMd(el)}\n\n`);
        break;
      case "h5":
        parts.push(`##### ${inlineToMd(el)}\n\n`);
        break;
      case "h6":
        parts.push(`###### ${inlineToMd(el)}\n\n`);
        break;
      case "blockquote":
        parts.push(
          nodeToMarkdown(el)
            .trim()
            .split("\n")
            .map((l) => `> ${l}`)
            .join("\n") + "\n\n",
        );
        break;
      case "ul": {
        parts.push(listToMarkdown(el) + "\n");
        break;
      }
      case "ol": {
        parts.push(listToMarkdown(el) + "\n");
        break;
      }
      case "pre": {
        const code = el.querySelector("code");
        const lang =
          Array.from(code?.classList || [])
            .find((className) => className.startsWith("language-"))
            ?.replace("language-", "") || "";
        const text = code?.textContent || el.textContent || "";
        parts.push(`\`\`\`${lang}\n${text}\n\`\`\`\n\n`);
        break;
      }
      case "hr":
        parts.push("---\n\n");
        break;
      case "img": {
        const src = el.getAttribute("src") || "";
        const alt = el.getAttribute("alt") || "";
        parts.push(`![${alt}](${src})\n\n`);
        break;
      }
      case "table":
        parts.push(tableToMd(el) + "\n\n");
        break;
      default:
        parts.push(nodeToMarkdown(el));
    }
  }

  return parts.join("");
}

function listToMarkdown(list: Element, level = 0): string {
  const lines: string[] = [];
  const ordered = list.tagName.toLowerCase() === "ol";
  const taskList = list.getAttribute("data-type") === "taskList";
  let index = 1;

  for (const li of Array.from(list.children)) {
    if (li.tagName.toLowerCase() !== "li") continue;

    const indent = "  ".repeat(level);
    const isTask = taskList || li.getAttribute("data-type") === "taskItem";
    const marker = isTask
      ? `- [${li.getAttribute("data-checked") === "true" ? "x" : " "}]`
      : ordered
        ? `${index}.`
        : "-";
    lines.push(`${indent}${marker} ${listItemTextToMarkdown(li).trim()}\n`);

    for (const childList of Array.from(li.children).filter((child) =>
      ["ul", "ol"].includes(child.tagName.toLowerCase()),
    )) {
      lines.push(listToMarkdown(childList, level + 1));
    }

    index++;
  }

  return lines.join("");
}

function listItemTextToMarkdown(li: Element): string {
  let result = "";

  for (const child of Array.from(li.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      result += child.textContent || "";
      continue;
    }

    if (child.nodeType !== Node.ELEMENT_NODE) continue;

    const el = child as HTMLElement;
    if (["ul", "ol"].includes(el.tagName.toLowerCase())) continue;
    result += inlineToMd(el);
  }

  return result;
}

function inlineToMd(el: Element): string {
  let result = "";
  for (const child of Array.from(el.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      result += child.textContent || "";
      continue;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) continue;

    const c = child as HTMLElement;
    const tag = c.tagName.toLowerCase();
    const inner = inlineToMd(c);

    switch (tag) {
      case "strong":
      case "b":
        result += `**${inner}**`;
        break;
      case "em":
      case "i":
        result += `*${inner}*`;
        break;
      case "s":
      case "del":
        result += `~~${inner}~~`;
        break;
      case "code":
        result += `\`${inner}\``;
        break;
      case "a":
        result += `[${inner}](${c.getAttribute("href") || ""})`;
        break;
      case "u":
        result += inner;
        break;
      case "br":
        result += "\n";
        break;
      case "p":
        result += inner;
        break;
      case "label":
        result += inner;
        break;
      case "span": {
        if (c.getAttribute("data-type") === "content-tag") {
          result += c.textContent || inner;
          break;
        }
        const fontFamily = sanitizeFontFamily(c.style.fontFamily || "");
        if (fontFamily) {
          result += `<span style="font-family: ${fontFamily}">${inner}</span>`;
        } else {
          result += inner;
        }
        break;
      }
      case "input":
        break;
      case "img": {
        const src = c.getAttribute("src") || "";
        const alt = c.getAttribute("alt") || "";
        result += `![${alt}](${src})`;
        break;
      }
      default:
        result += inner;
    }
  }
  return result;
}

function tableToMd(table: HTMLElement): string {
  const rows: string[][] = [];
  for (const tr of Array.from(table.querySelectorAll("tr"))) {
    const cells: string[] = [];
    for (const td of Array.from(tr.querySelectorAll("th, td"))) {
      cells.push(inlineToMd(td).trim());
    }
    rows.push(cells);
  }

  if (rows.length === 0) return "";

  const lines: string[] = [];
  lines.push("| " + rows[0].join(" | ") + " |");
  lines.push("| " + rows[0].map(() => "---").join(" | ") + " |");
  for (let i = 1; i < rows.length; i++) {
    lines.push("| " + rows[i].join(" | ") + " |");
  }
  return lines.join("\n");
}
