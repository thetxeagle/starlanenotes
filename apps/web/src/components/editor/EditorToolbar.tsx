import {
  Bold,
  Bookmark,
  Italic,
  Strikethrough,
  Code,
  IndentDecrease,
  IndentIncrease,
  List,
  ListOrdered,
  CheckSquare,
  Quote,
  Minus,
  Link,
  Image,
  Table,
  FileCode,
  Undo2,
  Redo2,
  Rows3,
  Columns3,
  Trash2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { SaveStatus } from "@/hooks/useAutoSave";
import type { Editor } from "@tiptap/react";
import { Select } from "@/components/ui/select";
import { EditorUrlDialog, type EditorUrlInsertKind } from "./EditorUrlDialog";

export type EditorViewMode = "markdown" | "preview";

interface EditorToolbarProps {
  editor: Editor | null;
  status: SaveStatus;
  locked: boolean;
}

const statusLabels: Record<SaveStatus, string> = {
  idle: "",
  saving: "Saving...",
  saved: "Saved",
  error: "Save failed",
};

const statusColors: Record<SaveStatus, string> = {
  idle: "text-text-muted",
  saving: "text-accent",
  saved: "text-text-muted",
  error: "text-danger",
};

export const EDITOR_TOOLBAR_ERROR_STATUS_CLASS = statusColors.error;

export function EditorToolbar({ editor, status, locked }: EditorToolbarProps) {
  const [tablePickerOpen, setTablePickerOpen] = useState(false);
  const [urlDialogKind, setUrlDialogKind] = useState<EditorUrlInsertKind | null>(null);
  const [urlInput, setUrlInput] = useState("");
  const tablePickerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (event: MouseEvent) => {
      if (tablePickerRef.current && !tablePickerRef.current.contains(event.target as Node)) {
        setTablePickerOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

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
    disabled?: boolean,
  ) => (
    <button
      type="button"
      onClick={action}
      disabled={locked || disabled}
      className={`rounded-button flex h-7 w-7 items-center justify-center transition-colors duration-100 ${
        isActive
          ? "bg-surface-raised text-text"
          : "text-text-muted hover:bg-surface-raised hover:text-text"
      } disabled:opacity-40`}
      aria-label={label}
      title={label}
    >
      {icon}
    </button>
  );

  return (
    <div className="border-border bg-surface h-9 shrink-0 overflow-x-auto border-b">
      {urlDialogKind && (
        <EditorUrlDialog
          kind={urlDialogKind}
          value={urlInput}
          onChange={setUrlInput}
          onCancel={closeUrlDialog}
          onSubmit={insertUrl}
        />
      )}
      <div className="flex min-w-max items-center gap-0.5 px-3">
        {/* Undo / Redo */}
        {btn(<Undo2 size={14} />, "Undo", () => editor.chain().focus().undo().run())}
        {btn(<Redo2 size={14} />, "Redo", () => editor.chain().focus().redo().run())}

        {btn(
          <IndentIncrease size={14} />,
          "Indent",
          () => editor.chain().focus().sinkListItem("listItem").run(),
          false,
          !editor.can().sinkListItem("listItem"),
        )}
        {btn(
          <IndentDecrease size={14} />,
          "Outdent",
          () => editor.chain().focus().liftListItem("listItem").run(),
          false,
          !editor.can().liftListItem("listItem"),
        )}

        <div className="bg-border mx-1 h-4 w-px" />

        {/* Inline formatting */}
        {btn(
          <Bold size={14} />,
          "Bold",
          () => editor.chain().focus().toggleBold().run(),
          editor.isActive("bold"),
        )}
        {btn(
          <Italic size={14} />,
          "Italic",
          () => editor.chain().focus().toggleItalic().run(),
          editor.isActive("italic"),
        )}
        {btn(
          <Strikethrough size={14} />,
          "Strikethrough",
          () => editor.chain().focus().toggleStrike().run(),
          editor.isActive("strike"),
        )}
        {btn(
          <Code size={14} />,
          "Inline code",
          () => editor.chain().focus().toggleCode().run(),
          editor.isActive("code"),
        )}

        <div className="bg-border mx-1 h-4 w-px" />

        {/* Block formatting */}
        {btn(
          <List size={14} />,
          "Bullet list",
          () => editor.chain().focus().toggleBulletList().run(),
          editor.isActive("bulletList"),
        )}
        {btn(
          <ListOrdered size={14} />,
          "Numbered list",
          () => editor.chain().focus().toggleOrderedList().run(),
          editor.isActive("orderedList"),
        )}
        {btn(
          <CheckSquare size={14} />,
          "Task list",
          () => editor.chain().focus().toggleTaskList().run(),
          editor.isActive("taskList"),
        )}

        <div className="bg-border mx-1 h-4 w-px" />

        {btn(
          <Quote size={14} />,
          "Quote",
          () => editor.chain().focus().toggleBlockquote().run(),
          editor.isActive("blockquote"),
        )}
        {btn(<Minus size={14} />, "Divider", () =>
          editor.chain().focus().setHorizontalRule().run(),
        )}
        <div className="relative" ref={tablePickerRef}>
          {btn(<Table size={14} />, "Insert table", () => setTablePickerOpen((prev) => !prev))}
          {tablePickerOpen && (
            <div className="rounded-card border-border bg-surface shadow-card absolute left-0 top-8 z-50 border p-2">
              <div className="text-text-muted mb-2 text-xs">Insert table</div>
              <div className="grid grid-cols-8 gap-1">
                {Array.from({ length: 64 }, (_, index) => {
                  const rows = Math.floor(index / 8) + 1;
                  const cols = (index % 8) + 1;
                  return (
                    <button
                      key={`${rows}x${cols}`}
                      type="button"
                      title={`${cols} columns, ${rows} rows`}
                      aria-label={`Insert ${cols} by ${rows} table`}
                      className="border-border bg-bg hover:border-primary hover:bg-primary/15 h-4 w-4 rounded-[2px] border"
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => {
                        editor
                          .chain()
                          .focus()
                          .insertTable({ rows, cols, withHeaderRow: true })
                          .run();
                        setTablePickerOpen(false);
                      }}
                    />
                  );
                })}
              </div>
            </div>
          )}
        </div>
        {editor.isActive("table") && (
          <>
            {btn(<Rows3 size={14} />, "Add row above", () =>
              editor.chain().focus().addRowBefore().run(),
            )}
            {btn(<Rows3 size={14} />, "Add row below", () =>
              editor.chain().focus().addRowAfter().run(),
            )}
            {btn(<Columns3 size={14} />, "Add column left", () =>
              editor.chain().focus().addColumnBefore().run(),
            )}
            {btn(<Columns3 size={14} />, "Add column right", () =>
              editor.chain().focus().addColumnAfter().run(),
            )}
            {btn(<Rows3 size={14} />, "Delete row", () =>
              editor.chain().focus().deleteRow().run(),
            )}
            {btn(<Columns3 size={14} />, "Delete column", () =>
              editor.chain().focus().deleteColumn().run(),
            )}
            {btn(<Trash2 size={14} />, "Delete table", () =>
              editor.chain().focus().deleteTable().run(),
            )}
          </>
        )}
        {btn(
          <FileCode size={14} />,
          "Code block",
          () => editor.chain().focus().toggleCodeBlock().run(),
          editor.isActive("codeBlock"),
        )}
        {editor.isActive("codeBlock") && (
          <>
            <label className="sr-only" htmlFor="code-block-language">
              Code block language
            </label>
            <Select
              id="code-block-language"
              value={editor.getAttributes("codeBlock").language || ""}
              onChange={(event) => {
                const language = event.target.value || null;
                editor.chain().focus().updateAttributes("codeBlock", { language }).run();
              }}
              disabled={locked}
              title="Code block language"
              className="h-7 w-auto rounded-button bg-surface px-2 text-xs text-text-muted hover:bg-surface-raised hover:text-text disabled:opacity-40"
              aria-label="Code block language"
            >
              <option value="">Text</option>
              <option value="bash">Bash</option>
              <option value="css">CSS</option>
              <option value="html">HTML</option>
              <option value="javascript">JavaScript</option>
              <option value="json">JSON</option>
              <option value="markdown">Markdown</option>
              <option value="powershell">PowerShell</option>
              <option value="python">Python</option>
              <option value="rust">Rust</option>
              <option value="typescript">TypeScript</option>
              <option value="yaml">YAML</option>
            </Select>
          </>
        )}
        {btn(
          <Link size={14} />,
          "Link",
          () => openUrlDialog("link"),
          editor.isActive("link"),
        )}
        {btn(<Bookmark size={14} />, "Bookmark", () => openUrlDialog("bookmark"))}
        {btn(<Image size={14} />, "Image", () => openUrlDialog("image"))}

        {/* Spacer */}
        <div className="flex-1" />

        {/* Save status */}
        {status !== "idle" && (
          <span
            className={`mr-2 text-xs ${statusColors[status]}`}
            aria-live="polite"
            aria-atomic="true"
          >
            {statusLabels[status]}
          </span>
        )}
      </div>
    </div>
  );
}
