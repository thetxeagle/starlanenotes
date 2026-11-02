import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  Bold,
  CheckSquare,
  Code,
  Italic,
  Link as LinkIcon,
  List,
  ListOrdered,
  MessageSquare,
  Plus,
  Quote,
  Redo2,
  Smile,
  Strikethrough,
  Underline,
  Undo2,
} from "lucide-react";
import type { Editor as TipTapEditor } from "@tiptap/react";
import { TextSelection } from "@tiptap/pm/state";
import { Select } from "@/components/ui/select";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { EmojiPickerContent } from "@/components/editor/EmojiPickerContent";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_ACTIVE_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
} from "@/components/ui/control-chrome";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import type { MobileToolbarActionId } from "./Editor";

export interface PersistentDocumentToolbarProps {
  editor: TipTapEditor;
  host: HTMLElement;
  commentsAvailable: boolean;
  insertOpen: boolean;
  onAction: (action: MobileToolbarActionId, trigger?: HTMLElement) => void;
  onInsert: (trigger: HTMLButtonElement) => void;
}

export type PersistentToolbarHeadingValue = "paragraph" | "1" | "2" | "3" | "4" | "5" | "6";
export type PersistentToolbarSelection = { from: number; to: number };

function getHeadingValue(editor: TipTapEditor): PersistentToolbarHeadingValue {
  if (editor.isActive("heading", { level: 1 })) return "1";
  if (editor.isActive("heading", { level: 2 })) return "2";
  if (editor.isActive("heading", { level: 3 })) return "3";
  if (editor.isActive("heading", { level: 4 })) return "4";
  if (editor.isActive("heading", { level: 5 })) return "5";
  if (editor.isActive("heading", { level: 6 })) return "6";
  return "paragraph";
}

export function applyPersistentToolbarHeading(
  editor: TipTapEditor,
  value: PersistentToolbarHeadingValue,
  selection: PersistentToolbarSelection | null,
): boolean {
  const chain = editor.chain().focus();
  const selectedChain = selection ? chain.setTextSelection(selection) : chain;
  return value === "paragraph"
    ? selectedChain.setParagraph().run()
    : selectedChain.setHeading({ level: Number(value) as 1 | 2 | 3 | 4 | 5 | 6 }).run();
}

export function PersistentDocumentToolbar({
  editor,
  host,
  commentsAvailable,
  insertOpen,
  onAction,
  onInsert,
}: PersistentDocumentToolbarProps) {
  const [heading, setHeading] = useState<PersistentToolbarHeadingValue>(() =>
    getHeadingValue(editor),
  );
  const [emojiOpen, setEmojiOpen] = useState(false);
  const [, setEditorRevision] = useState(0);
  const capturedSelection = useRef<PersistentToolbarSelection | null>(null);

  const captureSelection = useCallback(() => {
    if (editor.isDestroyed) return;
    const { selection } = editor.state;
    capturedSelection.current =
      selection instanceof TextSelection ? { from: selection.from, to: selection.to } : null;
  }, [editor]);

  const restoreCapturedSelection = useCallback(() => {
    if (editor.isDestroyed) return;
    const range = capturedSelection.current;
    const chain = editor.chain().focus();
    if (range) chain.setTextSelection(range).run();
    else editor.commands.focus();
  }, [editor]);

  useEffect(() => {
    const refresh = () => {
      if (editor.isFocused) captureSelection();
      setHeading(getHeadingValue(editor));
      setEditorRevision((revision) => revision + 1);
    };
    editor.on("selectionUpdate", refresh);
    editor.on("transaction", refresh);
    refresh();
    return () => {
      editor.off("selectionUpdate", refresh);
      editor.off("transaction", refresh);
    };
  }, [captureSelection, editor]);

  const changeHeading = (value: string) => {
    const nextHeading = value as PersistentToolbarHeadingValue;
    applyPersistentToolbarHeading(editor, nextHeading, capturedSelection.current);
    setHeading(nextHeading);
    window.requestAnimationFrame(restoreCapturedSelection);
  };

  const insertEmoji = (emoji: string) => {
    if (editor.isDestroyed) return;
    const chain = editor.chain().focus();
    const selectedChain = capturedSelection.current
      ? chain.setTextSelection(capturedSelection.current)
      : chain;
    selectedChain.insertContent(emoji).run();
    setEmojiOpen(false);
    window.requestAnimationFrame(restoreCapturedSelection);
  };

  const actionButton = (
    label: string,
    icon: ReactNode,
    action: MobileToolbarActionId,
    active?: boolean,
    disabled = false,
  ) => (
    <Tooltip key={label}>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          aria-pressed={active}
          disabled={disabled}
          title={label}
          onPointerDown={(event) => {
            captureSelection();
            event.preventDefault();
            if (!editor.isDestroyed) editor.view.focus();
          }}
          onClick={(event) => onAction(action, event.currentTarget)}
          className={`${active ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : CONTROL_CHROME_ACTION_CLASS} hover:bg-surface-raised hover:text-text focus-ring flex h-8 w-8 shrink-0 items-center justify-center disabled:cursor-not-allowed disabled:opacity-40`}
        >
          {icon}
        </button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );

  return createPortal(
    <TooltipProvider delayDuration={350}>
      <div
        className={`${CONTROL_CHROME_RAIL_CLASS} w-full min-w-0 overflow-x-auto rounded-none border-0 p-0 shadow-none`}
        role="toolbar"
        aria-label="Document formatting and insertion"
        data-starlane-persistent-editor-toolbar="true"
      >
        {actionButton(
          "Undo",
          <Undo2 size={15} aria-hidden="true" />,
          "undo",
          undefined,
          !editor.can().undo(),
        )}
        {actionButton(
          "Redo",
          <Redo2 size={15} aria-hidden="true" />,
          "redo",
          undefined,
          !editor.can().redo(),
        )}
        <span aria-hidden="true" className="bg-border mx-0.5 h-5 w-px shrink-0" />
        <div className="w-40 shrink-0" onPointerDownCapture={captureSelection}>
          <Select
            aria-label="Paragraph style"
            value={heading}
            onChange={(event) => changeHeading(event.target.value)}
            className="rounded-button h-8 border-0 bg-transparent pl-2 !pr-2 text-xs shadow-none"
          >
            <option value="paragraph">Paragraph</option>
            <option value="1">Heading 1</option>
            <option value="2">Heading 2</option>
            <option value="3">Heading 3</option>
            <option value="4">Heading 4</option>
            <option value="5">Heading 5</option>
            <option value="6">Heading 6</option>
          </Select>
        </div>
        <span aria-hidden="true" className="bg-border mx-0.5 h-5 w-px shrink-0" />
        {actionButton(
          "Bold",
          <Bold size={15} aria-hidden="true" />,
          "bold",
          editor.isActive("bold"),
        )}
        {actionButton(
          "Italic",
          <Italic size={15} aria-hidden="true" />,
          "italic",
          editor.isActive("italic"),
        )}
        {actionButton(
          "Underline",
          <Underline size={15} aria-hidden="true" />,
          "underline",
          editor.isActive("underline"),
        )}
        {actionButton(
          "Strikethrough",
          <Strikethrough size={15} aria-hidden="true" />,
          "strike",
          editor.isActive("strike"),
        )}
        {actionButton(
          "Inline code",
          <Code size={15} aria-hidden="true" />,
          "code",
          editor.isActive("code"),
        )}
        <span aria-hidden="true" className="bg-border mx-0.5 h-5 w-px shrink-0" />
        {actionButton(
          "Bullet list",
          <List size={15} aria-hidden="true" />,
          "bullet",
          editor.isActive("bulletList"),
        )}
        {actionButton(
          "Numbered list",
          <ListOrdered size={15} aria-hidden="true" />,
          "numbered",
          editor.isActive("orderedList"),
        )}
        {actionButton(
          "Task list",
          <CheckSquare size={15} aria-hidden="true" />,
          "task",
          editor.isActive("taskList"),
        )}
        {actionButton(
          "Quote",
          <Quote size={15} aria-hidden="true" />,
          "quote",
          editor.isActive("blockquote"),
        )}
        <span aria-hidden="true" className="bg-border mx-0.5 h-5 w-px shrink-0" />
        {actionButton(
          "Link",
          <LinkIcon size={15} aria-hidden="true" />,
          "link",
          editor.isActive("link"),
        )}
        {commentsAvailable
          ? actionButton(
              "Comment on selection",
              <MessageSquare size={15} aria-hidden="true" />,
              "comment",
            )
          : null}
        <Popover open={emojiOpen} onOpenChange={setEmojiOpen}>
          <Tooltip>
            <TooltipTrigger asChild>
              <PopoverTrigger asChild>
                <button
                  type="button"
                  aria-label="Insert emoji"
                  aria-haspopup="dialog"
                  aria-expanded={emojiOpen}
                  title="Insert emoji"
                  onPointerDown={(event) => {
                    captureSelection();
                    event.preventDefault();
                    if (!editor.isDestroyed) editor.view.focus();
                  }}
                  className={`${CONTROL_CHROME_ACTION_CLASS} hover:bg-surface-raised hover:text-text focus-ring flex h-8 w-8 shrink-0 items-center justify-center`}
                >
                  <Smile size={15} aria-hidden="true" />
                </button>
              </PopoverTrigger>
            </TooltipTrigger>
            <TooltipContent>Insert emoji</TooltipContent>
          </Tooltip>
          <PopoverContent
            align="start"
            side="bottom"
            sideOffset={6}
            className="starlane-emoji-picker w-[22.5rem] max-w-[calc(100vw-1rem)] p-2"
            aria-label="Emoji picker"
          >
            <EmojiPickerContent onSelect={insertEmoji} />
          </PopoverContent>
        </Popover>
        <span aria-hidden="true" className="bg-border mx-0.5 h-5 w-px shrink-0" />
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              aria-label="Insert content"
              aria-haspopup="dialog"
              aria-expanded={insertOpen}
              title="Insert content"
              onPointerDown={(event) => {
                event.preventDefault();
                if (!editor.isDestroyed) editor.view.focus();
              }}
              onClick={(event) => onInsert(event.currentTarget)}
              className={`${CONTROL_CHROME_ACTION_CLASS} hover:bg-surface-raised hover:text-text focus-ring flex h-8 shrink-0 items-center gap-1.5 px-2 text-xs`}
            >
              <Plus size={14} aria-hidden="true" /> Insert
            </button>
          </TooltipTrigger>
          <TooltipContent>Insert content</TooltipContent>
        </Tooltip>
      </div>
    </TooltipProvider>,
    host,
  );
}
