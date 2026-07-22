import { useEffect, useState } from "react";
import {
  Bold,
  Bookmark,
  Code,
  Heading1,
  Heading2,
  Heading3,
  Image,
  Keyboard,
  Link,
  List,
  ListOrdered,
  Minus,
  Quote,
  Redo2,
  Strikethrough,
  Table,
  Undo2,
  Italic,
  Rows3,
  CheckSquare,
} from "lucide-react";
import type { Editor } from "@tiptap/react";
import { EditorUrlDialog, type EditorUrlInsertKind } from "./EditorUrlDialog";

interface MobileEditorToolbarProps {
  editor: Editor | null;
  locked: boolean;
}

const SESSION_KEY = "mobile_editor_toolbar_visible";

function readInitialVisibility() {
  if (typeof sessionStorage === "undefined") return true;
  const saved = sessionStorage.getItem(SESSION_KEY);
  return saved !== "false";
}

export function MobileEditorToolbar({ editor, locked }: MobileEditorToolbarProps) {
  const [visible, setVisible] = useState(readInitialVisibility);
  const [urlDialogKind, setUrlDialogKind] = useState<EditorUrlInsertKind | null>(null);
  const [urlInput, setUrlInput] = useState("");

  useEffect(() => {
    if (typeof sessionStorage === "undefined") return;
    sessionStorage.setItem(SESSION_KEY, String(visible));
  }, [visible]);

  if (!editor) return null;

  const openUrlDialog = (kind: EditorUrlInsertKind) => {
    setUrlInput("");
    setUrlDialogKind(kind);
  };

  const closeUrlDialog = () => {
    setUrlInput("");
    setUrlDialogKind(null);
  };

  const insertUrl = () => {
    const url = urlInput.trim();
    if (!url || !urlDialogKind) return;
    if (urlDialogKind === "link") {
      editor.chain().focus().setLink({ href: url }).run();
    } else if (urlDialogKind === "image") {
      editor.chain().focus().setImage({ src: url }).run();
    } else {
      const title = (() => {
        try {
          return new URL(url).hostname.replace(/^www\./i, "") || url;
        } catch {
          return url;
        }
      })();
      editor.chain().focus().insertContent({ type: "bookmarkCard", attrs: { href: url, title } }).run();
    }
    closeUrlDialog();
  };

  const btn = (
    icon: React.ReactNode,
    label: string,
    action: () => void,
    isActive?: boolean,
  ) => (
    <button
      type="button"
      onClick={action}
      disabled={locked}
      className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-button transition-colors duration-100 ${
        isActive
          ? "bg-surface-raised text-text"
          : "text-text-muted hover:bg-surface-raised hover:text-text"
      } disabled:opacity-40`}
      aria-label={label}
    >
      {icon}
    </button>
  );

  const dismissKeyboard = () => {
    const active = document.activeElement;
    if (active instanceof HTMLElement) {
      active.blur();
    }
  };

  if (!visible) {
    return (
      <div className="fixed bottom-[calc(env(safe-area-inset-bottom)+3.8rem)] right-3 z-40">
        <button
          type="button"
          onClick={() => setVisible(true)}
          className="flex items-center gap-1 rounded-full border border-border bg-surface px-3 py-1.5 text-xs text-text shadow-card"
          aria-label="Show mobile editor toolbar"
        >
          <Rows3 size={12} />
          Toolbar
        </button>
      </div>
    );
  }

  return (
    <>
      {urlDialogKind && (
        <EditorUrlDialog
          kind={urlDialogKind}
          value={urlInput}
          onChange={setUrlInput}
          onCancel={closeUrlDialog}
          onSubmit={insertUrl}
        />
      )}
      <div className="fixed inset-x-0 bottom-[calc(env(safe-area-inset-bottom)+3.6rem)] z-40 border-t border-border bg-surface">
        <div className="flex gap-1 overflow-x-auto px-2 py-1.5">
        {btn(<Undo2 size={14} />, "Undo", () => editor.chain().focus().undo().run())}
        {btn(<Redo2 size={14} />, "Redo", () => editor.chain().focus().redo().run())}
        <div className="mx-0.5 h-8 w-px shrink-0 bg-border" />
        {btn(<Bold size={14} />, "Bold", () => editor.chain().focus().toggleBold().run(), editor.isActive("bold"))}
        {btn(<Italic size={14} />, "Italic", () => editor.chain().focus().toggleItalic().run(), editor.isActive("italic"))}
        {btn(<Strikethrough size={14} />, "Strikethrough", () => editor.chain().focus().toggleStrike().run(), editor.isActive("strike"))}
        {btn(<Code size={14} />, "Inline code", () => editor.chain().focus().toggleCode().run(), editor.isActive("code"))}
        <div className="mx-0.5 h-8 w-px shrink-0 bg-border" />
        {btn(<Heading1 size={14} />, "Heading 1", () => editor.chain().focus().toggleHeading({ level: 1 }).run(), editor.isActive("heading", { level: 1 }))}
        {btn(<Heading2 size={14} />, "Heading 2", () => editor.chain().focus().toggleHeading({ level: 2 }).run(), editor.isActive("heading", { level: 2 }))}
        {btn(<Heading3 size={14} />, "Heading 3", () => editor.chain().focus().toggleHeading({ level: 3 }).run(), editor.isActive("heading", { level: 3 }))}
        <div className="mx-0.5 h-8 w-px shrink-0 bg-border" />
        {btn(<List size={14} />, "Bullet list", () => editor.chain().focus().toggleBulletList().run(), editor.isActive("bulletList"))}
        {btn(<ListOrdered size={14} />, "Numbered list", () => editor.chain().focus().toggleOrderedList().run(), editor.isActive("orderedList"))}
        {btn(<CheckSquare size={14} />, "Task list", () => editor.chain().focus().toggleTaskList().run(), editor.isActive("taskList"))}
        <div className="mx-0.5 h-8 w-px shrink-0 bg-border" />
        {btn(<Quote size={14} />, "Quote", () => editor.chain().focus().toggleBlockquote().run(), editor.isActive("blockquote"))}
        {btn(<Minus size={14} />, "Divider", () => editor.chain().focus().setHorizontalRule().run())}
        {btn(<Table size={14} />, "Table", () => editor.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run())}
        {btn(<Link size={14} />, "Link", () => openUrlDialog("link"), editor.isActive("link"))}
        {btn(<Bookmark size={14} />, "Bookmark", () => openUrlDialog("bookmark"))}
        {btn(<Image size={14} />, "Image", () => openUrlDialog("image"))}
        <div className="mx-0.5 h-8 w-px shrink-0 bg-border" />
        <button
          type="button"
          onClick={dismissKeyboard}
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-button text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
          aria-label="Dismiss keyboard"
        >
          <Keyboard size={14} />
        </button>
        <button
          type="button"
          onClick={() => setVisible(false)}
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-button text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
          aria-label="Hide mobile editor toolbar"
        >
          <Rows3 size={14} />
        </button>
        </div>
      </div>
    </>
  );
}
