import { useEffect, useId, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import type { ReactNodeViewProps } from "@tiptap/react";
import { NodeViewWrapper } from "@tiptap/react";
import { NodeSelection } from "prosemirror-state";
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  Download,
  ExternalLink,
  FileText,
  GalleryHorizontalEnd,
  ImageUp,
  Images,
  Trash2,
} from "lucide-react";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { Input } from "@/components/ui/input";
import {
  EditorMediaActionMenu,
  triggerMediaDownload,
  type EditorMediaAction,
} from "./EditorMediaActionMenu";

const IMAGE_CAPTION_MAX_LENGTH = 280;
export const OPEN_IMAGE_ATTACHMENT_HISTORY_EVENT = "starlane:open-image-attachment-history";
export const OPEN_IMAGE_REPLACE_EVENT = "starlane:open-image-replace";
export const OPEN_IMAGE_CONVERT_EVENT = "starlane:convert-image";
export const IMAGE_ALIGNMENT_VALUES = ["left", "center", "right"] as const;
export type ImageAlignment = (typeof IMAGE_ALIGNMENT_VALUES)[number];
/**
 * Captions deliberately use the portable Markdown image alt field. That keeps
 * editing, read-only rendering, export, imports, and local-vault mirrors on one
 * representation instead of adding browser-only image metadata.
 */
export function normalizeImageCaption(value: string): string {
  return value.replace(/\s+/g, " ").trim().slice(0, IMAGE_CAPTION_MAX_LENGTH);
}

/**
 * Alignment lives in a Starlane-prefixed Markdown image title, not local UI
 * state. Readers that do not know the marker still render the image normally.
 */
export function normalizeImageAlignment(value: unknown): ImageAlignment {
  return IMAGE_ALIGNMENT_VALUES.includes(value as ImageAlignment)
    ? (value as ImageAlignment)
    : "left";
}

export function getImageAlignmentClass(alignment: ImageAlignment): string {
  if (alignment === "center") return "mx-auto";
  if (alignment === "right") return "ml-auto";
  return "mr-auto";
}

export function getImageAlignmentMarkdownTitle(alignment: ImageAlignment): string {
  return alignment === "left" ? "" : ` \"starlane:align=${alignment}\"`;
}

export function getImageAlignmentActionLabels(): string[] {
  return ["Align image left", "Align image center", "Align image right"];
}

export function getImageBlockActionLabels(editable: boolean): string[] {
  return editable
    ? [
        "Open image",
        "Download image",
        "Align image left",
        "Align image center",
        "Align image right",
        "Add to gallery",
        "Add to carousel",
        "Replace image",
        "Delete image",
      ]
    : ["Open image", "Download image"];
}

/** Keeps a delayed acknowledgement bound to the original actual image node. */
export function captureImageAttachmentHistoryTarget(
  editor: ReactNodeViewProps["editor"],
  getPos: ReactNodeViewProps["getPos"],
  readVersion: () => string,
  updateAttributes: ReactNodeViewProps["updateAttributes"],
) {
  const position = typeof getPos === "function" ? getPos() : undefined,
    original = readVersion();
  const verify = () => {
    const current = typeof position === "number" ? editor.state.doc.nodeAt(position) : undefined;
    if (
      editor.isDestroyed ||
      typeof position !== "number" ||
      typeof getPos !== "function" ||
      getPos() !== position ||
      readVersion() !== original ||
      current?.type.name !== "image" ||
      current.attrs.attachmentId !== original
    )
      throw new Error(
        "Original image block changed; inspect its acknowledged replacement before use.",
      );
  };
  verify();
  return {
    editor,
    versionId: original,
    verify,
    replace: (nextSrc: string, versionId: string) => {
      verify();
      if (!editor.isEditable) throw new Error("Original image is no longer editable.");
      updateAttributes({ src: nextSrc, attachmentId: versionId });
    },
  };
}

export function ImageBlockNodeView({
  node,
  updateAttributes,
  deleteNode,
  editor,
  getPos,
  selected,
}: ReactNodeViewProps) {
  const captionId = useId();
  const src = String(node.attrs.src || "");
  const caption = String(node.attrs.alt || "");
  const alignment = normalizeImageAlignment(node.attrs.alignment);
  const attachmentId = String(node.attrs.attachmentId || "");
  const currentAttachment = useRef(attachmentId);
  currentAttachment.current = attachmentId;
  const options = editor.extensionManager.extensions.find(
    (extension) => extension.name === "fileAttachment",
  )?.options as { spaceId?: string; docId?: string } | undefined;
  const serverAttachment = Boolean(
    attachmentId && options?.spaceId && options.docId && getCurrentLaunchMode() !== "local",
  );
  const [historyError,setHistoryError]=useState("");
  const openAttachmentHistory = () => {
    try {
    const detail = captureImageAttachmentHistoryTarget(
      editor,
      getPos,
      () => currentAttachment.current,
      updateAttributes,
    );
    setHistoryError("");
    document.dispatchEvent(new CustomEvent(OPEN_IMAGE_ATTACHMENT_HISTORY_EVENT, { detail }));
    } catch(error){setHistoryError(error instanceof Error?error.message:"Original image history unavailable.");}
  };
  const [draftCaption, setDraftCaption] = useState(caption);

  useEffect(() => {
    setDraftCaption(caption);
  }, [caption]);

  const commitCaption = () => {
    const nextCaption = normalizeImageCaption(draftCaption);
    if (nextCaption !== caption) updateAttributes({ alt: nextCaption });
    if (nextCaption !== draftCaption) setDraftCaption(nextCaption);
  };

  const selectImageBlock = () => {
    if (!editor.isEditable || typeof getPos !== "function") return;
    const position = getPos();
    if (typeof position !== "number") return;
    editor.view.dispatch(
      editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, position)),
    );
    editor.view.focus();
  };

  const imageActions: EditorMediaAction[] = [
    ...(serverAttachment
      ? [
          {
            label: "Attachment history",
            icon: FileText,
            group: "file",
            opensDialog: true,
            onSelect: openAttachmentHistory,
          },
        ]
      : []),
    {
      label: "Open image",
      icon: ExternalLink,
      group: "file",
      onSelect: () => window.open(src, "_blank", "noopener,noreferrer"),
    },
    {
      label: "Download image",
      icon: Download,
      group: "file",
      onSelect: () => void triggerMediaDownload(src, caption || "image"),
    },
    ...(
      [
        ["left", AlignLeft, "Align image left"],
        ["center", AlignCenter, "Align image center"],
        ["right", AlignRight, "Align image right"],
      ] as const
    ).map(([nextAlignment, icon, label]) => ({
      label,
      icon,
      group: "alignment",
      active: alignment === nextAlignment,
      onSelect: () => updateAttributes({ alignment: nextAlignment }),
    })),
    {
      label: "Add to gallery",
      icon: Images,
      group: "convert",
      onSelect: () =>
        window.dispatchEvent(
          new CustomEvent(OPEN_IMAGE_CONVERT_EVENT, {
            detail: {
              editor,
              position: typeof getPos === "function" ? getPos() : null,
              kind: "gallery",
              src,
              alt: caption,
            },
          }),
        ),
    },
    {
      label: "Add to carousel",
      icon: GalleryHorizontalEnd,
      group: "convert",
      onSelect: () =>
        window.dispatchEvent(
          new CustomEvent(OPEN_IMAGE_CONVERT_EVENT, {
            detail: {
              editor,
              position: typeof getPos === "function" ? getPos() : null,
              kind: "carousel",
              src,
              alt: caption,
            },
          }),
        ),
    },
    {
      label: "Replace image",
      icon: ImageUp,
      opensDialog: serverAttachment,
      group: "edit",
      onSelect: () => {
        if (serverAttachment) {
          openAttachmentHistory();
          return;
        }
        window.dispatchEvent(
          new CustomEvent(OPEN_IMAGE_REPLACE_EVENT, {
            detail: {
              editor,
              replace: (nextSrc: string, nextAlt: string, attachmentId?: string | null) =>
                updateAttributes({
                  src: nextSrc,
                  alt: nextAlt,
                  attachmentId: attachmentId ?? null,
                }),
            },
          }),
        );
      },
    },
    {
      label: "Delete image",
      icon: Trash2,
      group: "delete",
      destructive: true,
      onSelect: deleteNode,
    },
  ];

  return (
    <NodeViewWrapper
      as="figure"
      data-image-block="true"
      className={`group/media relative my-4 w-fit max-w-full ${getImageAlignmentClass(alignment)}`}
      contentEditable={false}
      onPointerDown={(event: ReactPointerEvent<HTMLElement>) => {
        const target = event.target as Element;
        if (target.closest("button, a, input, textarea, select")) return;
        event.preventDefault();
        selectImageBlock();
      }}
    >
      <img
        src={src}
        alt={caption}
        draggable={false}
        onPointerDown={(event) => {
          event.preventDefault();
          selectImageBlock();
        }}
        className="rounded-card max-w-full"
      />
      {editor.isEditable || serverAttachment ? (
        <div
          className="absolute top-2 right-2 opacity-0 transition-opacity duration-100 group-focus-within/media:opacity-100 group-hover/media:opacity-100 data-[selected=true]:opacity-100"
          data-selected={selected}
        >
          <EditorMediaActionMenu
            label="Image actions"
            actions={
              editor.isEditable
                ? imageActions
                : imageActions.filter((action) => action.group === "file")
            }
          />
        </div>
      ) : null}
      {historyError?<p role="alert" className="text-danger mt-2 text-sm">{historyError}</p>:null}
      <figcaption className="mt-2 max-w-xl">
        {editor.isEditable && selected ? (
          <label htmlFor={captionId} className="block">
            <span className="text-text-muted text-xs font-medium">Caption</span>
            <Input
              id={captionId}
              value={draftCaption}
              maxLength={IMAGE_CAPTION_MAX_LENGTH}
              onChange={(event) => setDraftCaption(event.target.value)}
              onBlur={commitCaption}
              onKeyDown={(event) => {
                if (event.key === "Enter") event.currentTarget.blur();
              }}
              placeholder="Describe this image"
              className="mt-1 min-h-11 sm:min-h-0"
            />
          </label>
        ) : caption ? (
          <p className="text-text-muted text-sm">{caption}</p>
        ) : null}
      </figcaption>
    </NodeViewWrapper>
  );
}
