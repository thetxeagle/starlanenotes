import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  Bold,
  CheckSquare,
  Code,
  Italic,
  Link as LinkIcon,
  List,
  ListOrdered,
  Quote,
  Redo2,
  Strikethrough,
  Table2,
  Undo2,
  Plus,
} from "lucide-react";
import { Editor } from "@/components/editor/Editor";
import { EditorUrlDialog, type EditorUrlInsertKind } from "@/components/editor/EditorUrlDialog";
import { Select } from "@/components/ui/select";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_ACTIVE_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
} from "@/components/ui/control-chrome";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { TipTapEditor } from "@/components/editor/Editor";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";

export interface PrototypeEditorProps {
  content: string;
  editable: boolean;
  onChange: (content: string) => void;
  onEditorReady?: (editor: TipTapEditor) => void;
  onDocumentNavigate?: (documentId: string) => void;
  toolbarTarget?: HTMLElement | null;
}

type TextStyle = "paragraph" | "1" | "2" | "3";
type SelectionRange = { from: number; to: number };

function getTextStyle(editor: TipTapEditor): TextStyle {
  if (editor.isActive("heading", { level: 1 })) return "1";
  if (editor.isActive("heading", { level: 2 })) return "2";
  if (editor.isActive("heading", { level: 3 })) return "3";
  return "paragraph";
}

/** Isolated workspace prototype adapter around the production TipTap editor. */
export function PrototypeEditor({
  content,
  editable,
  onChange,
  onEditorReady,
  onDocumentNavigate,
  toolbarTarget,
}: PrototypeEditorProps) {
  const [editor, setEditor] = useState<TipTapEditor | null>(null);
  const [textStyle, setTextStyle] = useState<TextStyle>("paragraph");
  const [insertOpen, setInsertOpen] = useState(false);
  const [urlDialogKind, setUrlDialogKind] = useState<EditorUrlInsertKind | null>(null);
  const [urlInput, setUrlInput] = useState("");
  const [, setEditorRevision] = useState(0);
  const savedSelection = useRef<SelectionRange | null>(null);
  const insertMenuRef = useRef<HTMLDivElement | null>(null);
  const handleInsertMenuKeyDown = useMenuKeyboardNavigation(insertMenuRef);

  useEffect(() => {
    if (!editor) return;
    const refreshState = () => {
      const { from, to } = editor.state.selection;
      if (editor.isFocused) savedSelection.current = { from, to };
      setTextStyle(getTextStyle(editor));
      setEditorRevision((revision) => revision + 1);
    };
    editor.on("selectionUpdate", refreshState);
    editor.on("transaction", refreshState);
    refreshState();
    return () => {
      editor.off("selectionUpdate", refreshState);
      editor.off("transaction", refreshState);
    };
  }, [editor]);

  const rememberSelection = () => {
    if (!editor) return;
    const { from, to } = editor.state.selection;
    savedSelection.current = { from, to };
  };

  const selectedChain = () => {
    if (!editor) return null;
    const range = savedSelection.current;
    const chain = editor.chain().focus();
    return range ? chain.setTextSelection(range) : chain;
  };

  const changeTextStyle = (next: string) => {
    if (!editable) return;
    const chain = selectedChain();
    if (!chain) return;
    if (next === "paragraph") chain.setParagraph().run();
    else chain.setHeading({ level: Number(next) as 1 | 2 | 3 }).run();
    setTextStyle(next as TextStyle);
  };

  const runSelected = (command: (chain: ReturnType<TipTapEditor["chain"]>) => boolean) => {
    if (!editable) return;
    const chain = selectedChain();
    if (chain) command(chain);
  };

  const restoreSelection = () => {
    const range = savedSelection.current;
    if (!editor || !range) return editor?.chain().focus() ?? null;
    return editor.chain().focus().setTextSelection(range);
  };

  const insertBlock = (kind: "table" | "code" | "columns") => {
    const chain = restoreSelection();
    if (!chain) return;
    if (kind === "table") {
      chain.deleteSelection().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run();
    } else if (kind === "code") {
      chain.deleteSelection().toggleCodeBlock().run();
    } else {
      chain
        .deleteSelection()
        .insertContent({
          type: "columns",
          attrs: { layout: "two-equal" },
          content: [
            { type: "sectionColumn", content: [{ type: "paragraph" }] },
            { type: "sectionColumn", content: [{ type: "paragraph" }] },
          ],
        })
        .run();
    }
    setInsertOpen(false);
  };

  const toolbarButton = (
    label: string,
    icon: React.ReactNode,
    action: () => void,
    active = false,
    disabled = false,
  ) => (
    <button
      key={label}
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled || !editable}
      onPointerDown={(event) => {
        rememberSelection();
        event.preventDefault();
      }}
      onClick={action}
      className={`${CONTROL_CHROME_ACTION_CLASS} hover:bg-surface-raised hover:text-text focus-visible:outline-primary flex h-8 w-8 shrink-0 items-center justify-center focus-visible:outline focus-visible:outline-2 ${active ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : ""} disabled:cursor-not-allowed disabled:opacity-40`}
    >
      {icon}
    </button>
  );

  const toolbar =
    editable && editor ? (
      <>
        {urlDialogKind && (
          <EditorUrlDialog
            kind={urlDialogKind}
            value={urlInput}
            onChange={setUrlInput}
            onCancel={() => {
              setUrlDialogKind(null);
              setUrlInput("");
              window.requestAnimationFrame(() => restoreSelection()?.run());
            }}
            onSubmit={() => {
              const url = urlInput.trim();
              if (!url) return;
              if (urlDialogKind === "link") {
                restoreSelection()?.setLink({ href: url }).run();
              }
              setUrlDialogKind(null);
              setUrlInput("");
            }}
          />
        )}
        <div
          className={`${CONTROL_CHROME_RAIL_CLASS} prototype-editor-toolbar w-full min-w-0 overflow-x-auto border-b-0 shadow-none`}
          role="toolbar"
          aria-label="Document formatting and insertion"
          onPointerDownCapture={rememberSelection}
          onKeyDownCapture={rememberSelection}
        >
          {toolbarButton(
            "Undo",
            <Undo2 size={15} />,
            () => runSelected((chain) => chain.undo().run()),
            false,
            !editor.can().undo(),
          )}
          {toolbarButton(
            "Redo",
            <Redo2 size={15} />,
            () => runSelected((chain) => chain.redo().run()),
            false,
            !editor.can().redo(),
          )}
          <span aria-hidden="true" className="bg-border mx-0.5 h-5 w-px shrink-0" />
          <div className="w-32 shrink-0">
            <Select
              aria-label="Paragraph style"
              value={textStyle}
              onChange={(event) => changeTextStyle(event.target.value)}
              className="rounded-button h-8 border-0 bg-transparent pl-2 text-xs shadow-none"
            >
              <option value="paragraph">Paragraph</option>
              <option value="1">Heading 1</option>
              <option value="2">Heading 2</option>
              <option value="3">Heading 3</option>
            </Select>
          </div>
          <span aria-hidden="true" className="bg-border mx-0.5 h-5 w-px shrink-0" />
          {toolbarButton(
            "Bold",
            <Bold size={15} />,
            () => runSelected((chain) => chain.toggleBold().run()),
            editor.isActive("bold"),
          )}
          {toolbarButton(
            "Italic",
            <Italic size={15} />,
            () => runSelected((chain) => chain.toggleItalic().run()),
            editor.isActive("italic"),
          )}
          {toolbarButton(
            "Strikethrough",
            <Strikethrough size={15} />,
            () => runSelected((chain) => chain.toggleStrike().run()),
            editor.isActive("strike"),
          )}
          {toolbarButton(
            "Inline code",
            <Code size={15} />,
            () => runSelected((chain) => chain.toggleCode().run()),
            editor.isActive("code"),
          )}
          <span aria-hidden="true" className="bg-border mx-0.5 h-5 w-px shrink-0" />
          {toolbarButton(
            "Bullet list",
            <List size={15} />,
            () => runSelected((chain) => chain.toggleBulletList().run()),
            editor.isActive("bulletList"),
          )}
          {toolbarButton(
            "Numbered list",
            <ListOrdered size={15} />,
            () => runSelected((chain) => chain.toggleOrderedList().run()),
            editor.isActive("orderedList"),
          )}
          {toolbarButton(
            "Task list",
            <CheckSquare size={15} />,
            () => runSelected((chain) => chain.toggleTaskList().run()),
            editor.isActive("taskList"),
          )}
          {toolbarButton(
            "Quote",
            <Quote size={15} />,
            () => runSelected((chain) => chain.toggleBlockquote().run()),
            editor.isActive("blockquote"),
          )}
          {toolbarButton(
            "Link",
            <LinkIcon size={15} />,
            () => {
              setUrlInput("");
              setUrlDialogKind("link");
            },
            editor.isActive("link"),
          )}
          <span aria-hidden="true" className="bg-border mx-0.5 h-5 w-px shrink-0" />
          <Popover open={insertOpen} onOpenChange={setInsertOpen}>
            <PopoverTrigger asChild>
              <button
                type="button"
                aria-label="Insert content"
                aria-haspopup="menu"
                className={`${CONTROL_CHROME_ACTION_CLASS} hover:bg-surface-raised hover:text-text flex h-8 shrink-0 items-center gap-1.5 px-2 text-xs`}
                onPointerDown={(event) => {
                  rememberSelection();
                  event.preventDefault();
                }}
              >
                <Plus size={14} /> Insert
              </button>
            </PopoverTrigger>
            <PopoverContent
              ref={insertMenuRef}
              className="w-48"
              role="menu"
              aria-label="Insert content"
              onKeyDown={handleInsertMenuKeyDown}
              onCloseAutoFocus={(event) => {
                event.preventDefault();
                window.requestAnimationFrame(() => editor.commands.focus());
              }}
              onOpenAutoFocus={(event) => {
                event.preventDefault();
                window.requestAnimationFrame(() => {
                  insertMenuRef.current
                    ?.querySelector<HTMLButtonElement>("[role='menuitem']")
                    ?.focus();
                });
              }}
            >
              {(
                [
                  ["table", <Table2 size={14} />, "Table"],
                  ["code", <Code size={14} />, "Code block"],
                  ["columns", <Plus size={14} />, "Two columns"],
                ] as const
              ).map(([kind, icon, label]) => (
                <button
                  key={kind}
                  type="button"
                  role="menuitem"
                  className="focus-ring rounded-button text-text-muted hover:bg-surface hover:text-text flex h-9 w-full items-center gap-2 px-2 text-left text-sm"
                  onPointerDown={(event) => event.preventDefault()}
                  onClick={() => insertBlock(kind)}
                >
                  {icon}
                  {label}
                </button>
              ))}
            </PopoverContent>
          </Popover>
        </div>
      </>
    ) : null;

  return (
    <section
      className="prototype-editor min-w-0"
      aria-label="Document editor"
      data-editable={editable ? "true" : "false"}
    >
      {toolbar && toolbarTarget ? createPortal(toolbar, toolbarTarget) : null}
      <Editor
        key={editable ? "edit" : "read"}
        content={content}
        onChange={onChange}
        readOnly={!editable}
        autoFocus={false}
        showBlockChrome={editable}
        onDocumentNavigate={onDocumentNavigate}
        onEditorReady={(readyEditor) => {
          setEditor(readyEditor);
          const { from, to } = readyEditor.state.selection;
          savedSelection.current = { from, to };
          setTextStyle(getTextStyle(readyEditor));
          onEditorReady?.(readyEditor);
        }}
      />
    </section>
  );
}
