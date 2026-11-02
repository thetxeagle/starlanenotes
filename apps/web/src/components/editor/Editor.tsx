import {prepareAttachmentCopy,prepareLegacyAttachmentCopy,fetchLegacyAttachmentCopySource,runAttachmentCopy,markAttachmentCopyAdopted} from "@/lib/attachmentCopyOperation";
import { captureAssetInsertionRange } from "@/lib/assetInsertionRange";
import { useAssetInsertionScope } from "@/hooks/useAssetInsertionScope";
import {directUploadAcknowledgementId,markDirectUploadAdopted} from "@/lib/directUploadReview";
import {releaseBrowserAttachmentImage} from "@/lib/browserAttachmentImage";
import {saveBrowserPinnedAttachment} from "@/lib/browserAttachmentSave";
import {AttachmentHistoryDialog} from './AttachmentHistoryDialog';
import { isTauri as isNativeAttachmentShell, captureOfflineServerContext as captureAttachmentContext, getApiBase as attachmentApiBase, api as attachmentApi } from "@/lib/api";
import {verifyOfflineReplayContext as verifyAttachmentContext} from "@/lib/offlineReplayOrigin";
import {
  normalizeBlockPlacement,
  normalizeTablePlacement,
  type BlockPlacement,
  type TablePlacement,
} from "./blockPlacement";
import {
  lazy,
  Suspense,
  useCallback,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import {
  useEditor,
  EditorContent,
  NodeViewContent,
  NodeViewWrapper,
  ReactNodeViewRenderer,
  type Editor as TipTapEditor,
  type ReactNodeViewProps,
} from "@tiptap/react";
import { Extension, getMarkRange, Mark, mergeAttributes, Node as TiptapNode } from "@tiptap/core";
import { DOMParser as ProseMirrorDOMParser, DOMSerializer, Fragment, type Node as ProseMirrorNode } from "prosemirror-model";
import {
  NodeSelection,
  TextSelection,
  EditorState,
  type Transaction,
} from "prosemirror-state";
import { CellSelection, TableMap } from "prosemirror-tables";
import { EditorZoomContext } from "@/components/editor/EditorZoomContext";
import { useAuth } from "@/hooks/useAuth";
import {
  formatSmartDate,
  SlashCommandPalette,
  type SlashCommand,
  type SlashTemplateField,
} from "@/components/editor/SlashCommands";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SearchInput } from "@/components/ui/search-input";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_ACTIVE_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
  CONTROL_CHROME_SURFACE_CLASS,
  CONTROL_CHROME_TOUCH_ACTION_CLASS,
} from "@/components/ui/control-chrome";
import {
  DialogBody,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import { useOverlayFocusLifecycle } from "@/components/ui/useOverlayFocusLifecycle";
import {
  CalloutHeader,
  normalizeCalloutKind,
  parseCalloutDirective,
  serializeCalloutDirective,
} from "@/components/editor/Callout";
import {
  ExpandNodeView,
  normalizeExpandTitle,
  parseExpandDirective,
  serializeExpandDirective,
} from "@/components/editor/Expand";
import {
  MathBlockEditor,
  parseMathBlockDirective,
  serializeMathBlockDirective,
} from "@/components/editor/MathBlock";
import { api } from "@/lib/api";
import {
  getTemplateSectionFieldIds,
  resolveTemplateSection,
  createTemplateSectionInsertionTransaction,
} from "@/lib/templateSection";
import {
  escapeTemplateMarkerAttribute,
  parseTemplateMarker,
  serializeTemplateMarker,
  type ParsedTemplateMarker,
} from "@/lib/templateMarkers";
import {
  decodeTemplateValue,
  decodeTemplateValueHtmlText,
  encodeTemplateValue,
  serializeTemplateValue,
} from "@/lib/templateValues";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { EditorCommentDialog } from "@/components/editor/EditorCommentDialog";
import { PersistentDocumentToolbar } from "@/components/editor/PersistentDocumentToolbar";
import { clearEditorInstanceIfCurrent } from "@/components/editor/editorInstanceLifecycle";
import {
  InlineCommentDecorations,
  updateInlineCommentDecorations,
} from "@/components/editor/InlineCommentDecorations";
import {
  DraftPeerCursorDecorations,
  updateDraftPeerCursorDecorations,
  type DraftPeerEditorRange,
} from "@/components/editor/DraftPeerCursorDecorations";
import { EditorBookmarkDialog } from "@/components/editor/EditorBookmarkDialog";
import { EditorTagDialog } from "@/components/editor/EditorTagDialog";
import { EditorLinkDialog } from "@/components/editor/EditorLinkDialog";
import {
  EditorWikiLinkDialog,
  type EditorWikiLinkDocument,
} from "@/components/editor/EditorWikiLinkDialog";
import {
  EditorMentionDialog,
  type EditorMentionMember,
} from "@/components/editor/EditorMentionDialog";
import {
  EditorUploadDialog,
  isEditorReusableImage,
  type EditorReusableAsset,
  type EditorUploadProgress,
  type EditorUploadResult,
} from "@/components/editor/EditorUploadDialog";
import {
  ImageBlockNodeView,
  getImageAlignmentMarkdownTitle,
  normalizeImageCaption,
  normalizeImageAlignment,
  OPEN_IMAGE_CONVERT_EVENT,
  OPEN_IMAGE_REPLACE_EVENT,
  OPEN_IMAGE_ATTACHMENT_HISTORY_EVENT,
} from "@/components/editor/ImageBlock";
import {
  EditorMediaActionMenu,
  triggerMediaDownload,
  type EditorMediaAction,
} from "@/components/editor/EditorMediaActionMenu";
import { EmojiPickerContent } from "@/components/editor/EmojiPickerContent";
import {
  EditorUtilityTargetDialog,
  type EditorUtilityTarget,
} from "@/components/editor/EditorUtilityTargetDialog";
import {
  CarouselNodeView,
  DEFAULT_CAROUSEL_TITLE,
  normalizeCarouselItems,
  parseCarouselDirective,
  serializeCarouselDirective,
  serializeCarouselItems,
} from "@/components/editor/CarouselBlock";
import {
  DEFAULT_GALLERY_TITLE,
  GalleryNodeView,
  normalizeGalleryLayout,
  parseGalleryDirective,
  serializeGalleryDirective,
  serializeGalleryItems,
} from "@/components/editor/GalleryBlock";
import { CodeBlockNodeView } from "@/components/editor/CodeBlockNodeView";
import {
  CodeTabsNodeView,
  normalizeCodeTabs,
  parseCodeTabsSource,
  serializeCodeTabs,
} from "@/components/editor/CodeTabsBlock";
import {
  COLUMNS_END_MARKER,
  EditorColumns,
  EditorSection,
  EditorSectionColumn,
  SECTION_COLUMN_MARKER,
  SECTION_COLUMN_MARKER_PREFIX,
  SECTION_END_MARKER,
  SECTION_LAYOUT_OPTIONS,
  addBlockToActiveEditorSection,
  deleteActiveEditorSection,
  getActiveEditorSection,
  getSectionColumnCount,
  getSectionGridTemplateColumns,
  getSectionStartMarker,
  getColumnsStartMarker,
  inferSectionLayoutFromColumnCount,
  getSectionLabelSlug,
  insertActiveEditorSection,
  moveActiveEditorSection,
  normalizeSectionLayout,
  parseSectionStartMarker,
  parseSectionStartOptions,
  parseColumnsStartMarker,
  serializeEditorColumnsMarkdown,
  serializeEditorSectionMarkdown,
  type SectionStyle,
  setActiveEditorSectionLayout,
  setActiveEditorSectionStyle,
  type ActiveEditorSection,
  type SectionLayout,
} from "@/components/editor/EditorSection";
import { createStarlaneBlockInteractionExtensions } from "@/components/editor/slashEditorBridge";
import { TemplateMarkerNode, TemplateValueNode } from "@/components/editor/TemplateMarkerNode";
import {
  createBlockMoveTransaction,
  resolveMovableBlock,
} from "@/components/editor/slashEditorBlockMove";
import {
  canStartInlineSlashMenu,
  deriveInlineSlashQuery,
  findInlineSlashQuery,
} from "@/components/editor/slashEditorSlashState";
import {
  createEditorLinkTransaction,
  createEditorUnlinkTransaction,
  normalizeEditorLinkHref,
} from "@/components/editor/slashEditorLink";
import {
  EditorBlockGutter,
  shouldShowEditorBlockGutter,
  type EditorGutterBlockTarget,
} from "@/components/editor/EditorBlockGutter";
import { TableStyleDialog } from "@/components/editor/TableStyleDialog";
import {
  emitDocumentCommentsChanged,
  listDocumentComments,
  subscribeToDocumentComments,
  type DocumentComment,
} from "@/lib/documentComments";
import {
  normalizeTableStyle,
  parseTableStyleDirective,
  parseTablePlacementDirective,
  serializeTableStyleDirective,
  type TableStyleId,
} from "@/components/editor/tableStyles";
import {
  isTableOfContentsDirective,
  TableOfContentsEditorBlock,
  TABLE_OF_CONTENTS_DIRECTIVE,
} from "@/components/editor/TableOfContents";
import {
  ChildPagesBlock,
  CHILD_PAGES_DIRECTIVE,
  COLLAPSIBLE_CHILD_PAGES_DIRECTIVE,
  parseChildPagesDirective,
  type ChildPagesVariant,
} from "@/components/editor/ChildPages";
import type { TemplateRenderContext } from "@/lib/templates";
import {
  dataClient,
  retainedUploadOperationId,
  type DocumentRecord,
  type LocalAssetContext,
  type PropertySchemaRecord,
  type SpaceMemberRecord,
  type TemplateRecord,
} from "@/lib/dataClient";
import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Link from "@tiptap/extension-link";
import ImageExt from "@tiptap/extension-image";
import { Table, TableView } from "@tiptap/extension-table";
import TableRow from "@tiptap/extension-table-row";
import TableHeader from "@tiptap/extension-table-header";
import TableCell from "@tiptap/extension-table-cell";
import Highlight from "@tiptap/extension-highlight";
import Typography from "@tiptap/extension-typography";
import Underline from "@tiptap/extension-underline";
import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight";
import { createLowlight } from "lowlight";
import bash from "highlight.js/lib/languages/bash";
import css from "highlight.js/lib/languages/css";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import ocaml from "highlight.js/lib/languages/ocaml";
import powershell from "highlight.js/lib/languages/powershell";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

export const OPEN_BOOKMARK_EDIT_EVENT = "starlane:open-bookmark-edit";
export const OPEN_FILE_REPLACE_EVENT = "starlane:open-file-replace";
export const WIKI_DOCUMENT_HREF_PREFIX = "starlane:document/";

export function buildWikiDocumentHref(documentId: string): string {
  return `${WIKI_DOCUMENT_HREF_PREFIX}${encodeURIComponent(documentId)}`;
}

export function parseWikiDocumentHref(href: string): string | null {
  if (!href.startsWith(WIKI_DOCUMENT_HREF_PREFIX)) return null;
  try {
    const documentId = decodeURIComponent(href.slice(WIKI_DOCUMENT_HREF_PREFIX.length)).trim();
    return documentId || null;
  } catch {
    return null;
  }
}

export function serializeWikiLinkMarkdown(title: string, documentId?: string | null): string {
  const normalizedTitle = title.trim();
  const normalizedDocumentId = documentId?.trim();
  return normalizedDocumentId
    ? `[[${normalizedTitle}|${normalizedDocumentId}]]`
    : `[[${normalizedTitle}]]`;
}

function BookmarkCardNodeView({ node, editor, getPos }: ReactNodeViewProps) {
  const href = String(node.attrs.href || "");
  const title = String(node.attrs.title || "Bookmark");
  const description = String(node.attrs.description || "");
  const imageUrl = String(node.attrs.imageUrl || "");
  const domain = getBookmarkDomain(href);
  const provider = getBookmarkProvider(href);
  const repository = getBookmarkRepository(href);
  const displayTitle = getBookmarkDisplayTitle(title, href);
  const openEditor = (event: ReactMouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    const position = typeof getPos === "function" ? getPos() : null;
    if (typeof position !== "number") return;
    document.dispatchEvent(
      new CustomEvent(OPEN_BOOKMARK_EDIT_EVENT, {
        detail: { editor, position, href, title, description, imageUrl },
      }),
    );
  };

  return (
    <NodeViewWrapper
      data-bookmark-card="true"
      contentEditable={false}
      className="bookmark-card rounded-card border-success/80 bg-surface my-3 border p-5"
      onPointerDown={(event: ReactPointerEvent<HTMLDivElement>) => {
        if (
          !editor.isEditable ||
          (event.target instanceof Element &&
            event.target.closest("[data-bookmark-control='true']"))
        )
          return;
        const position = typeof getPos === "function" ? getPos() : null;
        if (typeof position !== "number") return;
        event.preventDefault();
        editor.view.dispatch(
          editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, position)),
        );
        editor.view.focus();
      }}
      onClick={(event: ReactMouseEvent<HTMLDivElement>) => {
        if (editor.isEditable) {
          event.preventDefault();
          event.stopPropagation();
        }
      }}
    >
      {imageUrl ? (
        <img
          src={imageUrl}
          alt=""
          loading="lazy"
          referrerPolicy="no-referrer"
          className="mb-3 max-h-48 w-full rounded-md object-cover"
        />
      ) : null}
      {provider ? (
        <div className="bookmark-card__provider" aria-label={`${provider} link`}>
          <span className="bookmark-card__provider-name">{provider}</span>
          {repository ? <span className="bookmark-card__repository">{repository}</span> : null}
        </div>
      ) : null}
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          {editor.isEditable ? (
            <button
              type="button"
              data-bookmark-control="true"
              className="bookmark-card__title text-left"
              onClick={openEditor}
            >
              {displayTitle}
            </button>
          ) : (
            <a href={href} target="_blank" rel="noreferrer" className="bookmark-card__title">
              {displayTitle}
            </a>
          )}
          {description ? (
            <p className="bookmark-card__description">{description}</p>
          ) : (
            <p className="bookmark-card__description bookmark-card__description--fallback">
              {href}
            </p>
          )}
          <span className="bookmark-card__domain">
            <Bookmark size={12} aria-hidden="true" />
            {domain}
          </span>
        </div>
        {editor.isEditable ? (
          <button
            type="button"
            data-bookmark-control="true"
            aria-label="Edit bookmark"
            title="Edit bookmark"
            className="rounded-button border-border bg-surface-raised text-text-muted hover:text-text border p-2"
            onClick={openEditor}
          >
            <Pencil size={14} aria-hidden="true" />
          </button>
        ) : null}
      </div>
    </NodeViewWrapper>
  );
}

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
      imageUrl: {
        default: "",
        parseHTML: (element) => element.getAttribute("image-url") || "",
        renderHTML: (attributes) =>
          attributes.imageUrl ? { "image-url": attributes.imageUrl } : {},
      },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-bookmark-card]" }];
  },
  renderHTML({ HTMLAttributes }) {
    const href = String(HTMLAttributes.href || "");
    const title = String(HTMLAttributes.title || "Bookmark");
    const description = String(HTMLAttributes.description || "");
    const imageUrl = String(HTMLAttributes.imageUrl || "");
    const domain = getBookmarkDomain(href);
    const provider = getBookmarkProvider(href);
    const repository = getBookmarkRepository(href);
    const displayTitle = getBookmarkDisplayTitle(title, href);
    const image = imageUrl
      ? [
          "img",
          {
            src: imageUrl,
            alt: "",
            loading: "lazy",
            referrerpolicy: "no-referrer",
            class: "mb-3 max-h-48 w-full rounded-md object-cover",
          },
        ]
      : null;
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-bookmark-card": "true",
        class: "bookmark-card my-3 rounded-card border border-success/80 bg-surface p-5",
      }),
      ...(image ? [image] : []),
      ...(provider
        ? [
            [
              "div",
              { class: "bookmark-card__provider", "aria-label": `${provider} link` },
              ["span", { class: "bookmark-card__provider-name" }, provider],
              ...(repository ? [["span", { class: "bookmark-card__repository" }, repository]] : []),
            ],
          ]
        : []),
      [
        "a",
        {
          href,
          target: "_blank",
          rel: "noreferrer",
          class: "bookmark-card__title",
        },
        displayTitle,
      ],
      description
        ? ["p", { class: "bookmark-card__description" }, description]
        : ["p", { class: "bookmark-card__description bookmark-card__description--fallback" }, href],
      [
        "span",
        {
          class: "bookmark-card__domain",
        },
        domain,
      ],
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(BookmarkCardNodeView, {
      stopEvent: ({ event }) =>
        event.target instanceof Element &&
        Boolean(event.target.closest("[data-bookmark-control='true']")),
    });
  },
});

const Expand = TiptapNode.create({
  name: "expand",
  group: "block",
  content: "block+",
  defining: true,
  addAttributes() {
    return {
      title: {
        default: "Click to expand",
        parseHTML: (element) => normalizeExpandTitle(element.getAttribute("data-expand-title")),
        renderHTML: (attributes) => ({
          "data-expand-title": normalizeExpandTitle(attributes.title),
        }),
      },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-expand-block]" }];
  },
  renderHTML({ node, HTMLAttributes }) {
    const title = normalizeExpandTitle(node.attrs.title);
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-expand-block": "true",
        "data-expand-title": title,
      }),
      ["div", { "data-expand-content": "true" }, 0],
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(ExpandNodeView);
  },
});

const Carousel = TiptapNode.create({
  name: "carousel",
  group: "block",
  atom: true,
  selectable: false,
  addAttributes() {
    return {
      title: {
        default: DEFAULT_CAROUSEL_TITLE,
        parseHTML: (element) =>
          element.getAttribute("data-carousel-title") || DEFAULT_CAROUSEL_TITLE,
        renderHTML: (attributes) => ({
          "data-carousel-title": String(attributes.title || DEFAULT_CAROUSEL_TITLE),
        }),
      },
      items: {
        default: [],
        parseHTML: (element) => normalizeCarouselItems(element.getAttribute("data-carousel-items")),
        renderHTML: (attributes) => ({
          "data-carousel-items": serializeCarouselItems(normalizeCarouselItems(attributes.items)),
        }),
      },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-carousel-block]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-carousel-block": "true",
      }),
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(CarouselNodeView);
  },
});

const Gallery = TiptapNode.create({
  name: "gallery",
  group: "block",
  atom: true,
  selectable: false,
  addAttributes() {
    return {
      title: {
        default: DEFAULT_GALLERY_TITLE,
        parseHTML: (element) => element.getAttribute("data-gallery-title") || DEFAULT_GALLERY_TITLE,
        renderHTML: (attributes) => ({
          "data-gallery-title": String(attributes.title || DEFAULT_GALLERY_TITLE),
        }),
      },
      layout: {
        default: "responsive-columns",
        parseHTML: (element) => normalizeGalleryLayout(element.getAttribute("data-gallery-layout")),
        renderHTML: (attributes) => ({
          "data-gallery-layout": normalizeGalleryLayout(attributes.layout),
        }),
      },
      items: {
        default: [],
        parseHTML: (element) => normalizeCarouselItems(element.getAttribute("data-gallery-items")),
        renderHTML: (attributes) => ({
          "data-gallery-items": serializeGalleryItems(normalizeCarouselItems(attributes.items)),
        }),
      },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-gallery-block]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["div", mergeAttributes(HTMLAttributes, { "data-gallery-block": "true" })];
  },
  addNodeView() {
    return ReactNodeViewRenderer(GalleryNodeView);
  },
});

type EditorAttachmentHistoryScope = {draftId?:string;verify:()=>void;verifyEdit?:()=>void;captureReplacement?:()=>()=>void};

function FileAttachmentNodeView({
  node,
  editor,
  getPos,
  deleteNode,
  updateAttributes,
  selected,
}: ReactNodeViewProps) {
  const [downloading, setDownloading] = useState(false);
  const [historyScope, setHistoryScope] = useState<EditorAttachmentHistoryScope | null>(null);
  const [downloadError, setDownloadError] = useState("");
  const name = String(node.attrs.name || "Attachment");
  const size = Number(node.attrs.size || 0);
  const url = String(node.attrs.url || "");
  const attachmentId = String(node.attrs.attachmentId || "");
  const currentAttachment = useRef(attachmentId);
  currentAttachment.current = attachmentId;
  const options = editor.extensionManager.extensions.find(
    (extension) => extension.name === "fileAttachment",
  )?.options as { spaceId?: string; docId?: string; captureSelection?:()=>EditorAttachmentHistoryScope } | undefined;
  const openFileHistory = () => {
    try {
      const scope = options?.captureSelection?.();
      if (!scope) throw new Error("Original attachment selection unavailable; reopen this page before inspecting history.");
      const originalPin = attachmentId;
      const verify = () => {
        scope.verify();
        const position = typeof getPos === "function" ? getPos() : undefined;
        const currentNode = typeof position === "number" ? editor.state.doc.nodeAt(position) : null;
        // Display hydration may replace node metadata without changing its exact pin.
        // Reading that version does not adopt a replacement into the document.
        if (editor.isDestroyed || currentAttachment.current !== originalPin ||
          currentNode?.type.name !== "fileAttachment" || currentNode.attrs.attachmentId !== originalPin)
          throw new Error("Original attachment selection changed; inspect its retained replacement before use.");
      };
      const verifyEdit = scope.verifyEdit ? ()=>{scope.verifyEdit!();verify();} : undefined;
      const captureReplacement = verifyEdit ? () => {
        verifyEdit();
        const originalPosition = typeof getPos === "function" ? getPos() : undefined;
        const originalNode = typeof originalPosition === "number" ? editor.state.doc.nodeAt(originalPosition) : null;
        return () => {
          verifyEdit();
          const position = typeof getPos === "function" ? getPos() : undefined;
          if (!originalNode || typeof position !== "number" || editor.state.doc.nodeAt(position) !== originalNode)
            throw new Error("Original replacement block changed; retain its acknowledged operation for review.");
        };
      } : undefined;
      verify();
      setHistoryScope({draftId:scope.draftId,verify,verifyEdit,captureReplacement});
    } catch (error) { setDownloadError(error instanceof Error ? error.message : "Original attachment history unavailable."); }
  };

  const startDownload = async () => {
    if (downloading) return;
    setDownloading(true);
    setDownloadError("");
    try {
      if(getCurrentLaunchMode() !== "local" && attachmentId && options?.spaceId && options.docId) {
        const selectedContext=captureAttachmentContext();
        if(!attachmentId||!options?.spaceId||!options.docId)throw new Error("Exact attachment version and current page required for native Save file.");
        if(isNativeAttachmentShell()) await dataClient.savePinnedDocumentAttachmentVersion(options.spaceId,options.docId,attachmentId);
        else await saveBrowserPinnedAttachment({space_id:options.spaceId,document_id:options.docId},attachmentId,name);
        verifyAttachmentContext(selectedContext,attachmentApiBase(),attachmentApi.getToken());
        return;
      }
      let downloadUrl = url;
      if (!downloadUrl && attachmentId && options?.spaceId && options.docId) {
        const attachments = await dataClient.listDocumentAttachments(
          options.spaceId,
          options.docId,
        );
        const attachment = attachments.find((candidate) => candidate.id === attachmentId);
        if (!attachment) throw new Error("This attachment is no longer available.");
        downloadUrl = await dataClient.downloadDocumentAttachmentToVault(
          options.spaceId,
          options.docId,
          attachment,
        );
      }
      if (!downloadUrl) throw new Error("This attachment does not have downloadable data.");

      await triggerMediaDownload(downloadUrl, name);
    } catch (error) {
      setDownloadError(error instanceof Error ? error.message : "Could not download this file.");
    } finally {
      setDownloading(false);
    }
  };

  const selectFileBlock = () => {
    if (!editor.isEditable || typeof getPos !== "function") return;
    const position = getPos();
    if (typeof position !== "number") return;
    editor.view.dispatch(
      editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, position)),
    );
    editor.view.focus();
  };

  const fileActions: EditorMediaAction[] = [
    {
      label: "Attachment history",
      opensDialog: true,
      icon: FileText,
      group: "file",
      disabled: !attachmentId || !options?.spaceId || !options.docId,
      onSelect: openFileHistory,
    },
    {
      label: downloading ? `Downloading ${name}…` : "Download file",
      icon: Download,
      group: "file",
      disabled: downloading,
      onSelect: () => void startDownload(),
    },
    {
      label: "Replace file",
      opensDialog: true,
      icon: Upload,
      group: "edit",
      onSelect: () => {
        if (attachmentId && options?.spaceId && options.docId) {openFileHistory(); return;}
        document.dispatchEvent(
          new CustomEvent(OPEN_FILE_REPLACE_EVENT, {
            detail: {
              editor,
              replace: (
                nextName: string,
                nextSize: number,
                nextUrl: string,
                nextAttachmentId?: string | null,
              ) =>
                updateAttributes({
                  name: nextName,
                  size: nextSize,
                  url: nextUrl,
                  attachmentId: nextAttachmentId ?? null,
                }),
            },
          }),
        );
      },
    },
    {
      label: "Delete file",
      icon: Trash2,
      group: "delete",
      destructive: true,
      onSelect: deleteNode,
    },
  ];

  return (
    <NodeViewWrapper
      data-type="file-attachment"
      data-name={name}
      data-size={String(size)}
      data-url={url}
      data-attachment-id={attachmentId || undefined}
      className="file-attachment-card group/media"
      contentEditable={false}
      data-selected={selected}
      onPointerDown={(event: ReactPointerEvent<HTMLDivElement>) => {
        const target = event.target as Element;
        if (target.closest("button, a, input, textarea, select")) return;
        event.preventDefault();
        selectFileBlock();
      }}
    >
      <FileText size={20} className="file-attachment-card__icon" aria-hidden="true" />
      <span className="file-attachment-card__content">
        <strong>{name}</strong>
        <span>{formatFileSize(size)}</span>
      </span>
      <EditorMediaActionMenu label={`${name} actions`} actions={fileActions} />
      {historyScope && options?.spaceId && options.docId && attachmentId ? <AttachmentHistoryDialog
        spaceId={options.spaceId} documentId={options.docId} versionId={attachmentId} draftId={historyScope.draftId}
        verifySelection={historyScope.verify}
        captureReplacementSelection={historyScope.captureReplacement}
        onReplaced={editor.isEditable && historyScope.verifyEdit ? result => {
          historyScope.verifyEdit!();
          updateAttributes({name:result.version.original_name,size:result.version.byte_length,url:"",attachmentId:result.version.version_id});
          setHistoryScope(current=>current===historyScope?null:current);
        } : undefined}
        onClose={()=>setHistoryScope(current=>current===historyScope?null:current)}
      /> : null}
      {downloadError ? (
        <p role="alert" className="text-danger file-attachment-card__error text-xs">
          {downloadError}
        </p>
      ) : null}
    </NodeViewWrapper>
  );
}

const FileAttachment = TiptapNode.create({
  name: "fileAttachment",
  group: "block",
  atom: true,
  selectable: true,
  addOptions() {
    return { spaceId: undefined as string | undefined, docId: undefined as string | undefined,
      captureSelection: undefined as (()=>EditorAttachmentHistoryScope) | undefined };
  },
  addAttributes() {
    return {
      name: {
        default: "Attachment",
        parseHTML: (element) =>
          element.getAttribute("data-name") || element.textContent?.trim() || "Attachment",
      },
      size: {
        default: 0,
        parseHTML: (element) => Number(element.getAttribute("data-size") || 0),
      },
      url: { default: "", parseHTML: (element) => element.getAttribute("data-url") || "" },
      attachmentId: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-attachment-id"),
        renderHTML: (attributes) =>
          attributes.attachmentId ? { "data-attachment-id": String(attributes.attachmentId) } : {},
      },
    };
  },
  parseHTML() {
    return [{ tag: 'div[data-type="file-attachment"]' }];
  },
  renderHTML({ node, HTMLAttributes }) {
    const name = String(node.attrs.name || "Attachment");
    const size = Number(node.attrs.size || 0);
    const url = String(node.attrs.url || "");
    const attachmentId = String(node.attrs.attachmentId || "");
    const label = url
      ? [
          "a",
          {
            href: url,
            class: "starlane-link",
            target: "_blank",
            rel: "noopener noreferrer",
          },
          name,
        ]
      : ["strong", {}, name];
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-type": "file-attachment",
        "data-name": name,
        "data-size": String(size),
        "data-url": url,
        ...(attachmentId ? { "data-attachment-id": attachmentId } : {}),
        class: "file-attachment-card",
        contenteditable: "false",
      }),
      ["p", {}, label, " ", ["em", {}, `(${formatFileSize(size)})`]],
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(FileAttachmentNodeView);
  },
});

function TableOfContentsNodeView({ editor }: ReactNodeViewProps) {
  return (
    <NodeViewWrapper contentEditable={false} data-table-of-contents="true">
      <TableOfContentsEditorBlock editor={editor} />
    </NodeViewWrapper>
  );
}

function ChildPagesNodeView({ node, editor }: ReactNodeViewProps) {
  const options = editor.extensionManager.extensions.find(
    (extension) => extension.name === "childPages",
  )?.options as { spaceId?: string; docId?: string } | undefined;
  return (
    <NodeViewWrapper
      contentEditable={false}
      data-child-pages="true"
      data-child-pages-variant={node.attrs.variant}
    >
      <ChildPagesBlock
        spaceId={options?.spaceId}
        docId={options?.docId}
        variant={node.attrs.variant as ChildPagesVariant}
        interactive={false}
      />
    </NodeViewWrapper>
  );
}

function CalloutNodeView({ node, updateAttributes }: ReactNodeViewProps) {
  const kind = normalizeCalloutKind(String(node.attrs.kind || "note"));

  return (
    <NodeViewWrapper
      as="aside"
      data-callout="true"
      data-callout-kind={kind}
      role="note"
      aria-label={`${kind} callout`}
      className="rounded-card border-border bg-surface my-4 border p-4"
    >
      <CalloutHeader
        kind={kind}
        editable
        onKindChange={(nextKind) => updateAttributes({ kind: nextKind })}
      />
      <NodeViewContent className="text-text mt-3 min-h-6 text-sm leading-6 outline-none" />
    </NodeViewWrapper>
  );
}

const Callout = TiptapNode.create({
  name: "callout",
  group: "block",
  content: "inline*",
  defining: true,
  addAttributes() {
    return {
      kind: {
        default: "note",
        parseHTML: (element) => normalizeCalloutKind(element.getAttribute("data-callout-kind")),
      },
    };
  },
  parseHTML() {
    return [{ tag: "aside[data-callout]" }];
  },
  renderHTML({ HTMLAttributes }) {
    const kind = normalizeCalloutKind(String(HTMLAttributes.kind || "note"));
    return [
      "aside",
      mergeAttributes(HTMLAttributes, {
        "data-callout": "true",
        "data-callout-kind": kind,
        role: "note",
        "aria-label": `${kind} callout`,
      }),
      0,
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(CalloutNodeView);
  },
});

function MathBlockNodeView({ node, updateAttributes }: ReactNodeViewProps) {
  return (
    <NodeViewWrapper data-math-block="true" contentEditable={false}>
      <MathBlockEditor
        expression={String(node.attrs.expression || "")}
        onExpressionChange={(expression) => updateAttributes({ expression })}
      />
    </NodeViewWrapper>
  );
}

const MathBlock = TiptapNode.create({
  name: "mathBlock",
  group: "block",
  atom: true,
  selectable: false,
  addAttributes() {
    return {
      expression: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-expression") || "",
      },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-math-block]" }];
  },
  renderHTML({ HTMLAttributes }) {
    const expression = String(HTMLAttributes.expression || "");
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-math-block": "true",
        "data-expression": expression,
        "aria-label": "Math block",
      }),
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(MathBlockNodeView);
  },
});

const TableOfContents = TiptapNode.create({
  name: "tableOfContents",
  group: "block",
  atom: true,
  selectable: false,
  parseHTML() {
    return [{ tag: "div[data-table-of-contents]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-table-of-contents": "true",
      }),
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(TableOfContentsNodeView);
  },
});

const ChildPages = TiptapNode.create({
  name: "childPages",
  group: "block",
  atom: true,
  selectable: true,
  addOptions() {
    return { spaceId: undefined as string | undefined, docId: undefined as string | undefined };
  },
  addAttributes() {
    return {
      variant: {
        default: "list",
        parseHTML: (element) =>
          element.getAttribute("data-child-pages-variant") === "collapsible"
            ? "collapsible"
            : "list",
      },
    };
  },
  parseHTML() {
    return [{ tag: 'div[data-child-pages="true"]' }];
  },
  renderHTML({ node, HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-child-pages": "true",
        "data-child-pages-variant": node.attrs.variant || "list",
      }),
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(ChildPagesNodeView);
  },
});

const StarlaneCodeBlock = CodeBlockLowlight.extend({
  selectable: true,
  addAttributes() {
    return {
      ...(this.parent?.() ?? {}),
      placement: {
        default: "expand",
        parseHTML: (element) => {
          const placement = element.getAttribute("data-code-placement");
          return placement ? normalizeBlockPlacement(placement) : "expand";
        },
        renderHTML: (attributes) =>
          attributes.placement === "expand"
            ? {}
            : { "data-code-placement": normalizeBlockPlacement(attributes.placement) },
      },
      title: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-code-title") || "",
        renderHTML: (attributes) =>
          attributes.title ? { "data-code-title": String(attributes.title).slice(0, 120) } : {},
      },
    };
  },
  addNodeView() {
    return ReactNodeViewRenderer(CodeBlockNodeView, {
      stopEvent: ({ event }) =>
        event.target instanceof Element &&
        Boolean(event.target.closest("[data-code-block-edit-controls='true']")),
    });
  },
});

const CodeTabs = TiptapNode.create({
  name: "codeTabs",
  group: "block",
  atom: true,
  selectable: false,
  addAttributes() {
    return {
      title: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-code-tabs-title") || "",
        renderHTML: (attributes) =>
          attributes.title
            ? { "data-code-tabs-title": String(attributes.title).slice(0, 120) }
            : {},
      },
      tabs: {
        default: [{ label: "Tab 1", language: "", code: "" }],
        parseHTML: (element) => {
          try {
            return normalizeCodeTabs(
              JSON.parse(element.getAttribute("data-code-tabs-items") || "[]"),
            );
          } catch {
            return normalizeCodeTabs([]);
          }
        },
        renderHTML: (attributes) => ({
          "data-code-tabs-items": JSON.stringify(normalizeCodeTabs(attributes.tabs)),
        }),
      },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-code-tabs-block]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["div", mergeAttributes(HTMLAttributes, { "data-code-tabs-block": "true" })];
  },
  addNodeView() {
    return ReactNodeViewRenderer(CodeTabsNodeView);
  },
});

class StarlaneTableView extends TableView {
  constructor(node: ProseMirrorNode, cellMinWidth: number) {
    super(node, cellMinWidth);
    this.syncTableStyle(node);
  }

  update(node: ProseMirrorNode) {
    const updated = super.update(node);
    if (updated) this.syncTableStyle(node);
    return updated;
  }

  private syncTableStyle(node: ProseMirrorNode) {
    this.table.dataset.tableStyle = normalizeTableStyle(node.attrs.tableStyle);
    this.dom.dataset.blockPlacement = normalizeTablePlacement(node.attrs.placement);
    this.table.dataset.hasHeader = String(node.firstChild?.firstChild?.type.name === "tableHeader");
  }
}

const StarlaneTable = Table.extend({
  addAttributes() {
    return {
      ...(this.parent?.() ?? {}),
      placement: {
        default: "center",
        parseHTML: (element) =>
          normalizeTablePlacement(element.getAttribute("data-table-placement")),
        renderHTML: (attributes) =>
          attributes.placement === "center"
            ? {}
            : { "data-table-placement": normalizeTablePlacement(attributes.placement) },
      },
      tableStyle: {
        default: "directory",
        parseHTML: (element) => normalizeTableStyle(element.getAttribute("data-table-style")),
        renderHTML: (attributes) => ({
          "data-table-style": normalizeTableStyle(attributes.tableStyle),
        }),
      },
    };
  },
});
import {
  ArrowDownAZ,
  ArrowDownZA,
  Bookmark,
  Bold,
  BetweenHorizontalEnd,
  BetweenHorizontalStart,
  BetweenVerticalEnd,
  BetweenVerticalStart,
  Check,
  ChevronLeft,
  ChevronRight,
  Code,
  Command,
  Download,
  ExternalLink,
  FileCode,
  FileText,
  Heading,
  Highlighter,
  IndentDecrease,
  IndentIncrease,
  Italic,
  List,
  ListChecks,
  ListOrdered,
  Link as LinkIcon,
  MessageSquare,
  MoreHorizontal,
  Paintbrush,
  Pencil,
  Quote,
  Redo2,
  ArrowDown,
  ArrowUp,
  AlignCenter,
  AlignLeft,
  AlignRight,
  Rows3,
  Smile,
  Strikethrough,
  TableCellsMerge,
  TableProperties,
  Tag,
  Type,
  Trash2,
  Undo2,
  Underline as UnderlineIcon,
  Unlink,
  Upload,
  Columns2,
  Columns3,
  X,
} from "lucide-react";

// Register only the languages exposed by the code-block picker. Keeping this
// explicit avoids the dozens of grammars in Lowlight's `common` bundle while
// ensuring editor and page-view highlighting use the same language registry.
export const editorLowlight = createLowlight({
  bash,
  css,
  javascript,
  json,
  markdown,
  ocaml,
  powershell,
  python,
  rust,
  sql,
  typescript,
  tsx: typescript,
  xml,
  html: xml,
  yaml,
});

export const EDITOR_DANGER_MENU_ITEM_CLASS =
  "rounded-button block w-full px-2 py-1.5 text-left text-xs text-danger transition-colors duration-100 hover:bg-danger/10 disabled:opacity-40";

/** Guarded clean-source adoption only; no user update or save acknowledgement. */
export type ApplyExternalEditorContent = (content: string, isCurrent: () => boolean, expectedContent: string) => boolean;
export interface EditorSourceSelection { source: string; anchor: number; head: number }
export interface EditorDraftPeerCursor { connectionId: string; displayName: string; source: string; anchor: number; head: number }

export interface EditorProps {
  content: string;
  onChange: (content: string) => void;
  readOnly?: boolean;
  autoFocus?: boolean;
  plainMode?: boolean;
  showBlockChrome?: boolean;
  showMobileCommandButton?: boolean;
  desktopToolbarHost?: HTMLElement | null;
  templates?: TemplateRecord[];
  templateFields?: SlashTemplateField[];
  templateAuthoring?: boolean;
  templateContext?: TemplateRenderContext;
  assetContext?: LocalAssetContext;
  coverAttachmentReference?: string;
  attachmentDraftId?: string;
  spaceId?: string;
  docId?: string;
  inlineCommentsVisible?: boolean;
  onCommentCreated?: () => void;
  onInlineCommentOpen?: (commentId: string) => void;
  onDocumentNavigate?: (documentId: string) => void;
  onEditorReady?: (editor: TipTapEditor) => void;
  onExternalContentReady?: (apply: ApplyExternalEditorContent | null) => void;
  onSourceSelectionChange?: (selection: EditorSourceSelection | null) => void;
  onDraftCommentRequested?: (selection: EditorSourceSelection | null) => void;
  draftPeerCursors?: EditorDraftPeerCursor[];
}

const EDITOR_CURSOR_MARKER = "\uE001";
export function editorPositionToMarkdownOffset(editor: TipTapEditor, position: number, expectedSource: string): number | null {
  if (!Number.isSafeInteger(position) || position < 0 || position > editor.state.doc.content.size || expectedSource.includes(EDITOR_CURSOR_MARKER)) return null;
  const root = editor.view.dom;
  let point: {node: Node; offset: number};
  try { point = editor.view.domAtPos(position); } catch { return null; }
  const path: number[] = [];
  let current: Node | null = point.node;
  while (current && current !== root) {
    const parent: Node | null = current.parentNode;
    if (!parent) return null;
    path.unshift(Array.prototype.indexOf.call(parent.childNodes,current));
    current = parent;
  }
  if (current !== root) return null;
  const clone = root.cloneNode(true) as HTMLElement;
  let target: Node = clone;
  for (const index of path) {
    const child: ChildNode | undefined = target.childNodes[index];
    if (!child) return null;
    target = child;
  }
  if (target.nodeType === Node.TEXT_NODE) {
    const value = target.textContent ?? "";
    if (point.offset < 0 || point.offset > value.length) return null;
    target.textContent = `${value.slice(0,point.offset)}${EDITOR_CURSOR_MARKER}${value.slice(point.offset)}`;
  } else {
    if (point.offset < 0 || point.offset > target.childNodes.length) return null;
    target.insertBefore(document.createTextNode(EDITOR_CURSOR_MARKER),target.childNodes[point.offset] ?? null);
  }
  const marked = htmlToMarkdown(clone.innerHTML);
  const offset = marked.indexOf(EDITOR_CURSOR_MARKER);
  return offset >= 0 && offset === marked.lastIndexOf(EDITOR_CURSOR_MARKER)
    && marked.slice(0,offset) + marked.slice(offset + 1) === expectedSource ? offset : null;
}

export function markdownOffsetToEditorPosition(editor: TipTapEditor, source: string, target: number): number | null {
  if (!Number.isSafeInteger(target) || target < 0 || target > source.length) return null;
  const max = editor.state.doc.content.size;
  const cache = new Map<number,number|null>();
  const mapped = (position:number) => {
    if (!cache.has(position)) cache.set(position,editorPositionToMarkdownOffset(editor,position,source));
    return cache.get(position)!;
  };
  let low = 0, high = max;
  while (low <= high) {
    const middle = Math.floor((low + high) / 2);
    let position = middle, value = mapped(position);
    if (value === null) {
      let found = false;
      for (let distance = 1; distance <= 32 && (middle - distance >= low || middle + distance <= high); distance++) {
        for (const candidate of [middle - distance,middle + distance]) {
          if (candidate < low || candidate > high) continue;
          const candidateValue = mapped(candidate);
          if (candidateValue !== null) { position = candidate; value = candidateValue; found = true; break; }
        }
        if (found) break;
      }
      if (!found) return null;
    }
    if (value === target) return position;
    if (value < target) low = position + 1; else high = position - 1;
  }
  return null;
}

const TemplatePickerDialog = lazy(() => import("@/components/TemplatePickerDialog").then((module) => ({ default: module.TemplatePickerDialog })));
const EditorTemplateSectionDialog = lazy(() => import("@/components/editor/EditorTemplateSectionDialog").then((module) => ({ default: module.EditorTemplateSectionDialog })));
const EditorTemplateGuidanceDialog = lazy(() => import("@/components/editor/EditorTemplateGuidanceDialog").then((module) => ({ default: module.EditorTemplateGuidanceDialog })));

type TemplateInsertionOverlayState =
  | { kind: "schema-loading"; mode: "picker" | "section"; template?: TemplateRecord }
  | { kind: "schema-error"; mode: "picker" | "section"; template?: TemplateRecord }
  | { kind: "picker"; schemas: PropertySchemaRecord[] }
  | { kind: "section"; template: TemplateRecord; schemas: PropertySchemaRecord[] }
  | { kind: "guidance"; content: string };

export { type TipTapEditor };

// Enforce sections in the schema instead of repairing the document from an
// appended transaction. Transaction repair can fight editor plugins that add
// or normalize trailing blocks, creating an unbounded mount-time update loop.
export const SectionDocument = TiptapNode.create({
  name: "doc",
  topNode: true,
  content: "block+",
});

export type EditorBlockTransform =
  | "paragraph"
  | "heading-1"
  | "heading-2"
  | "heading-3"
  | "bullet"
  | "numbered"
  | "task"
  | "quote"
  | "code";

export interface EditorBlockTransformExecutor {
  chain: () => {
    focus: () => {
      setParagraph: () => { run: () => boolean };
      setHeading: (attributes: { level: 1 | 2 | 3 }) => { run: () => boolean };
      toggleBulletList: () => { run: () => boolean };
      toggleOrderedList: () => { run: () => boolean };
      toggleTaskList: () => { run: () => boolean };
      toggleBlockquote: () => { run: () => boolean };
      toggleCodeBlock: () => { run: () => boolean };
    };
  };
}

export function applyEditorBlockTransform(
  editor: EditorBlockTransformExecutor,
  target: EditorBlockTransform,
): boolean {
  const chain = editor.chain().focus();
  switch (target) {
    case "paragraph":
      return chain.setParagraph().run();
    case "heading-1":
      return chain.setHeading({ level: 1 }).run();
    case "heading-2":
      return chain.setHeading({ level: 2 }).run();
    case "heading-3":
      return chain.setHeading({ level: 3 }).run();
    case "bullet":
      return chain.toggleBulletList().run();
    case "numbered":
      return chain.toggleOrderedList().run();
    case "task":
      return chain.toggleTaskList().run();
    case "quote":
      return chain.toggleBlockquote().run();
    case "code":
      return chain.toggleCodeBlock().run();
  }
}

interface SlashState {
  query: string;
  from: number;
  to: number;
  position: { top: number; left: number };
  searchable: boolean;
}

interface TemplateInsertionTarget {
  editor: TipTapEditor;
  doc: ProseMirrorNode;
  from: number;
  to: number;
}

interface SelectionToolbarPosition {
  top: number;
  left: number;
}

export function shouldShowSelectionToolbar({
  mobile,
  readOnly,
  persistentToolbar = false,
  selection,
  inTable,
}: {
  mobile: boolean;
  readOnly: boolean;
  persistentToolbar?: boolean;
  selection: EditorState["selection"];
  inTable: boolean;
}): boolean {
  return (
    !mobile &&
    !readOnly &&
    !persistentToolbar &&
    selection instanceof TextSelection &&
    !selection.empty &&
    !inTable
  );
}

interface BlockMenuState {
  menuPosition: { top: number; left: number };
  target: Pick<TopLevelBlock, "from" | "to" | "index" | "parentStart"> & {
    parentChildCount?: number;
  };
  open: boolean;
  canMoveUp: boolean;
  canMoveDown: boolean;
  transformOpen: boolean;
}

interface ContextMenuState {
  hasSelection: boolean;
  inTable: boolean;
  hasBlock: boolean;
  canMoveBlockUp: boolean;
  canMoveBlockDown: boolean;
  transformOpen: boolean;
}

interface LinkMenuState {
  position: { top: number; left: number };
  from: number;
  to: number;
  text: string;
  href: string;
  draftHref: string;
}

interface CommentDialogState {
  selectionStart?: number;
  selectionEnd?: number;
  selectionText?: string;
}

type LinkDialogState = Omit<LinkMenuState, "position" | "draftHref">;

interface UploadMenuState {
  preferImage: boolean;
  from?: number;
  to?: number;
}

export function isUploadMenuOwnedSelection(
  uploadRange: { from: number; to: number } | null,
  selection: { empty: boolean; from: number; to: number },
): boolean {
  return Boolean(
    uploadRange &&
    !selection.empty &&
    selection.from === uploadRange.from &&
    selection.to === uploadRange.to,
  );
}

interface TagDialogState {
  text: string;
  from: number;
  to: number;
}

interface BookmarkDialogState {
  from: number;
  to: number;
  href: string;
  title: string;
  description?: string;
  imageUrl?: string;
}

interface WikiLinkDialogState {
  from: number;
  to: number;
  documents: EditorWikiLinkDocument[];
  query: string;
  loading: boolean;
  error: boolean;
}
interface MentionDialogState {
  from: number;
  to: number;
  members: SpaceMemberRecord[];
  query: string;
  loading: boolean;
  error: boolean;
}

interface UtilityTargetDialogState {
  kind: "space" | "author";
  from: number;
  to: number;
  targets: EditorUtilityTarget[];
  query: string;
  loading: boolean;
  error: boolean;
}

interface TopLevelBlock {
  node: ProseMirrorNode;
  from: number;
  to: number;
  index: number;
  parentStart?: number;
  parentChildCount?: number;
}

export type MobileToolbarActionId =
  | "undo"
  | "redo"
  | "comment"
  | "link"
  | "bookmark"
  | "upload"
  | "tag"
  | "sectionControls"
  | "blockControls"
  | "heading"
  | "font"
  | "fontSize"
  | "bold"
  | "italic"
  | "underline"
  | "strike"
  | "highlight"
  | "color"
  | "alignment"
  | "emoji"
  | "code"
  | "codeBlock"
  | "quote"
  | "bullet"
  | "numbered"
  | "task"
  | "indent"
  | "outdent"
  | "commands";

export const MOBILE_TOOLBAR_STORAGE_KEY = "starlane_mobile_editor_toolbar";
export const MOBILE_TOOLBAR_UPDATED_EVENT = "starlane:mobile-toolbar-updated";
export const MOBILE_EDITOR_TOOLBAR_BASE_CLASS =
  "mobile-editor-toolbar fixed z-[35] flex items-center gap-2 transition-[bottom] will-change-[bottom]";
export const MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS =
  "left-0 right-0 bg-surface/95 px-2 py-1 shadow-card duration-0";
export const MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS =
  "left-3 right-3 transition-[bottom] duration-150 ease-out";
export const MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS = `${CONTROL_CHROME_RAIL_CLASS} scrollbar-none h-11 flex-1 overflow-x-auto`;
export const MOBILE_EDITOR_TOOLBAR_ACTION_CLASS = CONTROL_CHROME_TOUCH_ACTION_CLASS;

/** Formatting commands stay neutral until the shared active-state owner marks
 * them selected. Semantic status and AI colors do not categorize tools. */
export function getEditorToolbarActionTone(_action: MobileToolbarActionId): string {
  return "text-text-muted";
}
export const MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS = `${CONTROL_CHROME_SURFACE_CLASS} ${CONTROL_CHROME_ACTION_CLASS} active:bg-surface-raised flex h-11 w-11 shrink-0 items-center justify-center`;
export const TABLE_EDITOR_TOOLBAR_SURFACE_CLASS = `${MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS} max-w-[min(100vw-1rem,48rem)] rounded-button shadow-card`;
export const MOBILE_TABLE_TOOLBAR_BASE_CLASS =
  "mobile-editor-toolbar fixed z-[60] flex items-center transition-[bottom] will-change-[bottom]";

export function getToolbarPopoverPosition(
  rect: Pick<DOMRect, "top" | "bottom" | "left" | "width">,
  viewport: Pick<Window, "innerWidth" | "innerHeight">,
  menuSize = { width: 192, height: 180 },
): { top: number; left: number } {
  const gap = 4;
  const topBelow = rect.bottom + gap;
  const top =
    topBelow + menuSize.height <= viewport.innerHeight - 8
      ? topBelow
      : Math.max(8, rect.top - menuSize.height - gap);
  const left = Math.max(8, Math.min(rect.left, viewport.innerWidth - menuSize.width - 8));
  return { top, left };
}

function AnchoredAlignmentMenu({
  open,
  onOpenChange,
  onSelect,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (alignment: "left" | "center" | "right") => void;
  children: ReactNode;
}) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const handleMenuKeyDown = useMenuKeyboardNavigation(menuRef);

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent
        align="start"
        side="bottom"
        sideOffset={6}
        className="w-48 p-1"
        role="menu"
        aria-label="Text alignment"
      >
        <div ref={menuRef} onKeyDown={handleMenuKeyDown}>
          <p className="text-text-muted px-3 py-2 text-xs font-medium">Text alignment</p>
          {EDITOR_TEXT_ALIGNMENT_OPTIONS.map(({ label, value, icon: Icon }) => (
            <button
              key={value}
              type="button"
              role="menuitemradio"
              onClick={() => onSelect(value)}
              className="starlane-menu-item flex w-full items-center gap-3"
            >
              <Icon size={15} />
              {label}
            </button>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}

function AnchoredEmojiMenu({
  open,
  onOpenChange,
  onSelect,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (emoji: string) => void;
  children: ReactNode;
}) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const handleMenuKeyDown = useMenuKeyboardNavigation(menuRef);

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent
        align="start"
        side="bottom"
        sideOffset={6}
        className="starlane-emoji-picker w-[22.5rem] max-w-[calc(100vw-1rem)] p-2"
        role="menu"
        aria-label="Emoji"
      >
        <div ref={menuRef} onKeyDown={handleMenuKeyDown}>
          <EmojiPickerContent onSelect={onSelect} />
        </div>
      </PopoverContent>
    </Popover>
  );
}
const EDITOR_CANVAS_CHROME_ROLES = ["dialog", "menu", "toolbar"];
const EDITOR_CANVAS_CHROME_SELECTOR = [
  "a[href]",
  "button",
  "input",
  "select",
  "textarea",
  ...EDITOR_CANVAS_CHROME_ROLES.map((role) => `[role='${role}']`),
  "[data-starlane-code-frame]",
].join(",");

export interface EditorCanvasPointerContext {
  readOnly: boolean;
  primaryButton: boolean;
  targetInsideEditor: boolean;
  targetInsideChrome: boolean;
}

export function shouldClaimEditorCanvasPointer({
  readOnly,
  primaryButton,
  targetInsideEditor,
  targetInsideChrome,
}: EditorCanvasPointerContext): boolean {
  return !readOnly && primaryButton && !targetInsideEditor && !targetInsideChrome;
}

/**
 * Source view and local browser assets use only image-only data URLs. Keep the
 * editor parser aligned with markdownToHtml, which already rejects executable
 * data URLs and every non-image protocol.
 */
export const EDITOR_IMAGE_OPTIONS = {
  allowBase64: true,
  HTMLAttributes: { class: "rounded-card max-w-full" },
};

export const DEFAULT_MOBILE_TOOLBAR_ACTIONS: MobileToolbarActionId[] = [
  "undo",
  "redo",
  "commands",
  "emoji",
  "heading",
  "font",
  "fontSize",
  "bold",
  "italic",
  "underline",
  "strike",
  "color",
  "highlight",
  "code",
  "alignment",
  "bullet",
  "numbered",
  "task",
  "indent",
  "outdent",
  "link",
  "bookmark",
  "tag",
  "codeBlock",
  "quote",
  "upload",
  "sectionControls",
  "blockControls",
];

export const EDITOR_TEXT_COLOR_OPTIONS = [
  { label: "Default", value: null },
  { label: "Black", value: "#000000" },
  { label: "Slate", value: "#94a3b8" },
  { label: "Red", value: "#f87171" },
  { label: "Orange", value: "#fb923c" },
  { label: "Yellow", value: "#facc15" },
  { label: "Green", value: "#4ade80" },
  { label: "Cyan", value: "#22d3ee" },
  { label: "Blue", value: "#60a5fa" },
  { label: "Violet", value: "#a78bfa" },
  { label: "Pink", value: "#f472b6" },
] as const;

export const EDITOR_HIGHLIGHT_COLOR_OPTIONS = [
  { label: "None", value: null },
  { label: "Yellow", value: "var(--editor-highlight-yellow)" },
  { label: "Green", value: "var(--editor-highlight-green)" },
  { label: "Blue", value: "var(--editor-highlight-blue)" },
  { label: "Pink", value: "var(--editor-highlight-pink)" },
  { label: "Orange", value: "var(--editor-highlight-orange)" },
  { label: "Purple", value: "var(--editor-highlight-purple)" },
] as const;

export const EDITOR_FONT_SIZE_OPTIONS = ["12px", "14px", "16px", "18px", "24px", "32px"] as const;
export const EDITOR_TEXT_ALIGNMENT_OPTIONS = [
  { label: "Left", value: "left", icon: AlignLeft },
  { label: "Center", value: "center", icon: AlignCenter },
  { label: "Right", value: "right", icon: AlignRight },
] as const;
const ALL_MOBILE_TOOLBAR_ACTIONS: MobileToolbarActionId[] = [
  ...DEFAULT_MOBILE_TOOLBAR_ACTIONS,
  "commands",
];

const MOBILE_TOOLBAR_ACTION_SET = new Set<MobileToolbarActionId>(ALL_MOBILE_TOOLBAR_ACTIONS);

export function getAvailableMobileToolbarActionIds(
  activeActions: readonly MobileToolbarActionId[],
): MobileToolbarActionId[] {
  return ALL_MOBILE_TOOLBAR_ACTIONS.filter(
    (action) =>
      action !== "comment" &&
      (action !== "commands" || !activeActions.includes(action)) &&
      !activeActions.includes(action),
  );
}

const CODE_LANGUAGE_OPTIONS = [
  { value: "", label: "Plain text" },
  { value: "css", label: "CSS" },
  { value: "html", label: "HTML" },
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
  { value: "ocaml", label: "OCaml" },
  { value: "yaml", label: "YAML" },
];

export interface EditorFontOption {
  value: string;
  label: string;
}

export interface EditorFontGroup {
  label: string;
  aliases: readonly string[];
  options: readonly EditorFontOption[];
}

const EDITOR_FONT_GROUPS: readonly EditorFontGroup[] = [
  {
    label: "Sans serif",
    aliases: ["sans", "sans-serif", "humanist"],
    options: [
      { value: "", label: "Default (Inter)" },
      { value: "Inter", label: "Inter" },
      { value: "Atkinson Hyperlegible", label: "Atkinson Hyperlegible" },
      { value: "DM Sans", label: "DM Sans" },
      { value: "Figtree", label: "Figtree" },
      { value: "IBM Plex Sans", label: "IBM Plex Sans" },
      { value: "Manrope", label: "Manrope" },
      { value: "Nunito Sans", label: "Nunito Sans" },
      { value: "Plus Jakarta Sans", label: "Plus Jakarta Sans" },
      { value: "Source Sans 3", label: "Source Sans 3" },
      { value: "Space Grotesk", label: "Space Grotesk" },
      { value: "Work Sans", label: "Work Sans" },
    ],
  },
  {
    label: "Serif",
    aliases: ["serif", "book", "editorial"],
    options: [
      { value: "Alegreya", label: "Alegreya" },
      { value: "Cormorant Garamond", label: "Cormorant Garamond" },
      { value: "Lora", label: "Lora" },
      { value: "Merriweather", label: "Merriweather" },
      { value: "Playfair Display", label: "Playfair Display" },
      { value: "Source Serif 4", label: "Source Serif 4" },
    ],
  },
  {
    label: "Monospace",
    aliases: ["mono", "monospace", "code"],
    options: [
      { value: "Fira Code", label: "Fira Code" },
      { value: "IBM Plex Mono", label: "IBM Plex Mono" },
      { value: "JetBrains Mono", label: "JetBrains Mono" },
      { value: "Roboto Mono", label: "Roboto Mono" },
      { value: "Source Code Pro", label: "Source Code Pro" },
      { value: "Space Mono", label: "Space Mono" },
    ],
  },
  {
    label: "Display",
    aliases: ["display", "headline"],
    options: [{ value: "Bebas Neue", label: "Bebas Neue" }],
  },
  {
    label: "Handwriting",
    aliases: ["handwriting", "script"],
    options: [{ value: "Caveat", label: "Caveat" }],
  },
];

export function getEditorFontFamilyCount(): number {
  return EDITOR_FONT_GROUPS.reduce(
    (count, group) => count + group.options.filter((option) => option.value).length,
    0,
  );
}

export function filterEditorFontGroups(query: string): EditorFontGroup[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) return [...EDITOR_FONT_GROUPS];

  const exactCategory = EDITOR_FONT_GROUPS.find(
    (group) =>
      group.label.toLocaleLowerCase() === normalizedQuery ||
      group.aliases.some((alias) => alias === normalizedQuery),
  );
  if (exactCategory) return [exactCategory];

  return EDITOR_FONT_GROUPS.flatMap((group) => {
    const categoryText = `${group.label} ${group.aliases.join(" ")}`.toLocaleLowerCase();
    const options = group.options.filter((option) =>
      `${option.label} ${categoryText}`.toLocaleLowerCase().includes(normalizedQuery),
    );
    return options.length > 0 ? [{ ...group, options }] : [];
  });
}

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

export function isMobileViewportContracted(layoutHeight: number, visualHeight: number): boolean {
  return layoutHeight - visualHeight > 80;
}

export function shouldRestoreEditorSelectionAfterKeyboardClose(input: {
  wasOpen: boolean;
  keyboardOpen: boolean;
  viewportContracted: boolean;
}): boolean {
  return input.wasOpen && !input.keyboardOpen && !input.viewportContracted;
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
  const keyboardOpen = isMobileViewportContracted(baseline, measurement.visualHeight);
  return {
    // iOS pans visualViewport.offsetTop while the keyboard stays open. Keep a
    // non-zero keyboard state during that pan instead of briefly docking the
    // toolbar at the safe area and scheduling a false close restoration.
    inset: keyboardOpen ? Math.max(1, Math.round(rawInset)) : 0,
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
    toolbarBottom:
      "max(calc(0.75rem + var(--starlane-mobile-statusbar-height, 0px)), calc(env(safe-area-inset-bottom) + 0.75rem + var(--starlane-mobile-statusbar-height, 0px)))",
    menuBottom:
      "max(calc(4.75rem + var(--starlane-mobile-statusbar-height, 0px)), calc(env(safe-area-inset-bottom) + 4.75rem + var(--starlane-mobile-statusbar-height, 0px)))",
    reason: "safe-area",
  };
}

export function isEditorSelectionObscured(
  rect: Pick<DOMRect, "top" | "bottom">,
  viewport: { top: number; bottom: number },
  reservedBottom = 56,
): boolean {
  return rect.top < viewport.top + 8 || rect.bottom > viewport.bottom - reservedBottom;
}

export function scrollEditorSelectionIntoView(
  editor: Pick<TipTapEditor, "state" | "view">,
  options: { preferCenter?: boolean } = {},
): void {
  const selection = editor.state.selection;
  const domPosition = editor.view.domAtPos(Math.max(0, selection.from));
  const node =
    domPosition.node instanceof Element ? domPosition.node : domPosition.node.parentElement;
  const target = node?.closest<HTMLElement>(
    "[data-node-view-wrapper], p, h1, h2, h3, h4, h5, h6, li, pre, blockquote",
  );
  if (!target) return;

  const viewport = window.visualViewport;
  const viewportTop = viewport?.offsetTop ?? 0;
  const viewportBottom = viewportTop + (viewport?.height ?? window.innerHeight);
  const obscured = isEditorSelectionObscured(target.getBoundingClientRect(), {
    top: viewportTop,
    bottom: viewportBottom,
  });
  if (!options.preferCenter && !obscured) return;

  target.scrollIntoView({
    block: options.preferCenter ? "center" : "nearest",
    inline: "nearest",
  });
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

const FontSize = Mark.create({
  name: "fontSize",
  addAttributes() {
    return {
      fontSize: {
        default: null,
        parseHTML: (element) => element.style.fontSize || null,
        renderHTML: (attributes) =>
          attributes.fontSize ? { style: `font-size: ${attributes.fontSize}` } : {},
      },
    };
  },
  parseHTML() {
    return [{ tag: "span[style*=font-size]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["span", mergeAttributes(HTMLAttributes), 0];
  },
});

const TextAlignment = Extension.create({
  name: "textAlignment",
  addGlobalAttributes() {
    return [
      {
        types: ["paragraph", "heading", "blockquote"],
        attributes: {
          textAlign: {
            default: null,
            parseHTML: (element) => element.style.textAlign || null,
            renderHTML: (attributes) =>
              attributes.textAlign ? { style: `text-align: ${attributes.textAlign}` } : {},
          },
        },
      },
    ];
  },
});

function sanitizeTextColor(value: string): string {
  const color = value.trim();
  return /^(#[0-9a-f]{3,8}|rgb\((?:\s*\d+\s*,){2}\s*\d+\s*\)|rgba\((?:\s*\d+\s*,){3}\s*(?:0|1|0?\.\d+)\))$/i.test(
    color,
  )
    ? color
    : "";
}

const TextColor = Mark.create({
  name: "textColor",
  addAttributes() {
    return {
      color: {
        default: null,
        parseHTML: (element) => sanitizeTextColor(element.style.color || "") || null,
        renderHTML: (attributes) => {
          const color = sanitizeTextColor(String(attributes.color || ""));
          return color ? { style: `color: ${color}` } : {};
        },
      },
    };
  },
  parseHTML() {
    return [{ tag: "span[style*=color]" }];
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

const ContentMention = Mark.create({
  name: "contentMention",

  addAttributes() {
    return {
      userId: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-user-id"),
        renderHTML: (attributes) =>
          attributes.userId
            ? {
                "data-user-id": attributes.userId,
              }
            : {},
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-type='content-mention']" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-type": "content-mention",
        class: "editor-content-mention",
      }),
      0,
    ];
  },
});

const WikiLinkUnresolved = Mark.create({
  name: "wikiLinkUnresolved",

  addAttributes() {
    return {
      title: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-starlane-wiki-title"),
        renderHTML: (attributes) =>
          attributes.title ? { "data-starlane-wiki-title": attributes.title } : {},
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-starlane-wiki-title]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        class: "starlane-wiki-link starlane-wiki-link--unresolved",
        title: "Page target unavailable",
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
      className="rounded-input border-warning/30 bg-warning/10 text-warning mt-3 flex items-start justify-between gap-3 border px-3 py-2 text-sm"
      role="alert"
    >
      <span>{message}</span>
      <button
        type="button"
        onClick={onDismiss}
        className="rounded-input text-warning hover:bg-warning/10 p-1"
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

export function selectFirstCellInLastTableRow(transaction: Transaction): boolean {
  const { $from } = transaction.selection;
  let tableDepth = -1;
  for (let depth = $from.depth; depth > 0; depth--) {
    if ($from.node(depth).type.name === "table") {
      tableDepth = depth;
      break;
    }
  }
  if (tableDepth < 0) return false;

  const table = $from.node(tableDepth);
  if (table.childCount === 0) return false;
  const map = TableMap.get(table);
  if (map.problems || map.width === 0) return false;

  const tableStart = $from.before(tableDepth) + 1;
  const firstCellPosition = tableStart + map.positionAt(table.childCount - 1, 0, table);
  const cursorPosition = firstCellPosition + 2;
  if (cursorPosition > transaction.doc.content.size) return false;
  transaction.setSelection(TextSelection.near(transaction.doc.resolve(cursorPosition), 1));
  return true;
}

function getCurrentTopLevelBlock(editor: TipTapEditor): TopLevelBlock | null {
  return resolveMovableBlock(editor.state);
}

function getContextMenuState(editor: TipTapEditor | null, hasSelection = false): ContextMenuState {
  const block = editor ? getCurrentTopLevelBlock(editor) : null;
  return {
    hasSelection,
    inTable: editor?.isActive("table") ?? false,
    hasBlock: Boolean(block),
    canMoveBlockUp: Boolean(block && block.index > 0),
    canMoveBlockDown: Boolean(
      block && block.index < (block.parentChildCount ?? editor?.state.doc.childCount ?? 0) - 1,
    ),
    transformOpen: false,
  };
}

function getBlockMenuPosition(
  editor: TipTapEditor,
  block: ReturnType<typeof getCurrentTopLevelBlock>,
) {
  if (!block) return null;

  const dom = editor.view.nodeDOM(block.from);
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

  const centerLeft = Math.max(
    24,
    Math.min(blockRect.left + blockRect.width / 2, window.innerWidth - 24),
  );
  return {
    top:
      blockRect.bottom + 44 < window.innerHeight - 280
        ? blockRect.bottom + 44
        : Math.max(8, blockRect.top - 268),
    left: Math.max(8, Math.min(centerLeft - 88, window.innerWidth - 184)),
  };
}

export function createSiblingParagraphTransaction(
  state: EditorState,
  targetIndex: number,
  placement: "above" | "below",
): { transaction: Transaction; selectionPosition: number } | null {
  const paragraph = state.schema.nodes.paragraph;
  if (!paragraph || targetIndex < 0 || targetIndex >= state.doc.childCount) return null;

  let blockFrom = 0;
  for (let index = 0; index < targetIndex; index++) {
    blockFrom += state.doc.child(index).nodeSize;
  }

  const target = state.doc.child(targetIndex);
  const insertAt = placement === "above" ? blockFrom : blockFrom + target.nodeSize;
  const transaction = state.tr.insert(insertAt, paragraph.create());
  const selectionPosition = Math.min(insertAt + 1, transaction.doc.content.size);
  transaction.setSelection(TextSelection.create(transaction.doc, selectionPosition));
  transaction.setStoredMarks(null);

  return { transaction, selectionPosition };
}

function moveCurrentBlock(editor: TipTapEditor, direction: "up" | "down") {
  const block = getCurrentTopLevelBlock(editor);
  if (!block) return false;

  const targetIndex = direction === "up" ? block.index - 1 : block.index + 1;
  const siblingCount = block.parentChildCount ?? editor.state.doc.childCount;
  if (targetIndex < 0 || targetIndex >= siblingCount) return false;

  let targetFrom = block.parentStart ?? 0;
  for (let index = 0; index < targetIndex; index++) {
    const parent =
      block.parentStart != null && block.parentStart > 0
        ? editor.state.doc.resolve(block.parentStart).parent
        : editor.state.doc;
    targetFrom += parent.child(index).nodeSize;
  }

  const parent =
    block.parentStart != null && block.parentStart > 0
      ? editor.state.doc.resolve(block.parentStart).parent
      : editor.state.doc;
  const target = parent.child(targetIndex);
  const result = createBlockMoveTransaction(editor.state, {
    from: block.from,
    size: block.node.nodeSize,
    to: direction === "up" ? targetFrom : targetFrom + target.nodeSize,
  });
  if (!result) return false;

  editor.view.dispatch(result.transaction);
  editor.view.focus();
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

export function getSelectionToolbarTop(
  selectionTop: number,
  selectionBottom: number,
  toolbarHeight: number,
  viewportHeight: number,
): number {
  const above = selectionTop - toolbarHeight - 8;
  if (above >= 8) return above;

  const below = selectionBottom + 8;
  if (below <= viewportHeight - toolbarHeight - 8) return below;

  return Math.max(8, Math.min(above, viewportHeight - toolbarHeight - 8));
}

function getSelectionFloatingPosition(
  editor: TipTapEditor,
  width = 236,
  height = 40,
): { top: number; left: number } {
  const { from, to } = editor.state.selection;
  const start = editor.view.coordsAtPos(from);
  const end = editor.view.coordsAtPos(to);
  const top = getSelectionToolbarTop(
    Math.min(start.top, end.top),
    Math.max(start.bottom, end.bottom),
    height,
    window.innerHeight,
  );
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

export function getBookmarkRepository(value: string): string | null {
  try {
    const url = new URL(value);
    const segments = url.pathname
      .split("/")
      .map((segment) => decodeURIComponent(segment).trim())
      .filter(Boolean);
    if (segments.length < 2 || !getBookmarkProvider(value)) return null;
    return `${segments[0]}/${segments[1].replace(/\.git$/i, "")}`;
  } catch {
    return null;
  }
}

function getBookmarkDisplayTitle(title: string, href: string): string {
  const normalizedTitle = title.trim();
  const domain = getBookmarkDomain(href);
  if (!normalizedTitle || normalizedTitle === "Bookmark" || normalizedTitle === domain) {
    return getBookmarkRepository(href) || normalizedTitle || domain;
  }
  return normalizedTitle;
}

export function getBookmarkProvider(value: string): "GitHub" | "Gitea" | null {
  try {
    const hostname = new URL(value).hostname.toLowerCase();
    if (hostname === "github.com" || hostname.endsWith(".github.com")) return "GitHub";
    if (hostname === "gitea.io" || hostname.includes("gitea")) return "Gitea";
  } catch {
    return null;
  }
  return null;
}

function renderBookmarkCardHtml(
  href: string,
  title: string,
  description = "",
  imageUrl = "",
): string {
  const safeHref = escapeHtmlAttribute(href);
  const safeDescription = escapeHtml(description);
  const safeImageUrl = escapeHtmlAttribute(imageUrl);
  const domain = escapeHtml(getBookmarkDomain(href));
  const provider = getBookmarkProvider(href);
  const repository = getBookmarkRepository(href);
  const displayTitle = getBookmarkDisplayTitle(title, href);
  const image = imageUrl
    ? `<img src="${safeImageUrl}" alt="" loading="lazy" referrerpolicy="no-referrer" class="mb-3 max-h-48 w-full rounded-md object-cover" />`
    : "";
  const providerMarkup = provider
    ? `<div class="bookmark-card__provider" aria-label="${escapeHtmlAttribute(provider)} link"><span class="bookmark-card__provider-name">${escapeHtml(provider)}</span>${repository ? `<span class="bookmark-card__repository">${escapeHtml(repository)}</span>` : ""}</div>`
    : "";
  return `<div data-bookmark-card="true" href="${safeHref}" title="${escapeHtmlAttribute(title)}" description="${escapeHtmlAttribute(description)}" image-url="${safeImageUrl}" class="bookmark-card my-3 rounded-card border border-success/80 bg-surface p-5" contenteditable="false">${image}${providerMarkup}<a href="${safeHref}" target="_blank" rel="noreferrer" class="bookmark-card__title">${escapeHtml(displayTitle)}</a>${description ? `<p class="bookmark-card__description">${safeDescription}</p>` : `<p class="bookmark-card__description bookmark-card__description--fallback">${safeHref}</p>`}<span class="bookmark-card__domain">${domain}</span></div>`;
}

function getActiveTableElement(editor: TipTapEditor): HTMLTableElement | null {
  const selectionPositions = [editor.state.selection.$from, editor.state.selection.$to];

  for (const $position of selectionPositions) {
    for (let depth = $position.depth; depth > 0; depth--) {
      if ($position.node(depth).type.name !== "table") continue;

      const tablePosition = $position.before(depth);
      const tableDom = editor.view.nodeDOM(tablePosition);
      if (tableDom instanceof HTMLTableElement) return tableDom;
      if (tableDom instanceof HTMLElement) return tableDom.querySelector("table");
    }
  }

  const domAtSelection = editor.view.domAtPos(editor.state.selection.from).node;
  const selectionElement =
    domAtSelection instanceof Element ? domAtSelection : domAtSelection.parentElement;
  const closestTable = selectionElement?.closest("table");
  if (closestTable instanceof HTMLTableElement && editor.view.dom.contains(closestTable)) {
    return closestTable;
  }

  const focusedTable = editor.view.dom.querySelector("table:focus-within");
  if (focusedTable instanceof HTMLTableElement) return focusedTable;

  return null;
}

function getActiveTableToolbarPosition(editor: TipTapEditor): { top: number; left: number } | null {
  const table = getActiveTableElement(editor);
  if (!table) return null;

  const rect = table.getBoundingClientRect();
  const topBar = document
    .querySelector('[data-workspace-top-bar-slot="actions"]')
    ?.closest("header");
  const topBarBottom = topBar?.getBoundingClientRect().bottom ?? 0;
  return {
    top: getTableToolbarTop(rect.top, topBarBottom, window.innerHeight),
    left: Math.max(8, Math.min(rect.left, window.innerWidth - 360)),
  };
}

export function getTableToolbarTop(
  tableTop: number,
  headerBottom: number,
  viewportHeight: number,
  toolbarHeight = 44,
): number {
  const minTop = Math.max(8, headerBottom + 8);
  const maxTop = Math.max(8, viewportHeight - toolbarHeight - 8);
  return Math.max(Math.min(minTop, maxTop), Math.min(tableTop - 44, maxTop));
}

interface ActiveTableColumnContext {
  table: ProseMirrorNode;
  tablePosition: number;
  column: number;
  headerRowCount: number;
  map: TableMap;
}

function getActiveTableColumnContext(editor: TipTapEditor): ActiveTableColumnContext | null {
  const { selection } = editor.state;
  const $from = selection.$from;
  let tableDepth = -1;

  for (let depth = $from.depth; depth > 0; depth--) {
    if ($from.node(depth).type.name === "table") {
      tableDepth = depth;
      break;
    }
  }
  if (tableDepth < 0) return null;

  const table = $from.node(tableDepth);
  const tablePosition = $from.before(tableDepth);
  const tableStart = tablePosition + 1;
  const cellPosition =
    selection instanceof CellSelection
      ? selection.$anchorCell.pos
      : (() => {
          for (let depth = $from.depth; depth > tableDepth; depth--) {
            const node = $from.node(depth);
            if (node.type.name === "tableCell" || node.type.name === "tableHeader") {
              return $from.before(depth);
            }
          }
          return null;
        })();
  if (cellPosition === null) return null;

  const map = TableMap.get(table);
  const column = map.colCount(cellPosition - tableStart);
  if (column < 0 || column >= map.width) return null;

  const simpleRows = Array.from({ length: table.childCount }, (_, index) => table.child(index));
  if (
    map.problems ||
    simpleRows.some(
      (row) =>
        row.childCount !== map.width ||
        Array.from({ length: row.childCount }, (_, index) => row.child(index)).some(
          (cell) => Number(cell.attrs.colspan || 1) !== 1 || Number(cell.attrs.rowspan || 1) !== 1,
        ),
    )
  ) {
    return null;
  }

  const firstRow = simpleRows[0];
  const headerRowCount =
    firstRow && firstRow.childCount > 0 && firstRow.child(0).type.name === "tableHeader" ? 1 : 0;

  return { table, tablePosition, column, headerRowCount, map };
}

export function sortActiveTableByColumn(
  editor: TipTapEditor,
  direction: "ascending" | "descending",
): boolean {
  const context = getActiveTableColumnContext(editor);
  if (!context) return false;

  const rows = Array.from({ length: context.table.childCount }, (_, index) =>
    context.table.child(index),
  );
  const headerRows = rows.slice(0, context.headerRowCount);
  const dataRows = rows.slice(context.headerRowCount);
  if (dataRows.length < 2) return false;

  const multiplier = direction === "ascending" ? 1 : -1;
  const sortedRows = [...dataRows].sort((left, right) => {
    const leftText = left.child(context.column).textContent.trim();
    const rightText = right.child(context.column).textContent.trim();
    return (
      multiplier *
      leftText.localeCompare(rightText, undefined, {
        numeric: true,
        sensitivity: "base",
      })
    );
  });
  const nextTable = context.table.copy(Fragment.fromArray([...headerRows, ...sortedRows]));
  const nextMap = TableMap.get(nextTable);
  const selectedRow = Math.min(context.headerRowCount, nextMap.height - 1);
  const selectedCell =
    context.tablePosition + 1 + nextMap.positionAt(selectedRow, context.column, nextTable);

  editor.view.dispatch(
    editor.state.tr.replaceWith(
      context.tablePosition,
      context.tablePosition + context.table.nodeSize,
      nextTable,
    ),
  );
  editor.commands.setCellSelection({ anchorCell: selectedCell });
  editor.commands.focus();
  return true;
}

export function moveActiveTableColumn(editor: TipTapEditor, direction: "left" | "right"): boolean {
  const context = getActiveTableColumnContext(editor);
  if (!context) return false;

  const destination = context.column + (direction === "left" ? -1 : 1);
  if (destination < 0 || destination >= context.map.width) return false;

  const rows = Array.from({ length: context.table.childCount }, (_, rowIndex) => {
    const row = context.table.child(rowIndex);
    const cells = Array.from({ length: row.childCount }, (_, cellIndex) => row.child(cellIndex));
    [cells[context.column], cells[destination]] = [cells[destination], cells[context.column]];
    return row.copy(Fragment.fromArray(cells));
  });
  const nextTable = context.table.copy(Fragment.fromArray(rows));
  const nextMap = TableMap.get(nextTable);
  const selectedRow = Math.min(context.headerRowCount, nextMap.height - 1);
  const selectedCell =
    context.tablePosition + 1 + nextMap.positionAt(selectedRow, destination, nextTable);

  editor.view.dispatch(
    editor.state.tr.replaceWith(
      context.tablePosition,
      context.tablePosition + context.table.nodeSize,
      nextTable,
    ),
  );
  editor.commands.setCellSelection({ anchorCell: selectedCell });
  editor.commands.focus();
  return true;
}

function getLinkRange(
  editor: TipTapEditor,
  pos = editor.state.selection.from,
  href?: string,
): LinkMenuState | null {
  const markType = editor.state.schema.marks.link;
  if (!markType) return null;

  // DOM positions for inline anchors usually land on the boundary before the
  // first character. Probe inside the anchor first so adjacent links cannot
  // steal the range from the link that was actually clicked.
  const attempts = [pos + 1, pos, pos - 1].filter(
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
      draftHref: rangeHref,
      position: getSelectionFloatingPosition(editor, 384, 40),
    };
  }

  return null;
}

function getLinkRangeAtAnchor(
  editor: TipTapEditor,
  anchor: Element,
  href = anchor.getAttribute("href") || undefined,
): LinkMenuState | null {
  const position = editor.view.posAtDOM(anchor, 0);
  return getLinkRange(editor, position, href);
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

function isEditorFile(value: unknown): value is File {
  if (!value || typeof value !== "object") return false;
  const file = value as Partial<File>;
  return (
    typeof file.name === "string" &&
    typeof file.size === "number" &&
    typeof file.type === "string" &&
    typeof file.arrayBuffer === "function"
  );
}

function insertAttachmentCard(
  editor: TipTapEditor,
  name: string,
  size: number,
  url?: string,
  attachmentId?: string | null,
  slashRange?: { from: number; to: number },
): boolean {
  const insertAt = slashRange ?? editor.state.selection.to;
  const title = escapeHtml(name);
  const label = url
    ? `<a href="${escapeHtmlAttribute(url)}" class="starlane-link" target="_blank" rel="noopener noreferrer">${title}</a>`
    : `<strong>${title}</strong>`;
  const html =
    `<div data-type="file-attachment" data-name="${title}" data-size="${size}" data-url="${escapeHtmlAttribute(url || "")}"${attachmentId ? ` data-attachment-id="${escapeHtmlAttribute(attachmentId)}"` : ""} class="file-attachment-card" contenteditable="false">` +
    `<p>${label} <em>(${formatFileSize(size)})</em></p>` +
    `</div>`;
  return editor.chain().focus().insertContentAt(insertAt, html).run();
}

function isEditorImageFile(file: File): boolean {
  const ext = getExtension(file.name);
  return file.type.startsWith("image/") || EDITOR_IMAGE_EXTENSIONS.has(ext);
}

export function persistMobileToolbarActions(actions: MobileToolbarActionId[]) {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(MOBILE_TOOLBAR_STORAGE_KEY, JSON.stringify(actions));
  window.dispatchEvent(new CustomEvent(MOBILE_TOOLBAR_UPDATED_EVENT, { detail: actions }));
}

export function normalizeMobileToolbarActions(value: unknown): MobileToolbarActionId[] {
  if (!Array.isArray(value)) return DEFAULT_MOBILE_TOOLBAR_ACTIONS;
  const next: MobileToolbarActionId[] = [];

  for (const item of value) {
    if (item === "file" || item === "image") {
      if (!next.includes("upload")) next.push("upload");
      continue;
    }
    if (typeof item === "string" && MOBILE_TOOLBAR_ACTION_SET.has(item as MobileToolbarActionId)) {
      const action = item as MobileToolbarActionId;
      // Comments are a Page view affordance, not an editing control. Drop
      // the legacy preference instead of restoring the old button.
      if (action === "comment") continue;
      if (!next.includes(action)) next.push(action);
    }
  }

  if (next.includes("code") && !next.includes("codeBlock")) {
    next.splice(next.indexOf("code") + 1, 0, "codeBlock");
  }
  if (!next.includes("font")) {
    const headingIndex = next.indexOf("heading");
    next.splice(headingIndex >= 0 ? headingIndex + 1 : next.length, 0, "font");
  }

  // Migrate the pre-format-menu preference once. Older preferences cannot
  // know about Emoji, Font size, or Alignment, so retaining their order would
  // make the new toolbar appear incomplete and out of sequence.
  if (!(next.includes("emoji") || next.includes("fontSize") || next.includes("alignment"))) {
    return DEFAULT_MOBILE_TOOLBAR_ACTIONS;
  }
  const ordered = DEFAULT_MOBILE_TOOLBAR_ACTIONS.filter((action) => next.includes(action));
  return ordered.length > 0 ? ordered : DEFAULT_MOBILE_TOOLBAR_ACTIONS;
}

export function getInitialMobileToolbarActions(): MobileToolbarActionId[] {
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

function contentMentionToHtml(displayName: string, userId: string): string {
  return `<span data-type="content-mention" data-user-id="${escapeHtmlAttribute(userId)}" class="editor-content-mention">@${escapeHtml(displayName)}</span>`;
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

export function getBlockMoveShortcut(
  event: Pick<KeyboardEvent, "altKey" | "ctrlKey" | "key" | "metaKey" | "shiftKey">,
): "up" | "down" | null {
  if (!event.altKey || !event.shiftKey || event.ctrlKey || event.metaKey) return null;
  if (event.key === "ArrowUp") return "up";
  if (event.key === "ArrowDown") return "down";
  return null;
}

export function getMobileToolbarAction(action: MobileToolbarActionId, editor: TipTapEditor | null) {
  const selectionEditor =
    editor && typeof editor.isActive === "function" && typeof editor.getAttributes === "function"
      ? editor
      : null;
  const isActive = (name: string) => (selectionEditor ? selectionEditor.isActive(name) : false);
  const hasAttribute = (name: string, key: string) =>
    selectionEditor ? Boolean(selectionEditor.getAttributes(name)[key]) : false;
  const activeHeadingLevel = selectionEditor
    ? ([1, 2, 3, 4, 5, 6] as const).find((level) => selectionEditor.isActive("heading", { level }))
    : undefined;
  const activeFontSize = selectionEditor
    ? String(selectionEditor.getAttributes("fontSize").fontSize || "")
    : "";
  const fontSizeLabel = activeFontSize || "Aa";
  switch (action) {
    case "undo":
      return { label: "Undo", icon: <Undo2 size={18} />, active: false };
    case "redo":
      return { label: "Redo", icon: <Redo2 size={18} />, active: false };
    case "comment":
      return { label: "Comment", icon: <MessageSquare size={18} />, active: false };
    case "link":
      return { label: "Link", icon: <LinkIcon size={18} />, active: isActive("link") };
    case "bookmark":
      return { label: "Bookmark", icon: <Bookmark size={18} />, active: false };
    case "upload":
      return { label: "Upload", icon: <Upload size={18} />, active: false };
    case "tag":
      return { label: "Tag", icon: <Tag size={18} />, active: false };
    case "sectionControls":
      return isActive("columns")
        ? { label: "Column block controls", icon: <Columns2 size={18} />, active: true }
        : { label: "Section controls", icon: <Rows3 size={18} />, active: false };
    case "blockControls":
      return { label: "Block controls", icon: <MoreHorizontal size={18} />, active: false };
    case "heading":
      return {
        label: activeHeadingLevel ? `Heading ${activeHeadingLevel}` : "Heading",
        icon: activeHeadingLevel ? (
          <span aria-hidden="true" className="text-xs leading-none font-semibold">
            H{activeHeadingLevel}
          </span>
        ) : (
          <Heading size={18} />
        ),
        active: Boolean(activeHeadingLevel),
      };
    case "font":
      return {
        label: "Font family",
        icon: <Type size={18} />,
        active: hasAttribute("fontFamily", "fontFamily"),
      };
    case "fontSize":
      return {
        label: activeFontSize ? `Font size ${activeFontSize}` : "Font size",
        icon: (
          <span aria-hidden="true" className="text-[11px] leading-none font-semibold">
            {fontSizeLabel}
          </span>
        ),
        active: Boolean(activeFontSize),
      };
    case "bold":
      return { label: "Bold", icon: <Bold size={18} />, active: isActive("bold") };
    case "italic":
      return { label: "Italic", icon: <Italic size={18} />, active: isActive("italic") };
    case "underline":
      return {
        label: "Underline",
        icon: <UnderlineIcon size={18} />,
        active: isActive("underline"),
      };
    case "strike":
      return {
        label: "Strikethrough",
        icon: <Strikethrough size={18} />,
        active: isActive("strike"),
      };
    case "highlight":
      return {
        label: "Highlight",
        icon: <Highlighter size={18} />,
        active: isActive("highlight"),
      };
    case "color":
      return {
        label: "Text color",
        icon: <Paintbrush size={18} />,
        active: hasAttribute("textColor", "color"),
      };
    case "alignment":
      return { label: "Text alignment", icon: <AlignLeft size={18} />, active: false };
    case "emoji":
      return { label: "Emoji", icon: <Smile size={18} />, active: false };
    case "code":
      return { label: "Inline code", icon: <Code size={18} />, active: isActive("code") };
    case "codeBlock":
      return {
        label: "Code block",
        icon: <FileCode size={18} />,
        active: isActive("codeBlock"),
      };
    case "quote":
      return { label: "Quote", icon: <Quote size={18} />, active: isActive("blockquote") };
    case "bullet":
      return {
        label: "Bullet list",
        icon: <List size={18} />,
        active: isActive("bulletList"),
      };
    case "numbered":
      return {
        label: "Numbered list",
        icon: <ListOrdered size={18} />,
        active: isActive("orderedList"),
      };
    case "task":
      return {
        label: "Task list",
        icon: <ListChecks size={18} />,
        active: isActive("taskList"),
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
  autoFocus = false,
  plainMode = false,
  showBlockChrome = true,
  showMobileCommandButton = false,
  desktopToolbarHost = null,
  templates = [],
  templateFields = [],
  templateAuthoring = false,
  templateContext = {},
  assetContext = {},
  coverAttachmentReference,
  attachmentDraftId,
  spaceId,
  docId,
  inlineCommentsVisible = false,
  onCommentCreated,
  onInlineCommentOpen,
  onDocumentNavigate,
  onEditorReady,
  onExternalContentReady,
  onSourceSelectionChange,
  onDraftCommentRequested,
  draftPeerCursors = [],
}: EditorProps) {
  const { cssVar } = useContext(EditorZoomContext);
  const { config } = useAuth();
  const initialEditorContentRef = useRef<string | null>(null);
  if (initialEditorContentRef.current === null) {
    initialEditorContentRef.current = markdownToHtml(content, false, templateFields);
  }
  const commentsAvailable = Boolean(spaceId && docId && getCurrentLaunchMode() !== "local");
  const maxPickedFileMb = getConfiguredUploadLimitMb(config?.max_upload_size_mb);
  const maxPickedFileBytes = maxPickedFileMb * 1024 * 1024;
  const attachmentSelectionRef = useRef({spaceId,docId,draftId:attachmentDraftId,readOnly});
  const previousAttachmentSelection = attachmentSelectionRef.current;
  if (previousAttachmentSelection.spaceId !== spaceId || previousAttachmentSelection.docId !== docId ||
    previousAttachmentSelection.draftId !== attachmentDraftId || previousAttachmentSelection.readOnly !== readOnly) {
    attachmentSelectionRef.current = {spaceId,docId,draftId:attachmentDraftId,readOnly};
  }
  const renderedAttachmentSelection = attachmentSelectionRef.current;
  const [imageAttachmentHistory,setImageAttachmentHistory] = useState<{
    editor:TipTapEditor;spaceId:string;docId:string;draftId:string|undefined;versionId:string;
    verify:()=>void;replace:(src:string,versionId:string)=>void;
  }|null>(null);

  const [slashState, setSlashState] = useState<SlashState | null>(null);
  const [blockMenu, setBlockMenu] = useState<BlockMenuState | null>(null);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  // selection-owned link preview: this virtual anchor follows the clicked inline link
  // without moving the editor selection into a separate trigger element.
  const [linkMenu, setLinkMenu] = useState<LinkMenuState | null>(null);
  const [commentDialog, setCommentDialog] = useState<CommentDialogState | null>(null);
  const [commentContent, setCommentContent] = useState("");
  const [commentError, setCommentError] = useState("");
  const [commentSaving, setCommentSaving] = useState(false);
  const [documentComments, setDocumentComments] = useState<DocumentComment[]>([]);
  const [linkDialog, setLinkDialog] = useState<LinkDialogState | null>(null);
  const [bookmarkDialog, setBookmarkDialog] = useState<BookmarkDialogState | null>(null);
  const [tableToolbarPosition, setTableToolbarPosition] = useState<{
    top: number;
    left: number;
  } | null>(null);
  const [selectionToolbarPosition, setSelectionToolbarPosition] =
    useState<SelectionToolbarPosition | null>(null);
  const [selectionTypeMenuOpen, setSelectionTypeMenuOpen] = useState(false);
  const [tableStyleDialogOpen, setTableStyleDialogOpen] = useState(false);
  const [uploadMenu, setUploadMenuState] = useState<UploadMenuState | null>(null);
  const uploadMenuRef = useRef<UploadMenuState | null>(null);
  const setUploadMenu = useCallback((next: UploadMenuState | null) => {
    uploadMenuRef.current = next;
    setUploadMenuState(next);
  }, []);
  const [existingAssets, setExistingAssets] = useState<EditorReusableAsset[]>([]);
  const [existingAssetsLoading, setExistingAssetsLoading] = useState(false);
  const [existingAssetsError, setExistingAssetsError] = useState("");
  const [tagDialog, setTagDialog] = useState<TagDialogState | null>(null);
  const [wikiLinkDialog, setWikiLinkDialog] = useState<WikiLinkDialogState | null>(null);
  const wikiLinkRequestIdRef = useRef(0);
  const [mentionDialog, setMentionDialog] = useState<MentionDialogState | null>(null);
  const [templateInsertionOverlay, setTemplateInsertionOverlay] =
    useState<TemplateInsertionOverlayState | null>(null);
  const [utilityTargetDialog, setUtilityTargetDialog] = useState<UtilityTargetDialogState | null>(
    null,
  );
  const [uploadWarning, setUploadWarning] = useState<string | null>(null);
  const [mobileKeyboardInset, setMobileKeyboardInset] = useState(0);
  const [mobileToolbarActions, setMobileToolbarActions] = useState<MobileToolbarActionId[]>(
    getInitialMobileToolbarActions,
  );
  const [blockAnnouncement, setBlockAnnouncement] = useState({ id: 0, message: "" });
  const [mobileHeadingMenuOpen, setMobileHeadingMenuOpen] = useState(false);
  const [fontFamilyMenuOpen, setFontFamilyMenuOpen] = useState(false);
  const [textColorMenuOpen, setTextColorMenuOpen] = useState(false);
  const [highlightColorMenuOpen, setHighlightColorMenuOpen] = useState(false);
  const [fontSizeMenuOpen, setFontSizeMenuOpen] = useState(false);
  const [alignmentMenuOpen, setAlignmentMenuOpen] = useState(false);
  const [emojiMenuOpen, setEmojiMenuOpen] = useState(false);
  const [fontFamilyQuery, setFontFamilyQuery] = useState("");
  const [mobileCodeLanguageOpen, setMobileCodeLanguageOpen] = useState(false);
  const [activeSection, setActiveSection] = useState<ActiveEditorSection | null>(null);
  const [sectionControlsOpen, setSectionControlsOpen] = useState(false);
  const slashStateRef = useRef<SlashState | null>(null);
  const slashPaletteTriggerRef = useRef<HTMLElement | null>(null);
  const commandMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const imageReplacementRef = useRef<
    ((src: string, alt: string, attachmentId?: string | null) => void) | null
  >(null);
  const fileReplacementRef = useRef<
    ((name: string, size: number, url: string, attachmentId?: string | null) => void) | null
  >(null);
  const mobileHeadingMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const mobileHeadingMenuRef = useRef<HTMLDivElement | null>(null);
  const fontFamilyMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const fontFamilyMenuRef = useRef<HTMLDivElement | null>(null);
  const fontFamilySearchRef = useRef<HTMLInputElement | null>(null);
  const textColorMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const textColorMenuRef = useRef<HTMLDivElement | null>(null);
  const highlightColorMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const highlightColorMenuRef = useRef<HTMLDivElement | null>(null);
  const fontSizeMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const fontSizeMenuRef = useRef<HTMLDivElement | null>(null);
  const alignmentMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const emojiMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const sectionControlsTriggerRef = useRef<HTMLButtonElement | null>(null);
  const sectionControlsMenuRef = useRef<HTMLDivElement | null>(null);
  const linkMenuRef = useRef<HTMLDivElement | null>(null);
  const linkMenuInputRef = useRef<HTMLInputElement | null>(null);
  const linkMenuErrorId = useId();
  const selectionToolbarRef = useRef<HTMLDivElement | null>(null);
  const selectionTypeMenuRef = useRef<HTMLDivElement | null>(null);
  const linkMenuTriggerRef = useRef<HTMLElement | null>(null);
  const blockMenuFocusTriggerRef = useRef<HTMLElement | null>(null);
  const blockMenuRef = useRef<HTMLDivElement | null>(null);
  const blockMenuOpenRef = useRef(false);
  const contextMenuOpeningRef = useRef(false);
  const handleMobileHeadingMenuKeyDown = useMenuKeyboardNavigation(mobileHeadingMenuRef);
  const handleFontFamilyMenuKeyDown = useMenuKeyboardNavigation(fontFamilyMenuRef);
  const handleTextColorMenuKeyDown = useMenuKeyboardNavigation(textColorMenuRef);
  const handleHighlightColorMenuKeyDown = useMenuKeyboardNavigation(highlightColorMenuRef);
  const handleFontSizeMenuKeyDown = useMenuKeyboardNavigation(fontSizeMenuRef);
  const handleSectionControlsMenuKeyDown = useMenuKeyboardNavigation(sectionControlsMenuRef);
  const handleBlockMenuKeyDown = useMenuKeyboardNavigation(blockMenuRef);
  const handleSelectionTypeMenuKeyDown = useMenuKeyboardNavigation(selectionTypeMenuRef);
  const mobileViewportBaselineRef = useRef(0);
  const mobileViewportWidthRef = useRef(0);
  const previousMobileKeyboardOpenRef = useRef(false);
  const editorInstanceRef = useRef<TipTapEditor | null>(null);
  const captureAttachmentHistoryRef = useRef<()=>EditorAttachmentHistoryScope>(()=>{
    throw new Error("Original attachment selection unavailable.");
  });
  const templateInsertionTargetRef = useRef<TemplateInsertionTarget | null>(null);
  const templateSchemaRequestIdRef = useRef(0);
  // Skip onChange during initial content load to prevent auto-save loop
  const isInitializing = useRef(true);
  const pendingEditorInsertions = useRef(0);
  const existingAssetInsertionBusy = useRef(false);
  const externalContentRevision = useRef(0);
  const externalTemplateFieldsRef = useRef(templateFields);
  externalTemplateFieldsRef.current = templateFields;

  const setSlash = useCallback((next: SlashState | null) => {
    slashStateRef.current = next;
    setSlashState(next);
  }, []);

  const syncInlineSlashMenu = useCallback(
    (tipTapEditor: TipTapEditor) => {
      const current = slashStateRef.current;
      if (current?.searchable) return;
      const query = current
        ? deriveInlineSlashQuery(tipTapEditor.state, current.from)
        : findInlineSlashQuery(tipTapEditor.state);
      if (!query) {
        if (current) setSlash(null);
        return;
      }
      const coords = tipTapEditor.view.coordsAtPos(query.to);
      const position = {
        top: Math.min(coords.bottom + 6, window.innerHeight - 340),
        left: Math.max(8, Math.min(coords.left, window.innerWidth - 280)),
      };
      if (
        current?.query === query.query &&
        current.to === query.to &&
        current.position.top === position.top &&
        current.position.left === position.left
      ) {
        return;
      }
      setSlash({
        searchable: false,
        ...query,
        position,
      });
    },
    [setSlash],
  );

  const announceBlockMove = useCallback((direction: "up" | "down") => {
    setBlockAnnouncement((current) => ({
      id: current.id + 1,
      message: `Block moved ${direction}.`,
    }));
  }, []);

  useEffect(() => {
    blockMenuOpenRef.current = Boolean(blockMenu?.open);
  }, [blockMenu?.open]);

  useEffect(() => {
    const syncToolbarActions = (event: Event) => {
      const actions = (event as CustomEvent<unknown>).detail;
      setMobileToolbarActions(normalizeMobileToolbarActions(actions));
    };
    window.addEventListener(MOBILE_TOOLBAR_UPDATED_EVENT, syncToolbarActions);
    return () => window.removeEventListener(MOBILE_TOOLBAR_UPDATED_EVENT, syncToolbarActions);
  }, []);

  const updateActiveSection = (activeEditor: TipTapEditor) => {
    const next = getActiveEditorSection(activeEditor);
    setActiveSection((current) => {
      if (current === next) return current;
      if (!current || !next) return next;
      const sameStyle =
        current.style.borderColor === next.style.borderColor &&
        current.style.backgroundColor === next.style.backgroundColor &&
        current.style.contentBackgroundColor === next.style.contentBackgroundColor &&
        current.style.headerBackgroundColor === next.style.headerBackgroundColor &&
        current.style.headerText === next.style.headerText;
      return current.position === next.position &&
        current.nodeSize === next.nodeSize &&
        current.index === next.index &&
        current.siblingCount === next.siblingCount &&
        current.layout === next.layout &&
        sameStyle
        ? current
        : next;
    });
    if (!next) {
      setSectionControlsOpen(false);
    }
  };

  useEffect(() => {
    const openHistory = (event:Event) => {
      const detail=(event as CustomEvent<{editor:TipTapEditor;versionId:string;verify:()=>void;replace:(src:string,versionId:string)=>void}>).detail;
      const selected=attachmentSelectionRef.current;
      if(!detail||detail.editor!==editorInstanceRef.current||getCurrentLaunchMode()==="local"||!selected.spaceId||!selected.docId||typeof detail.verify!=="function"||typeof detail.replace!=="function")return;
      try {
      const context=captureAttachmentContext(),original={...selected};
      const verifyScope = selected.readOnly
        ? () => { if (attachmentSelectionRef.current !== selected) throw new Error("Original image history selection changed."); }
        : captureAssetInsertionRef.current();
      const verify=()=>{
        verifyScope();
        const current=attachmentSelectionRef.current;
        if(detail.editor!==editorInstanceRef.current||detail.editor.isDestroyed||current.spaceId!==original.spaceId||current.docId!==original.docId||current.draftId!==original.draftId||getCurrentLaunchMode()==="local")throw new Error("Original image draft or editor changed; inspect the retained replacement before use.");
        verifyAttachmentContext(context,attachmentApiBase(),attachmentApi.getToken());detail.verify();
      };
      verify();setImageAttachmentHistory({editor:detail.editor,spaceId:selected.spaceId,docId:selected.docId,draftId:selected.draftId,versionId:detail.versionId,verify,replace:detail.replace});
      } catch(error){setUploadWarning(error instanceof Error?error.message:"Original image history unavailable.");}
    };
    document.addEventListener(OPEN_IMAGE_ATTACHMENT_HISTORY_EVENT,openHistory);
    return()=>document.removeEventListener(OPEN_IMAGE_ATTACHMENT_HISTORY_EVENT,openHistory);
  },[]);
  useEffect(()=>{setImageAttachmentHistory(current=>current&&(current.spaceId!==spaceId||current.docId!==docId||current.draftId!==attachmentDraftId)?null:current);},[spaceId,docId,attachmentDraftId]);

  useEffect(() => {
    const openImageReplace = (event: Event) => {
      const detail = (
        event as CustomEvent<{
          editor?: TipTapEditor;
          replace?: (src: string, alt: string, attachmentId?: string | null) => void;
        }>
      ).detail;
      if (detail?.editor !== editorInstanceRef.current || !detail.replace) return;
      imageReplacementRef.current = detail.replace;
      imageInputRef.current?.click();
    };
    document.addEventListener(OPEN_IMAGE_REPLACE_EVENT, openImageReplace);
    return () => document.removeEventListener(OPEN_IMAGE_REPLACE_EVENT, openImageReplace);
  }, []);

  useEffect(() => {
    const convertImage = (event: Event) => {
      const detail = (
        event as CustomEvent<{
          editor?: TipTapEditor;
          position?: number | null;
          kind?: "gallery" | "carousel";
          src?: string;
          alt?: string;
        }>
      ).detail;
      const activeEditor = editorInstanceRef.current;
      if (
        detail?.editor !== activeEditor ||
        !activeEditor ||
        typeof detail.position !== "number" ||
        (detail.kind !== "gallery" && detail.kind !== "carousel")
      ) {
        return;
      }
      const imageNode = activeEditor.state.doc.nodeAt(detail.position);
      if (!imageNode || imageNode.type.name !== "image") return;
      const item = {
        src: String(detail.src || imageNode.attrs.src || ""),
        alt: String(detail.alt || imageNode.attrs.alt || "Image 1"),
      };
      const type = detail.kind;
      activeEditor
        .chain()
        .focus()
        .deleteRange({ from: detail.position, to: detail.position + imageNode.nodeSize })
        .insertContentAt(detail.position, [
          {
            type,
            attrs:
              type === "gallery"
                ? { title: DEFAULT_GALLERY_TITLE, layout: "responsive-columns", items: [item] }
                : { title: DEFAULT_CAROUSEL_TITLE, items: [item] },
          },
          { type: "paragraph" },
        ])
        .run();
    };
    document.addEventListener(OPEN_IMAGE_CONVERT_EVENT, convertImage);
    return () => document.removeEventListener(OPEN_IMAGE_CONVERT_EVENT, convertImage);
  }, []);

  useEffect(() => {
    const openBookmarkEdit = (event: Event) => {
      if (readOnly) return;
      const detail = (
        event as CustomEvent<{
          editor?: TipTapEditor;
          position?: number;
          href?: string;
          title?: string;
          description?: string;
          imageUrl?: string;
        }>
      ).detail;
      if (detail?.editor !== editorInstanceRef.current || typeof detail.position !== "number")
        return;
      setBookmarkDialog({
        from: detail.position,
        to: detail.position + 1,
        href: detail.href || "",
        title: detail.title || "",
        description: detail.description || "",
        imageUrl: detail.imageUrl || "",
      });
    };
    document.addEventListener(OPEN_BOOKMARK_EDIT_EVENT, openBookmarkEdit);
    return () => document.removeEventListener(OPEN_BOOKMARK_EDIT_EVENT, openBookmarkEdit);
  }, [readOnly]);

  useEffect(() => {
    const imageInput = imageInputRef.current;
    if (!imageInput) return;
    const clearImageReplacement = () => {
      imageReplacementRef.current = null;
    };
    imageInput.addEventListener("cancel", clearImageReplacement);
    return () => imageInput.removeEventListener("cancel", clearImageReplacement);
  }, []);

  const closeMobileHeadingMenu = useCallback(() => setMobileHeadingMenuOpen(false), []);
  const closeAndRestoreMobileHeadingMenu = useOverlayFocusLifecycle({
    open: mobileHeadingMenuOpen,
    onClose: closeMobileHeadingMenu,
    containerRef: mobileHeadingMenuRef,
    triggerRef: mobileHeadingMenuTriggerRef,
  });
  const dismissMobileHeadingMenu = useCallback(
    (restoreFocus = true) => {
      if (restoreFocus) {
        closeAndRestoreMobileHeadingMenu();
        return;
      }
      closeMobileHeadingMenu();
    },
    [closeAndRestoreMobileHeadingMenu, closeMobileHeadingMenu],
  );
  const closeFontFamilyMenu = useCallback(() => setFontFamilyMenuOpen(false), []);
  const closeAndRestoreFontFamilyMenu = useOverlayFocusLifecycle({
    open: fontFamilyMenuOpen,
    onClose: closeFontFamilyMenu,
    containerRef: fontFamilyMenuRef,
    triggerRef: fontFamilyMenuTriggerRef,
  });
  const dismissFontFamilyMenu = useCallback(
    (restoreFocus = true) => {
      if (restoreFocus) {
        closeAndRestoreFontFamilyMenu();
        return;
      }
      closeFontFamilyMenu();
    },
    [closeAndRestoreFontFamilyMenu, closeFontFamilyMenu],
  );
  const closeTextColorMenu = useCallback(() => setTextColorMenuOpen(false), []);
  const closeAndRestoreTextColorMenu = useOverlayFocusLifecycle({
    open: textColorMenuOpen,
    onClose: closeTextColorMenu,
    containerRef: textColorMenuRef,
    triggerRef: textColorMenuTriggerRef,
  });
  const dismissTextColorMenu = useCallback(
    (restoreFocus = true) => {
      if (restoreFocus) {
        closeAndRestoreTextColorMenu();
        return;
      }
      closeTextColorMenu();
    },
    [closeAndRestoreTextColorMenu, closeTextColorMenu],
  );
  const closeHighlightColorMenu = useCallback(() => setHighlightColorMenuOpen(false), []);
  const closeAndRestoreHighlightColorMenu = useOverlayFocusLifecycle({
    open: highlightColorMenuOpen,
    onClose: closeHighlightColorMenu,
    containerRef: highlightColorMenuRef,
    triggerRef: highlightColorMenuTriggerRef,
  });
  const dismissHighlightColorMenu = useCallback(
    (restoreFocus = true) => {
      if (restoreFocus) {
        closeAndRestoreHighlightColorMenu();
        return;
      }
      closeHighlightColorMenu();
    },
    [closeAndRestoreHighlightColorMenu, closeHighlightColorMenu],
  );
  const closeFontSizeMenu = useCallback(() => setFontSizeMenuOpen(false), []);
  const closeAndRestoreFontSizeMenu = useOverlayFocusLifecycle({
    open: fontSizeMenuOpen,
    onClose: closeFontSizeMenu,
    containerRef: fontSizeMenuRef,
    triggerRef: fontSizeMenuTriggerRef,
  });
  const dismissFontSizeMenu = useCallback(
    (restoreFocus = true) => (restoreFocus ? closeAndRestoreFontSizeMenu() : closeFontSizeMenu()),
    [closeAndRestoreFontSizeMenu, closeFontSizeMenu],
  );
  const closeSectionControlsMenu = useCallback(() => setSectionControlsOpen(false), []);
  const closeAndRestoreSectionControlsMenu = useOverlayFocusLifecycle({
    open: sectionControlsOpen,
    onClose: closeSectionControlsMenu,
    containerRef: sectionControlsMenuRef,
    triggerRef: sectionControlsTriggerRef,
  });
  const dismissSectionControlsMenu = useCallback(
    (restoreFocus = true) => {
      if (restoreFocus) {
        closeAndRestoreSectionControlsMenu();
        return;
      }
      closeSectionControlsMenu();
    },
    [closeAndRestoreSectionControlsMenu, closeSectionControlsMenu],
  );
  useEffect(() => {
    if (!fontFamilyMenuOpen) return;
    setFontFamilyQuery("");
    const frame = window.requestAnimationFrame(() => fontFamilySearchRef.current?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, [fontFamilyMenuOpen]);
  const closeLinkMenu = useCallback(() => {
    setLinkMenu(null);
  }, []);
  const closeAndRestoreLinkMenu = useOverlayFocusLifecycle({
    open: Boolean(linkMenu),
    onClose: closeLinkMenu,
    containerRef: linkMenuRef,
    triggerRef: linkMenuTriggerRef,
  });
  const closeBlockMenu = useCallback(
    () =>
      setBlockMenu((current) =>
        current ? { ...current, open: false, transformOpen: false } : current,
      ),
    [],
  );
  const updateBlockMenuForBlock = useCallback(
    (
      tipTapEditor: TipTapEditor,
      block: TopLevelBlock | null,
      open?: boolean,
      menuPositionOverride?: { top: number; left: number },
    ) => {
      if (!block || block.node.type.name === "section" || block.node.type.name === "columns") {
        setBlockMenu(null);
        return;
      }

      const computedMenuPosition = getBlockMenuPosition(tipTapEditor, block);
      if (!computedMenuPosition) {
        setBlockMenu(null);
        return;
      }
      const menuPosition = menuPositionOverride ?? computedMenuPosition;

      setBlockMenu((current) => {
        const sameTarget = current?.target.from === block.from;
        // Opening the overflow menu focuses the editor so the command can
        // operate on the block. That selection update re-runs this helper;
        // keep the menu anchored to the trigger instead of recomputing from
        // the caret and jumping it toward the viewport center.
        const preserveOpenPosition =
          sameTarget && current?.open && open === undefined && !menuPositionOverride;
        const next = {
          target: {
            from: block.from,
            to: block.to,
            index: block.index,
            parentStart: block.parentStart,
            parentChildCount: block.parentChildCount,
          },
          open: open ?? (sameTarget ? Boolean(current?.open) : false),
          menuPosition: preserveOpenPosition ? current.menuPosition : menuPosition,
          canMoveUp: block.index > 0,
          canMoveDown:
            block.index < (block.parentChildCount ?? tipTapEditor.state.doc.childCount) - 1,
          transformOpen: sameTarget ? Boolean(current?.transformOpen) : false,
        };
        if (
          current &&
          current.target.from === next.target.from &&
          current.target.to === next.target.to &&
          current.target.index === next.target.index &&
          current.target.parentStart === next.target.parentStart &&
          current.target.parentChildCount === next.target.parentChildCount &&
          current.open === next.open &&
          current.menuPosition.top === next.menuPosition.top &&
          current.menuPosition.left === next.menuPosition.left &&
          current.canMoveUp === next.canMoveUp &&
          current.canMoveDown === next.canMoveDown &&
          current.transformOpen === next.transformOpen
        ) {
          return current;
        }
        return next;
      });
    },
    [],
  );

  const updateBlockMenu = useCallback(
    (tipTapEditor: TipTapEditor, open?: boolean) => {
      if (
        !showBlockChrome ||
        readOnly ||
        !tipTapEditor.isFocused ||
        tipTapEditor.isActive("table")
      ) {
        setBlockMenu(null);
        return;
      }

      updateBlockMenuForBlock(tipTapEditor, getCurrentTopLevelBlock(tipTapEditor), open);
    },
    [readOnly, showBlockChrome, updateBlockMenuForBlock],
  );

  const focusBlockMenuTarget = (tipTapEditor: TipTapEditor) => {
    const target = blockMenu?.target;
    if (!target) return false;

    const position = Math.max(1, Math.min(target.from + 1, tipTapEditor.state.doc.content.size));
    tipTapEditor.chain().focus().setTextSelection(position).run();
    return true;
  };

  const openInsertBlockPaletteForTarget = (
    tipTapEditor: TipTapEditor,
    target: Pick<
      TopLevelBlock,
      "from" | "to" | "index" | "parentStart" | "parentChildCount"
    > | null,
    placement: "above" | "below",
  ) => {
    if (!target || readOnly) return;
    const insertion =
      target.parentStart != null && target.parentStart > 0
        ? (() => {
            const paragraph = tipTapEditor.schema.nodes.paragraph?.create();
            if (!paragraph) return null;
            const insertAt = placement === "above" ? target.from : target.to;
            const transaction = tipTapEditor.state.tr.insert(insertAt, paragraph);
            const selectionPosition = Math.min(insertAt + 1, transaction.doc.content.size);
            transaction.setSelection(TextSelection.create(transaction.doc, selectionPosition));
            transaction.setStoredMarks(null);
            return { transaction, selectionPosition };
          })()
        : createSiblingParagraphTransaction(tipTapEditor.state, target.index, placement);
    if (!insertion) return;

    // Re-resolve from the target's current document index instead of using the
    // toolbar's stale absolute positions. The palette is pinned to this clean
    // sibling paragraph so a selected command cannot transform a neighbour.
    tipTapEditor.view.dispatch(insertion.transaction.scrollIntoView());
    tipTapEditor.view.focus();
    openSearchableCommandPalette(tipTapEditor, {
      from: insertion.selectionPosition,
      to: insertion.selectionPosition,
    });
    setBlockMenu(null);
  };

  const openInsertBlockPalette = (tipTapEditor: TipTapEditor, placement: "above" | "below") => {
    openInsertBlockPaletteForTarget(tipTapEditor, blockMenu?.target ?? null, placement);
  };

  const openGutterBlockMenu = (target: EditorGutterBlockTarget, trigger: HTMLElement) => {
    const triggerRect = trigger.getBoundingClientRect();
    const selectionPosition = Math.min(target.from + 1, editor.state.doc.content.size);
    editor.view.dispatch(
      editor.state.tr.setSelection(TextSelection.near(editor.state.doc.resolve(selectionPosition))),
    );
    blockMenuFocusTriggerRef.current = editor.view.dom;
    updateBlockMenuForBlock(editor, target, true, {
      top:
        triggerRect.bottom + 8 < window.innerHeight - 280
          ? triggerRect.bottom + 8
          : Math.max(8, triggerRect.top - 280),
      left: Math.max(
        8,
        Math.min(triggerRect.left + triggerRect.width / 2 - 88, window.innerWidth - 184),
      ),
    });
  };

  const insertParagraphAfterActiveBlock = (tipTapEditor: TipTapEditor) => {
    const { $from } = tipTapEditor.state.selection;
    let depth = $from.depth;
    while (depth > 0 && $from.node(depth).type.name !== "codeBlock") depth -= 1;
    if (depth <= 0) return false;
    const paragraph = tipTapEditor.schema.nodes.paragraph?.create();
    if (!paragraph) return false;
    const insertAt = $from.after(depth);
    // Insertion can be triggered by both the command palette and the toolbar.
    // Treat the operation as idempotent so those paths never stack blank
    // paragraphs or accidentally leave the code block as the final node.
    if (tipTapEditor.state.doc.resolve(insertAt).nodeAfter?.type.name === "paragraph") {
      return false;
    }
    const transaction = tipTapEditor.state.tr.insert(insertAt, paragraph);
    transaction.setSelection(TextSelection.create(transaction.doc, insertAt + 1));
    tipTapEditor.view.dispatch(transaction.scrollIntoView());
    tipTapEditor.view.focus();
    return true;
  };

  const openSearchableCommandPalette = useCallback(
    (
      tipTapEditor: TipTapEditor,
      selectionOverride?: { from: number; to: number },
      toolbarTrigger?: HTMLElement | null,
    ) => {
      if (readOnly) {
        return false;
      }

      const selection = selectionOverride ?? tipTapEditor.state.selection;
      const { from, to } = selection;
      const empty = from === to;
      slashPaletteTriggerRef.current = toolbarTrigger ?? tipTapEditor.view.dom;
      const coords = tipTapEditor.view.coordsAtPos(empty ? from : to);
      const mobilePalettePosition = showMobileCommandButton
        ? getMobileOverlayPosition(384, 340)
        : null;
      const toolbarPalettePosition = toolbarTrigger
        ? getToolbarPopoverPosition(toolbarTrigger.getBoundingClientRect(), window, {
            width: 384,
            height: 420,
          })
        : null;
      setSlash({
        query: "",
        from,
        to,
        position: mobilePalettePosition ??
          toolbarPalettePosition ?? {
            top: Math.min(coords.bottom + 6, window.innerHeight - 340),
            left: Math.max(8, Math.min(coords.left, window.innerWidth - 280)),
          },
        searchable: true,
      });
      setLinkMenu(null);
      setContextMenu(null);
      setUploadMenu(null);
      return true;
    },
    [readOnly, setSlash, setUploadMenu, showMobileCommandButton],
  );

  const openUploadMenu = useCallback(
    (preferImage: boolean, slashRange?: { from: number; to: number }) => {
      if (readOnly) return;
      setUploadMenu({ preferImage, ...slashRange });
      setSlash(null);
      setLinkMenu(null);
      setLinkDialog(null);
      setTagDialog(null);
      setContextMenu(null);
    },
    [readOnly, setSlash, setUploadMenu],
  );

  useEffect(() => {
    const openFileReplace = (event: Event) => {
      const detail = (
        event as CustomEvent<{
          editor?: TipTapEditor;
          replace?: (name: string, size: number, url: string, attachmentId?: string | null) => void;
        }>
      ).detail;
      if (detail?.editor !== editorInstanceRef.current || !detail.replace || readOnly) return;
      fileReplacementRef.current = detail.replace;
      openUploadMenu(false);
    };
    document.addEventListener(OPEN_FILE_REPLACE_EVENT, openFileReplace);
    return () => document.removeEventListener(OPEN_FILE_REPLACE_EVENT, openFileReplace);
  }, [openUploadMenu, readOnly]);

  const attachmentImageUrls = useRef(new Set<string>());
  const coverAttachmentRef=useRef(coverAttachmentReference);coverAttachmentRef.current=coverAttachmentReference;
  const activeAttachmentPage = useRef(assetContext);
  activeAttachmentPage.current = assetContext;
  const ownAttachmentImageUrl = useCallback((url:string) => {if(url.startsWith("blob:"))attachmentImageUrls.current.add(url);return url;}, []);
  useEffect(() => {const owned=attachmentImageUrls.current;return () => {owned.forEach(releaseBrowserAttachmentImage);owned.clear();};}, [assetContext.space_id,assetContext.document_id]);

  const refreshExistingAssets = useCallback(async () => {
    const selection = renderedAttachmentSelection;
    const current = () => attachmentSelectionRef.current === selection;
    if (!current()) return;
    if (!spaceId) {
      setExistingAssets([]);
      return;
    }
    setExistingAssetsLoading(true);
    setExistingAssetsError("");
    try {
      const assets = await dataClient.listReusableAssets(spaceId);
      if (!current()) return;
      const previewAssets = await Promise.all(
        assets.map(async (asset) => {
          if (!("original_name" in asset) || !isEditorReusableImage(asset) || !asset.document_id) {
            return asset;
          }
          try {
            const url = await dataClient.previewPinnedDocumentAttachmentVersionImage(spaceId,asset.document_id,asset.id);
            if (!current()) { releaseBrowserAttachmentImage(url); return asset; }
            return { ...asset, url:ownAttachmentImageUrl(url) };
          } catch {
            return asset;
          }
        }),
      );
      if (current()) setExistingAssets(previewAssets);
    } catch (error) {
      if (!current()) return;
      setExistingAssetsError(
        error instanceof Error ? error.message : "Could not load existing assets.",
      );
    } finally {
      if (current()) setExistingAssetsLoading(false);
    }
  }, [spaceId, ownAttachmentImageUrl, renderedAttachmentSelection]);

  useEffect(() => {
    if (!uploadMenu) return;
    void refreshExistingAssets();
  }, [refreshExistingAssets, uploadMenu]);

  const updateSelectionMenu = (tipTapEditor: TipTapEditor) => {
    const ownedUploadSelection = isUploadMenuOwnedSelection(
      uploadMenuRef.current?.from != null && uploadMenuRef.current.to != null
        ? { from: uploadMenuRef.current.from, to: uploadMenuRef.current.to }
        : null,
      tipTapEditor.state.selection,
    );
    if (ownedUploadSelection) {
      setSelectionToolbarPosition(null);
      setSelectionTypeMenuOpen(false);
      return;
    }
    const showSelectionToolbar = shouldShowSelectionToolbar({
      mobile: showMobileCommandButton,
      readOnly,
      persistentToolbar: Boolean(desktopToolbarHost),
      selection: tipTapEditor.state.selection,
      inTable: tipTapEditor.isActive("table"),
    });

    if (!showSelectionToolbar) {
      setSelectionToolbarPosition(null);
      setSelectionTypeMenuOpen(false);
    }

    if (showMobileCommandButton) {
      return;
    }

    if (tipTapEditor.state.selection.empty) {
      // Focus returns before a pointer click places the new caret. Never open a
      // link editor from that stale selection; the anchor click and explicit
      // Link command own opening it. Moving the caret dismisses the old menu.
      setLinkMenu(null);
      return;
    }

    if (readOnly) {
      return;
    }

    setLinkMenu(null);
    setUploadMenu(null);
    setTagDialog(null);
    if (showSelectionToolbar) {
      setSelectionToolbarPosition(getSelectionFloatingPosition(tipTapEditor, 372, 40));
    }
  };

  const openTagDialog = (tipTapEditor: TipTapEditor) => {
    if (readOnly) return;
    const { from, to, empty } = tipTapEditor.state.selection;
    setSlash(null);
    setLinkMenu(null);
    setLinkDialog(null);
    setUploadMenu(null);
    setTagDialog({
      from,
      to,
      text: empty ? "" : normalizeTagName(getSelectedText(tipTapEditor)),
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

  const openLinkDialog = (tipTapEditor: TipTapEditor, override?: Partial<LinkMenuState>) => {
    const activeLink = getLinkRange(tipTapEditor, tipTapEditor.state.selection.from);
    const from = override?.from ?? activeLink?.from ?? tipTapEditor.state.selection.from;
    const to = override?.to ?? activeLink?.to ?? tipTapEditor.state.selection.to;
    const text =
      override?.text ??
      activeLink?.text ??
      (tipTapEditor.state.selection.empty ? "" : getSelectedText(tipTapEditor));
    const href = override?.href ?? activeLink?.href ?? "";
    setLinkMenu(null);
    setContextMenu(null);
    setUploadMenu(null);
    setTagDialog(null);
    setSelectionToolbarPosition(null);
    setSelectionTypeMenuOpen(false);
    setLinkDialog({
      from,
      to,
      text,
      href,
    });
  };

  const openSelectionLinkEditor = (tipTapEditor: TipTapEditor) => {
    if (readOnly || tipTapEditor.state.selection.empty) {
      openLinkDialog(tipTapEditor);
      return;
    }
    const activeLink = getLinkRange(tipTapEditor, tipTapEditor.state.selection.from);
    const from = activeLink?.from ?? tipTapEditor.state.selection.from;
    const to = activeLink?.to ?? tipTapEditor.state.selection.to;
    const href = activeLink?.href ?? "";
    linkMenuTriggerRef.current = tipTapEditor.view.dom;
    setSlash(null);
    setContextMenu(null);
    setUploadMenu(null);
    setTagDialog(null);
    setSelectionToolbarPosition(null);
    setSelectionTypeMenuOpen(false);
    setLinkMenu({
      from,
      to,
      text: activeLink?.text ?? getSelectedText(tipTapEditor),
      href,
      draftHref: href,
      position: getSelectionFloatingPosition(tipTapEditor, 384, 40),
    });
  };

  const updateTableToolbar = useCallback(
    (tipTapEditor: TipTapEditor) => {
      if (readOnly) {
        setTableToolbarPosition(null);
        return;
      }
      const next = getActiveTableToolbarPosition(tipTapEditor);
      setTableToolbarPosition((current) =>
        current?.top === next?.top && current?.left === next?.left ? current : next,
      );
    },
    [readOnly],
  );

  const openBookmarkDialog = (tipTapEditor: TipTapEditor) => {
    if (readOnly) return;
    const { from, to, empty } = tipTapEditor.state.selection;
    const selectedText = empty ? "" : getSelectedText(tipTapEditor).trim();
    setSlash(null);
    setLinkMenu(null);
    setLinkDialog(null);
    setUploadMenu(null);
    setTagDialog(null);
    setBookmarkDialog({
      from,
      to,
      href: "",
      title: selectedText,
    });
  };

  const applyBookmarkDialog = async () => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !activeEditor.isEditable || !bookmarkDialog) return;
    const href = normalizeEditorLinkHref(bookmarkDialog.href);
    if (!href) return;
    pendingEditorInsertions.current += 1;
    try {
    const metadata = await dataClient.fetchBookmarkMetadata(href).catch(() => null);
    if (activeEditor.isDestroyed || editorInstanceRef.current !== activeEditor || !activeEditor.isEditable) return;
    isInitializing.current = false;
    const resolvedHref = metadata?.url || href;
    const title = bookmarkDialog.title.trim() || metadata?.title || getBookmarkDomain(resolvedHref);
    const description = metadata?.description || bookmarkDialog.description || "";
    const imageUrl = metadata?.image_url || bookmarkDialog.imageUrl || "";
    const from = Math.max(0, Math.min(bookmarkDialog.from, activeEditor.state.doc.content.size));
    const to = Math.max(0, Math.min(bookmarkDialog.to, activeEditor.state.doc.content.size));

    activeEditor
      .chain()
      .focus()
      .deleteRange({ from: Math.min(from, to), to: Math.max(from, to) })
      .insertContent({
        type: "bookmarkCard",
        attrs: {
          href: resolvedHref,
          title,
          description,
          imageUrl,
        },
      })
      .run();
    setBookmarkDialog(null);
    } finally { pendingEditorInsertions.current -= 1; }
  };

  const applyLinkDialog = () => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !linkDialog) return;
    const transaction = createEditorLinkTransaction(
      activeEditor.state,
      linkDialog,
      linkDialog.href,
    );
    if (!transaction) return;
    activeEditor.view.dispatch(transaction);
    activeEditor.commands.focus();

    setLinkDialog(null);
  };

  const applyLinkMenu = () => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !linkMenu) return;
    const transaction = createEditorLinkTransaction(
      activeEditor.state,
      linkMenu,
      linkMenu.draftHref,
    );
    if (!transaction) return;
    activeEditor.view.dispatch(transaction);
    closeAndRestoreLinkMenu();
  };

  const loadWikiLinkDocuments = (range?: { from: number; to: number }) => {
    if (!spaceId) return;
    const requestId = ++wikiLinkRequestIdRef.current;
    setWikiLinkDialog((current) =>
      range
        ? {
            from: range.from,
            to: range.to,
            documents: [],
            query: "",
            loading: true,
            error: false,
          }
        : current
          ? { ...current, documents: [], loading: true, error: false }
          : current,
    );
    void Promise.all([
      dataClient.listDocuments(spaceId, { include_nested: true }),
      dataClient.listFolders(spaceId).catch(() => []),
    ])
      .then(([documents, folders]) => {
        if (requestId !== wikiLinkRequestIdRef.current) return;
        const folderNames = new Map(folders.map((folder) => [folder.id, folder.name]));
        setWikiLinkDialog((current) =>
          current
            ? {
                ...current,
                documents: documents.map((document) => ({
                  id: document.id,
                  title: document.title,
                  icon: document.icon,
                  folderName: document.folder_id ? folderNames.get(document.folder_id) : undefined,
                  updatedAt: document.updated_at,
                })),
                loading: false,
              }
            : current,
        );
      })
      .catch(() => {
        if (requestId !== wikiLinkRequestIdRef.current) return;
        setWikiLinkDialog((current) =>
          current ? { ...current, loading: false, error: true } : current,
        );
      });
  };

  const openWikiLinkDialog = (range: { from: number; to: number }) => {
    loadWikiLinkDocuments(range);
  };

  const insertWikiLink = (document: Pick<DocumentRecord, "id" | "title">) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !wikiLinkDialog) return;
    const safeFrom = Math.max(
      0,
      Math.min(wikiLinkDialog.from, activeEditor.state.doc.content.size),
    );
    const safeTo = Math.max(0, Math.min(wikiLinkDialog.to, activeEditor.state.doc.content.size));
    activeEditor
      .chain()
      .focus()
      .deleteRange({ from: Math.min(safeFrom, safeTo), to: Math.max(safeFrom, safeTo) })
      .insertContent(serializeWikiLinkMarkdown(document.title, document.id))
      .run();
    setWikiLinkDialog(null);
  };
  const openMentionDialog = (range: { from: number; to: number }) => {
    if (!spaceId) return;
    setMentionDialog({
      from: range.from,
      to: range.to,
      members: [],
      query: "",
      loading: true,
      error: false,
    });
    void dataClient
      .listSpaceMembers(spaceId)
      .then((members) =>
        setMentionDialog((current) =>
          current ? { ...current, members, loading: false } : current,
        ),
      )
      .catch(() =>
        setMentionDialog((current) =>
          current ? { ...current, loading: false, error: true } : current,
        ),
      );
  };
  const insertMention = (member: Pick<SpaceMemberRecord, "user_id" | "display_name">) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !mentionDialog) return;
    activeEditor
      .chain()
      .focus()
      .deleteRange({
        from: Math.min(mentionDialog.from, mentionDialog.to),
        to: Math.max(mentionDialog.from, mentionDialog.to),
      })
      .insertContent(contentMentionToHtml(member.display_name, member.user_id))
      .run();
    setMentionDialog(null);
  };

  const openUtilityTargetDialog = (
    kind: "space" | "author",
    range: { from: number; to: number },
  ) => {
    setUtilityTargetDialog({
      kind,
      from: range.from,
      to: range.to,
      targets: [],
      query: "",
      loading: true,
      error: false,
    });
    const loadTargets =
      kind === "space"
        ? dataClient
            .listSpaces()
            .then((spaces) =>
              spaces.map((space) => ({ id: space.id, label: space.name, detail: space.slug })),
            )
        : spaceId
          ? dataClient.listSpaceMembers(spaceId).then((members) =>
              members.map((member) => ({
                id: member.user_id,
                label: member.display_name,
                detail: member.email,
              })),
            )
          : Promise.resolve([] as EditorUtilityTarget[]);
    void loadTargets
      .then((targets) =>
        setUtilityTargetDialog((current) =>
          current ? { ...current, targets, loading: false } : current,
        ),
      )
      .catch(() =>
        setUtilityTargetDialog((current) =>
          current ? { ...current, loading: false, error: true } : current,
        ),
      );
  };

  const insertUtilityTarget = (target: EditorUtilityTarget) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || !utilityTargetDialog) return;
    const from = Math.min(utilityTargetDialog.from, utilityTargetDialog.to);
    const to = Math.max(utilityTargetDialog.from, utilityTargetDialog.to);
    activeEditor.chain().focus().deleteRange({ from, to }).insertContent(target.label).run();
    setUtilityTargetDialog(null);
  };

  const removeLink = (state: LinkMenuState) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor) return;
    const transaction = createEditorUnlinkTransaction(activeEditor.state, state);
    if (!transaction) return;
    activeEditor.view.dispatch(transaction);
    closeAndRestoreLinkMenu();
  };

  const handlePickedFiles = async (
    fileList: ArrayLike<File> | null,
    onProgress?: (progress: EditorUploadProgress) => void,
  ): Promise<EditorUploadResult[]> => {
    const activeEditor = editorInstanceRef.current;
    const replaceFileTarget = fileReplacementRef.current;
    const replaceImageTarget = imageReplacementRef.current;
    const requestedFiles = Array.from(fileList ?? []).filter(isEditorFile);
    const files = replaceFileTarget || replaceImageTarget ? requestedFiles.slice(0, 1) : requestedFiles;
    if (!activeEditor || readOnly || files.length === 0) {
      return files.map((file, index) => {
        const result: EditorUploadResult = {
          index,
          status: "failed",
          error: `Could not insert ${file.name} because the editor is unavailable.`,
        };
        onProgress?.(result);
        return result;
      });
    }
    const verifyScope = captureAssetInsertion();
    const verifyOriginalEditor = () => {
      verifyScope();
      if (fileReplacementRef.current !== replaceFileTarget || imageReplacementRef.current !== replaceImageTarget)
        throw new Error("Original replacement target changed; inspect the retained upload before insertion.");
    };
    const slashRange = uploadMenuRef.current?.from != null && uploadMenuRef.current.to != null
      ? {from:uploadMenuRef.current.from,to:uploadMenuRef.current.to} : undefined;
    const insertionRange = replaceFileTarget || replaceImageTarget ? null
      : captureAssetInsertionRange(activeEditor,slashRange);
    pendingEditorInsertions.current += 1;
    try {
    let insertedAny = false;
    const results: EditorUploadResult[] = [];

    for (const [index, file] of files.entries()) {
      onProgress?.({ index, status: "uploading" });
      let acknowledgedOperationId: string | null = null;
      try {
        verifyOriginalEditor();
        if (file.size > maxPickedFileBytes) {
          const message = formatEditorUploadLimitWarning(file.name, maxPickedFileMb);
          setUploadWarning(message);
          const result: EditorUploadResult = { index, status: "failed", error: message };
          results.push(result);
          onProgress?.(result);
          continue;
        }
        setUploadWarning(null);
        const isImage = isEditorImageFile(file);
        const serverDocumentAsset =
          getCurrentLaunchMode() !== "local" && assetContext.space_id && assetContext.document_id;
        const uploadContext = serverDocumentAsset ? captureAttachmentContext() : null;
        const dataUrl = serverDocumentAsset ? "" : await readFileAsDataUrl(file);
        verifyOriginalEditor();
        const savedAsset = serverDocumentAsset
          ? await dataClient.saveDocumentFileAsset({
              name: file.name,
              bytes: file,
              space_id: assetContext.space_id!,
              document_id: assetContext.document_id!,
              draft_id: attachmentDraftId,
            })
          : await dataClient.saveBrowserAsset({
              name: file.name,
              mime_type: file.type || undefined,
              size: file.size,
              url: dataUrl,
              ...assetContext,
            });
        const isServerAttachment = "filename" in savedAsset;
        acknowledgedOperationId = isServerAttachment ? directUploadAcknowledgementId(savedAsset) : null;
        verifyOriginalEditor();
        const assetName = isServerAttachment
          ? savedAsset.original_name || savedAsset.filename
          : savedAsset.name;
        const assetSize = isServerAttachment ? savedAsset.size_bytes : savedAsset.size;
        let assetUrl = isServerAttachment ? dataUrl : savedAsset.url;
        if (serverDocumentAsset && !isServerAttachment) {
          setUploadWarning(
            `${file.name} was saved on this device. Retry the upload when the server returns.`,
          );
        }
        if (isServerAttachment && assetContext.space_id && assetContext.document_id) {
          if (uploadContext) verifyAttachmentContext(uploadContext,attachmentApiBase(),attachmentApi.getToken());
          verifyOriginalEditor();
          assetUrl = isImage ? await dataClient.previewPinnedDocumentAttachmentVersionImage(assetContext.space_id,assetContext.document_id,savedAsset.id) : "";
          try {
            if (uploadContext) verifyAttachmentContext(uploadContext,attachmentApiBase(),attachmentApi.getToken());
            verifyOriginalEditor();
          } catch (error) { releaseBrowserAttachmentImage(assetUrl); throw error; }
          ownAttachmentImageUrl(assetUrl);
        }
        verifyOriginalEditor();
        const insertionPosition = insertionRange?.current();
        const replaceFile = replaceFileTarget;
        if (replaceFile) {
          replaceFile(assetName, assetSize, assetUrl, isServerAttachment ? savedAsset.id : null);
          if (fileReplacementRef.current === replaceFileTarget) fileReplacementRef.current = null;
          insertedAny = true;
        } else if (isImage) {
          const attachmentId = isServerAttachment ? savedAsset.id : null;
          const replaceImage = replaceImageTarget;
          if (replaceImage) {
            replaceImage(assetUrl, assetName, attachmentId);
            if (imageReplacementRef.current === replaceImageTarget) imageReplacementRef.current = null;
            insertedAny = true;
          } else {
            const insertAt = insertionPosition ?? activeEditor.state.selection.to;
            const inserted = activeEditor
              .chain()
              .focus()
              .insertContentAt(insertAt, [
                {
                  type: "image",
                  attrs: { src: assetUrl, alt: assetName, attachmentId },
                },
                { type: "paragraph" },
              ])
              .run();
            if (!inserted) {
              throw new Error(`Could not insert ${file.name} at the current editor selection.`);
            }
            insertedAny = true;
          }
        } else {
          const inserted = insertAttachmentCard(
            activeEditor,
            assetName,
            assetSize,
            assetUrl,
            isServerAttachment ? savedAsset.id : null,
            insertionPosition,
          );
          if (!inserted) {
            throw new Error(`Could not insert ${file.name} at the current editor selection.`);
          }
          insertedAny = true;
        }
        insertionRange?.advance();
        if(isServerAttachment && assetContext.space_id && assetContext.document_id){const operationId=directUploadAcknowledgementId(savedAsset);if(operationId)await markDirectUploadAdopted({space_id:assetContext.space_id,document_id:assetContext.document_id,draft_id:attachmentDraftId},operationId);}
        verifyScope();
        const result: EditorUploadResult = { index, status: "uploaded" };
        results.push(result);
        onProgress?.(result);
      } catch (error) {
        const retainedId = acknowledgedOperationId ?? retainedUploadOperationId(error);
        const detail = error instanceof Error ? error.message : `Could not insert ${file.name}.`;
        const message = retainedId ? `${detail} Original upload retained on this device (${retainedId}); inspect the original attachment before retrying.` : detail;
        setUploadWarning(message);
        const result: EditorUploadResult = { index, status: "failed", error: message };
        results.push(result);
        onProgress?.(result);
      }
    }
    if (insertedAny && !activeEditor.isDestroyed) {
      try { verifyScope(); } catch { return results; }
      onChange(htmlToMarkdown(activeEditor.getHTML()));
    }
    return results;
    } finally { insertionRange?.close(); pendingEditorInsertions.current -= 1; }
  };

  const handleExistingAsset = async (asset: EditorReusableAsset, retainedAcknowledgement = false) => {
    if (!editorInstanceRef.current || readOnly) return;
    if(existingAssetInsertionBusy.current)throw new Error("An original asset insertion is already in progress.");
    const originalEditor = editorInstanceRef.current;
    const verifyScope = captureAssetInsertion();
    const replaceFileTarget = fileReplacementRef.current;
    const replaceImageTarget = imageReplacementRef.current;
    const slashRange = replaceFileTarget || replaceImageTarget ? undefined
      : uploadMenuRef.current?.from != null && uploadMenuRef.current.to != null
        ? {from:uploadMenuRef.current.from,to:uploadMenuRef.current.to} : undefined;
    const verifyOriginalAsset = () => {
      verifyScope();
      if (fileReplacementRef.current !== replaceFileTarget || imageReplacementRef.current !== replaceImageTarget)
        throw new Error("Original replacement target changed; no other block was changed.");
    };
    verifyOriginalAsset();
    const ownedUploadMenu = uploadMenuRef.current;
    const insertionRange = replaceFileTarget || replaceImageTarget ? null
      : captureAssetInsertionRange(originalEditor,slashRange);

    existingAssetInsertionBusy.current = true;
    pendingEditorInsertions.current += 1;
    let copyOperationId: string | null = null;
    const originalCopyTarget = spaceId && docId && attachmentDraftId ? {spaceId,documentId:docId,draftId:attachmentDraftId} : null;
    try {
    if ("original_name" in asset && !retainedAcknowledgement && getCurrentLaunchMode()!=="local") {
      if(!spaceId || !docId || !asset.document_id || !attachmentDraftId || !originalCopyTarget)throw new Error("Select an editable target draft before copying an asset.");
      if(asset.logical_attachment_id){
        const selection=await dataClient.selectExactAttachmentVersion(spaceId,asset.document_id,asset);
        verifyOriginalAsset();
        copyOperationId=await prepareAttachmentCopy(selection,originalCopyTarget);
        verifyOriginalAsset();
        asset=await runAttachmentCopy(copyOperationId);
        verifyOriginalAsset();
      } else {
        const selection=await fetchLegacyAttachmentCopySource(spaceId,asset.document_id,asset.id);
        verifyOriginalAsset();
        copyOperationId=await prepareLegacyAttachmentCopy(selection,originalCopyTarget);
        verifyOriginalAsset();
        asset=await runAttachmentCopy(copyOperationId);
        verifyOriginalAsset();
      }
    }
    if("original_name" in asset && retainedAcknowledgement && (asset.document_id!==docId || (asset.audience && asset.audience.draft_id!==attachmentDraftId)))throw new Error("Retained upload belongs to another original draft.");
    const name = "original_name" in asset ? asset.original_name : asset.name;
    const size = "size_bytes" in asset ? asset.size_bytes : asset.size;
    let assetUrl = asset.url;
    let attachmentId: string | null = null;

    if ("original_name" in asset && spaceId && asset.document_id) {
      attachmentId = asset.id;
      const preview = isEditorReusableImage(asset)
        ? await dataClient.previewPinnedDocumentAttachmentVersionImage(spaceId,asset.document_id,asset.id) : "";
      try { verifyOriginalAsset(); } catch (error) { releaseBrowserAttachmentImage(preview); throw error; }
      assetUrl = ownAttachmentImageUrl(preview);
    }

    verifyOriginalAsset();
    const activeEditor = originalEditor;
    if (!activeEditor || activeEditor.isDestroyed) return;

    const insertionPosition = insertionRange?.current();
    const replaceFile = replaceFileTarget;
    if (replaceFile) {
      replaceFile(name, size, assetUrl, attachmentId);
      if (fileReplacementRef.current === replaceFileTarget) fileReplacementRef.current = null;
    } else if (isEditorReusableImage(asset)) {
      const replaceImage = replaceImageTarget;
      if (replaceImage) {
        replaceImage(assetUrl, name, attachmentId);
        if (imageReplacementRef.current === replaceImageTarget) imageReplacementRef.current = null;
      } else {
        const insertAt = insertionPosition ?? activeEditor.state.selection.to;
        const inserted = activeEditor
          .chain()
          .focus()
          .insertContentAt(insertAt, [
            {
              type: "image",
              attrs: { src: assetUrl, alt: name, attachmentId },
            },
            { type: "paragraph" },
          ])
          .run();
        if (!inserted) {
          throw new Error(`Could not insert ${name} at the current editor selection.`);
        }
      }
    } else {
      if (!insertAttachmentCard(activeEditor, name, size, assetUrl, attachmentId, insertionPosition)) {
        throw new Error(`Could not insert ${name} at the current editor selection.`);
      }
    }
    insertionRange?.advance();
    verifyScope();
    onChange(htmlToMarkdown(activeEditor.getHTML()));
    if(copyOperationId && originalCopyTarget){await markAttachmentCopyAdopted(copyOperationId,originalCopyTarget);verifyScope();}
    if (uploadMenuRef.current === ownedUploadMenu) setUploadMenu(null);
    } finally { existingAssetInsertionBusy.current=false; insertionRange?.close(); pendingEditorInsertions.current -= 1; }
  };

  const closeCommentDialog = () => {
    if (commentSaving) return;
    setCommentDialog(null);
    setCommentContent("");
    setCommentError("");
  };

  const openCommentDialog = (activeEditor: TipTapEditor) => {
    if (!commentsAvailable || !spaceId || !docId) return;
    const { empty, from, to } = activeEditor.state.selection;
    if (onDraftCommentRequested) {
      const source = htmlToMarkdown(activeEditor.getHTML());
      const anchor = empty ? null : editorPositionToMarkdownOffset(activeEditor, from, source);
      const head = empty ? null : editorPositionToMarkdownOffset(activeEditor, to, source);
      onDraftCommentRequested(anchor === null || head === null ? null : {source,anchor,head});
      setSelectionToolbarPosition(null);
      setSelectionTypeMenuOpen(false);
      setContextMenu(null);
      setBlockMenu(null);
      return;
    }
    const selectionText = empty ? undefined : getSelectedText(activeEditor).trim().slice(0, 2_000);

    setCommentContent("");
    setCommentError("");
    setCommentDialog(
      selectionText ? { selectionStart: from, selectionEnd: to, selectionText } : {},
    );
    setSelectionToolbarPosition(null);
    setSelectionTypeMenuOpen(false);
    setContextMenu(null);
    setBlockMenu(null);
  };

  const submitComment = async () => {
    if (!spaceId || !docId || !commentDialog || !commentContent.trim() || commentSaving) return;

    setCommentSaving(true);
    setCommentError("");
    try {
      await api.fetch(`/spaces/${spaceId}/documents/${docId}/comments`, {
        method: "POST",
        body: {
          content: commentContent.trim(),
          selection_start: commentDialog.selectionStart,
          selection_end: commentDialog.selectionEnd,
          selection_text: commentDialog.selectionText,
        },
      });
      setCommentDialog(null);
      setCommentContent("");
      emitDocumentCommentsChanged(spaceId, docId);
      onCommentCreated?.();
    } catch {
      setCommentError("Unable to add your comment. Please try again.");
    } finally {
      setCommentSaving(false);
    }
  };

  const clearTemplateInsertionOverlay = () => {
    templateSchemaRequestIdRef.current += 1;
    templateInsertionTargetRef.current = null;
    setTemplateInsertionOverlay(null);
  };

  const insertTemplateMarkdown = (markdown: string): boolean => {
    const target = templateInsertionTargetRef.current;
    const activeEditor = editorInstanceRef.current;
    if (
      !target ||
      !activeEditor ||
      target.editor !== activeEditor ||
      target.doc !== activeEditor.state.doc ||
      activeEditor.isDestroyed ||
      readOnly ||
      target.from < 0 ||
      target.to < target.from ||
      target.to > activeEditor.state.doc.content.size
    ) return false;

    const html = markdownToHtml(markdown, true, templateFields);
    const parsed = ProseMirrorDOMParser.fromSchema(activeEditor.schema).parseSlice(
      new DOMParser().parseFromString(html, "text/html").body,
    );
    const transaction = createTemplateSectionInsertionTransaction(
      activeEditor.state,
      { from: target.from, to: target.to },
      parsed.content,
    );
    if (!transaction) return false;
    activeEditor.view.dispatch(transaction);
    activeEditor.view.focus();
    clearTemplateInsertionOverlay();
    return true;
  };

  const openTemplateSchemaFlow = (
    editorForInsert: TipTapEditor,
    range: { from: number; to: number },
    mode: "picker" | "section",
    template?: TemplateRecord,
  ) => {
    templateInsertionTargetRef.current = {
      editor: editorForInsert,
      doc: editorForInsert.state.doc,
      from: range.from,
      to: range.to,
    };
    const requestId = templateSchemaRequestIdRef.current + 1;
    templateSchemaRequestIdRef.current = requestId;
    setTemplateInsertionOverlay({ kind: "schema-loading", mode, template });
    if (!spaceId) {
      setTemplateInsertionOverlay({ kind: "schema-error", mode, template });
      return;
    }
    void dataClient.listPropertySchemas(spaceId).then(
      (schemas) => {
        if (
          requestId !== templateSchemaRequestIdRef.current ||
          templateInsertionTargetRef.current?.editor !== editorForInsert
        ) return;
        setTemplateInsertionOverlay(
          mode === "picker"
            ? { kind: "picker", schemas }
            : template
              ? { kind: "section", template, schemas }
              : null,
        );
      },
      () => {
        if (requestId !== templateSchemaRequestIdRef.current) return;
        setTemplateInsertionOverlay({ kind: "schema-error", mode, template });
      },
    );
  };

  const retryTemplateSchemaFlow = () => {
    if (!templateInsertionOverlay || templateInsertionOverlay.kind !== "schema-error") return;
    const target = templateInsertionTargetRef.current;
    if (!target) return;
    openTemplateSchemaFlow(
      target.editor,
      { from: target.from, to: target.to },
      templateInsertionOverlay.mode,
      templateInsertionOverlay.template,
    );
  };

  const prepareTemplateSectionInsertion = (
    template: TemplateRecord,
    schemas: PropertySchemaRecord[],
  ) => {
    const target = templateInsertionTargetRef.current;
    if (!target) return false;
    const resolved = resolveTemplateSection({
      template,
      context: templateContext,
      schemas,
    });
    if (resolved.unresolvedFields.length) return false;
    return insertTemplateMarkdown(resolved.content);
  };

  const handleTemplatePickerSelect = (template?: TemplateRecord) => {
    if (!template || !templateInsertionTargetRef.current || !editorInstanceRef.current) return false;
    const overlay = templateInsertionOverlay;
    if (!overlay || overlay.kind !== "picker") return false;
    if (getTemplateSectionFieldIds(template.content).size > 0) {
      setTemplateInsertionOverlay({ kind: "section", template, schemas: overlay.schemas });
      return true;
    }
    return prepareTemplateSectionInsertion(template, overlay.schemas);
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
          .insertContent(markdownToHtml(markdown, true, templateFields))
          .run();
        return;
      }

      activeEditor.chain().focus().insertContent(markdownToHtml(markdown, true, templateFields)).run();
    };

    if (command.id === "template") {
      setSlash(null);
      openTemplateSchemaFlow(activeEditor, selectedRange, "picker");
      return;
    }

    if (command.id.startsWith("template:")) {
      const template = templates.find((item) => command.id === `template:${item.id}`);
      if (template && getTemplateSectionFieldIds(template.content).size > 0) {
        setSlash(null);
        openTemplateSchemaFlow(activeEditor, selectedRange, "section", template);
        return;
      }
    }

    if (command.id === "template-guidance") {
      templateInsertionTargetRef.current = {
        editor: activeEditor,
        doc: activeEditor.state.doc,
        from: selectedRange.from,
        to: selectedRange.to,
      };
      setTemplateInsertionOverlay({ kind: "guidance", content: "" });
      setSlash(null);
      return;
    }

    if (command.id === "dynamic:space" || command.id === "dynamic:author") {
      setSlash(null);
      setLinkMenu(null);
      setContextMenu(null);
      openUtilityTargetDialog(command.id === "dynamic:space" ? "space" : "author", selectedRange);
      return;
    }

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
    } else if (command.id === "callout") {
      if (hasSelection) activeEditor.chain().focus().deleteRange(selectedRange).run();
      activeEditor
        .chain()
        .focus()
        .insertContent({ type: "callout", attrs: { kind: "note" } })
        .run();
    } else if (command.id === "expand") {
      if (hasSelection) activeEditor.chain().focus().deleteRange(selectedRange).run();
      activeEditor
        .chain()
        .focus()
        .insertContent({
          type: "expand",
          attrs: { title: "Click to expand" },
          content: [{ type: "paragraph" }],
        })
        .run();
    } else if (command.id === "child-pages" || command.id === "child-pages-collapsible") {
      if (hasSelection) activeEditor.chain().focus().deleteRange(selectedRange).run();
      activeEditor
        .chain()
        .focus()
        .insertContent({
          type: "childPages",
          attrs: { variant: command.id === "child-pages-collapsible" ? "collapsible" : "list" },
        })
        .insertContent({ type: "paragraph" })
        .run();
    } else if (command.id === "math") {
      if (hasSelection) activeEditor.chain().focus().deleteRange(selectedRange).run();
      activeEditor
        .chain()
        .focus()
        .insertContent({ type: "mathBlock", attrs: { expression: "" } })
        .run();
    } else if (command.id === "carousel") {
      if (hasSelection) activeEditor.chain().focus().deleteRange(selectedRange).run();
      activeEditor
        .chain()
        .focus()
        .insertContent({
          type: "carousel",
          attrs: {
            title: DEFAULT_CAROUSEL_TITLE,
            items: [{ src: "", alt: "Image 1" }],
          },
        })
        .run();
    } else if (command.id === "gallery") {
      if (hasSelection) activeEditor.chain().focus().deleteRange(selectedRange).run();
      activeEditor
        .chain()
        .focus()
        .insertContent({
          type: "gallery",
          attrs: {
            title: DEFAULT_GALLERY_TITLE,
            layout: "responsive-columns",
            items: [{ src: "", alt: "Image 1" }],
          },
        })
        .run();
    } else if (command.columnLayout) {
      const columnCount = getSectionColumnCount(command.columnLayout);
      activeEditor
        .chain()
        .focus()
        .deleteRange(selectedRange)
        .insertContent({
          type: "columns",
          attrs: { layout: command.columnLayout },
          content: Array.from({ length: columnCount }, () => ({
            type: "sectionColumn",
            content: [{ type: "paragraph" }],
          })),
        })
        .run();
    } else if (command.id === "code") {
      activeEditor.chain().focus().setTextSelection(selectedRange).toggleCodeBlock().run();
      insertParagraphAfterActiveBlock(activeEditor);
    } else if (command.id === "code-tabs") {
      if (hasSelection) activeEditor.chain().focus().deleteRange(selectedRange).run();
      activeEditor
        .chain()
        .focus()
        .insertContent({
          type: "codeTabs",
          attrs: {
            title: "Examples",
            tabs: [{ label: "Tab 1", language: "", code: "" }],
          },
        })
        .insertContent({ type: "paragraph" })
        .run();
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
      openWikiLinkDialog(selectedRange);
      setSlash(null);
      return;
    } else if (command.id === "mention") {
      openMentionDialog(selectedRange);
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
      openUploadMenu(command.id !== "file", selectedRange);
      activeEditor.chain().focus().setTextSelection(selectedRange).run();
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
    if (!activeEditor || readOnly || !activeEditor.isEditable) return;
    pendingEditorInsertions.current += 1;
    try {
    const text = await readClipboardText();
    if (activeEditor.isDestroyed || editorInstanceRef.current !== activeEditor || !activeEditor.isEditable) return;
    isInitializing.current = false;
    if (!text) {
      setContextMenu(null);
      return;
    }

    if (format === "markdown") {
      activeEditor.chain().focus().insertContent(markdownToHtml(text, true, templateFields)).run();
    } else {
      activeEditor.chain().focus().insertContent(text).run();
    }
    setContextMenu(null);
    } finally { pendingEditorInsertions.current -= 1; }
  };

  const applyTextAlignment = (alignment: "left" | "center" | "right") => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor) return;
    const { $from } = activeEditor.state.selection;
    for (let depth = $from.depth; depth > 0; depth -= 1) {
      const node = $from.node(depth);
      if (!(["paragraph", "heading", "blockquote"] as string[]).includes(node.type.name)) continue;
      const transaction = activeEditor.state.tr.setNodeMarkup($from.before(depth), undefined, {
        ...node.attrs,
        textAlign: alignment,
      });
      activeEditor.view.dispatch(transaction);
      activeEditor.view.focus();
      break;
    }
    setAlignmentMenuOpen(false);
  };

  const runToolbarAction = (action: MobileToolbarActionId, trigger?: HTMLElement) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || readOnly) return;

    if (trigger) {
      blockMenuFocusTriggerRef.current = trigger;
    }

    const chain = activeEditor.chain().focus();
    switch (action) {
      case "undo":
        activeEditor.chain().focus().undo().run();
        break;
      case "redo":
        activeEditor.chain().focus().redo().run();
        break;
      case "comment":
        openCommentDialog(activeEditor);
        break;
      case "link":
        if (!showMobileCommandButton && !activeEditor.state.selection.empty) {
          openSelectionLinkEditor(activeEditor);
        } else {
          openLinkDialog(activeEditor);
        }
        break;
      case "bookmark":
        openBookmarkDialog(activeEditor);
        break;
      case "upload":
        openUploadMenu(true);
        break;
      case "tag":
        openTagDialog(activeEditor);
        break;
      case "sectionControls":
        if (sectionControlsOpen) {
          dismissSectionControlsMenu(false);
        } else {
          setSectionControlsOpen(true);
        }
        break;
      case "blockControls": {
        const block = getCurrentTopLevelBlock(activeEditor);
        if (!block || block.node.type.name === "section") break;
        const triggerRect = trigger?.getBoundingClientRect();
        const menuPosition = triggerRect
          ? {
              top:
                triggerRect.bottom + 8 < window.innerHeight - 280
                  ? triggerRect.bottom + 8
                  : Math.max(8, triggerRect.top - 280),
              left: Math.max(
                8,
                Math.min(triggerRect.left + triggerRect.width / 2 - 88, window.innerWidth - 184),
              ),
            }
          : undefined;
        updateBlockMenuForBlock(activeEditor, block, true, menuPosition);
        break;
      }
      case "heading":
        if (mobileHeadingMenuOpen) {
          dismissMobileHeadingMenu(false);
        } else {
          setMobileHeadingMenuOpen(true);
        }
        break;
      case "font":
        if (fontFamilyMenuOpen) {
          dismissFontFamilyMenu(false);
        } else {
          setFontFamilyMenuOpen(true);
        }
        break;
      case "fontSize":
        if (fontSizeMenuOpen) dismissFontSizeMenu(false);
        else setFontSizeMenuOpen(true);
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
        if (highlightColorMenuOpen) {
          dismissHighlightColorMenu(false);
        } else {
          setHighlightColorMenuOpen(true);
        }
        break;
      case "color":
        if (textColorMenuOpen) {
          dismissTextColorMenu(false);
        } else {
          setTextColorMenuOpen(true);
        }
        break;
      case "alignment":
        setAlignmentMenuOpen((open) => !open);
        break;
      case "emoji":
        setEmojiMenuOpen((open) => !open);
        break;
      case "code":
        chain.toggleCode().run();
        break;
      case "codeBlock":
        chain.toggleCodeBlock().run();
        insertParagraphAfterActiveBlock(activeEditor);
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
            activeEditor
              .chain()
              .focus()
              .deleteRange({ from: blockLineStart, to: blockLineStart + 2 })
              .run();
          } else if (lineText.startsWith("\t")) {
            activeEditor
              .chain()
              .focus()
              .deleteRange({ from: blockLineStart, to: blockLineStart + 1 })
              .run();
          }
        } else {
          chain.liftListItem("listItem").run();
        }
        break;
      case "commands":
        openSearchableCommandPalette(activeEditor, undefined, commandMenuTriggerRef.current);
        break;
    }
  };

  const runSectionControl = (
    action: "insert-before" | "move-up" | "add-block" | "move-down" | "insert-after" | "delete",
  ) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || readOnly) return;
    const section = getActiveEditorSection(activeEditor) ?? activeSection;
    if (!section) return;

    switch (action) {
      case "insert-before":
        insertActiveEditorSection(activeEditor, section, "before");
        break;
      case "move-up":
        moveActiveEditorSection(activeEditor, section, "up");
        break;
      case "add-block":
        addBlockToActiveEditorSection(activeEditor, section);
        break;
      case "move-down":
        moveActiveEditorSection(activeEditor, section, "down");
        break;
      case "insert-after":
        insertActiveEditorSection(activeEditor, section, "after");
        break;
      case "delete":
        deleteActiveEditorSection(activeEditor, section);
        break;
    }
    dismissSectionControlsMenu(false);
  };

  const chooseActiveSectionLayout = (layout: SectionLayout) => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || readOnly) return;
    const section = getActiveEditorSection(activeEditor) ?? activeSection;
    if (!section) return;
    setActiveEditorSectionLayout(activeEditor, section, layout);
    dismissSectionControlsMenu(false);
  };

  const updateActiveSectionStyle = (style: Partial<SectionStyle>) => {
    const activeEditor = editorInstanceRef.current;
    const section = activeEditor
      ? (getActiveEditorSection(activeEditor) ?? activeSection)
      : activeSection;
    if (!activeEditor || !section || readOnly) return;
    setActiveEditorSectionStyle(activeEditor, section, { ...section.style, ...style });
  };

  const prepareEditorContextMenu = useCallback(
    (event: MouseEvent) => {
      if (!showBlockChrome || readOnly) return false;
      const target = event.target;
      const activeEditor = editorInstanceRef.current;
      if (
        !activeEditor ||
        activeEditor.isDestroyed ||
        !(target instanceof Element) ||
        !activeEditor.view.dom.contains(target) ||
        target.closest(EDITOR_CANVAS_CHROME_SELECTOR)
      ) {
        return false;
      }

      setSlash(null);
      setBlockMenu(null);
      setLinkMenu(null);
      setUploadMenu(null);
      setTagDialog(null);
      const position = activeEditor.view.posAtCoords({
        left: event.clientX,
        top: event.clientY,
      });
      if (position && activeEditor.state.selection.empty) {
        contextMenuOpeningRef.current = true;
        activeEditor.chain().focus().setTextSelection(position.pos).run();
      } else {
        contextMenuOpeningRef.current = false;
        activeEditor.view.focus();
      }
      return true;
    },
    [readOnly, setSlash, setUploadMenu, showBlockChrome],
  );

  const openEditorContextMenu = useCallback(() => {
    const activeEditor = editorInstanceRef.current;
    if (!activeEditor || activeEditor.isDestroyed || readOnly) return;
    setContextMenu(getContextMenuState(activeEditor, !activeEditor.state.selection.empty));
  }, [readOnly]);

  const editor = useEditor({
    extensions: [
      SectionDocument,
      StarterKit.configure({
        document: false,
        codeBlock: false,
        heading: { levels: [1, 2, 3, 4, 5, 6] },
        link: false,
        underline: false,
      }),
      Placeholder.configure({
        placeholder: showBlockChrome
          ? "Start writing, or type / for commands..."
          : "Start writing, then use Commands above the keyboard...",
      }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Link.configure({
        autolink: false,
        openOnClick: false,
        protocols: ["starlane"],
        HTMLAttributes: {
          class: "starlane-link cursor-pointer",
          target: "_blank",
          rel: "noopener noreferrer",
        },
      }),
      ImageExt.extend({
        // Images are content blocks. NodeSelection gives the shared block
        // controls a stable target without turning the image into text
        // selection or handing the edit surface to the system selection UI.
        selectable: true,
        draggable: false,
        addAttributes() {
          return {
            ...(this.parent?.() ?? {}),
            alignment: {
              default: "left",
              parseHTML: (element) =>
                normalizeImageAlignment(element.getAttribute("data-starlane-align")),
              renderHTML: (attributes) => {
                const alignment = normalizeImageAlignment(attributes.alignment);
                return alignment === "left" ? {} : { "data-starlane-align": alignment };
              },
            },
            attachmentId: {
              default: null,
              parseHTML: (element) => element.getAttribute("data-attachment-id"),
              renderHTML: (attributes) =>
                attributes.attachmentId
                  ? { "data-attachment-id": String(attributes.attachmentId) }
                  : {},
            },
          };
        },
        addNodeView() {
          return ReactNodeViewRenderer(ImageBlockNodeView);
        },
      }).configure(EDITOR_IMAGE_OPTIONS),
      StarlaneTable.configure({ resizable: true, View: StarlaneTableView }),
      TableRow,
      TableHeader,
      TableCell,
      Highlight.configure({ multicolor: true }),
      TextColor,
      Typography,
      Underline,
      FontFamily,
      FontSize,
      TextAlignment,
      ContentTag,
      ContentMention,
      WikiLinkUnresolved,
      StarlaneCodeBlock.configure({ lowlight: editorLowlight }),
      CodeTabs,
      Callout,
      Expand,
      MathBlock,
      Carousel,
      Gallery,
      TableOfContents,
      ChildPages.configure({ spaceId, docId }),
      TemplateMarkerNode,
      TemplateValueNode,
      BookmarkCard,
      FileAttachment.configure({ spaceId, docId, captureSelection:()=>captureAttachmentHistoryRef.current() }),
      EditorSection,
      EditorColumns,
      EditorSectionColumn,
      InlineCommentDecorations,
      DraftPeerCursorDecorations,
      ...createStarlaneBlockInteractionExtensions(),
    ],
    content: initialEditorContentRef.current,
    editable: !readOnly,
    autofocus: autoFocus && !readOnly,
    editorProps: {
      attributes: {
        class: [
          "tiptap-editor starlane-editor-writing-surface outline-none",
          plainMode ? "font-mono text-sm" : "font-sans text-body",
        ].join(" "),
        "aria-label": "Document editor",
        spellcheck: "true",
      },
      handleTextInput: (_view, from, _to, text): boolean => {
        if (!readOnly) isInitializing.current = false;
        const activeEditor = editorInstanceRef.current;
        if (
          readOnly ||
          text !== "/" ||
          !activeEditor ||
          !canStartInlineSlashMenu(activeEditor.state, from)
        ) {
          return false;
        }

        window.requestAnimationFrame(() => {
          if (activeEditor.isDestroyed) return;
          const query = deriveInlineSlashQuery(activeEditor.state, from);
          if (!query) return;
          slashPaletteTriggerRef.current = activeEditor.view.dom;
          const coords = activeEditor.view.coordsAtPos(query.to);
          setSlash({
            ...query,
            position: {
              top: Math.min(coords.bottom + 6, window.innerHeight - 340),
              left: Math.max(8, Math.min(coords.left, window.innerWidth - 280)),
            },
            searchable: false,
          });
        });
        return false;
      },
      handleKeyDown: (_view, event): boolean => {
        const activeEditor = editorInstanceRef.current;

        if (event.key === "Escape") {
          setSlash(null);
          setContextMenu(null);
          setBlockMenu(null);
          setLinkMenu(null);
          setLinkDialog(null);
          setBookmarkDialog(null);
          setUploadMenu(null);
          setTagDialog(null);
          setSelectionToolbarPosition(null);
          setSelectionTypeMenuOpen(false);
          setTableStyleDialogOpen(false);
          dismissMobileHeadingMenu();
          dismissSectionControlsMenu();
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

        const blockMoveDirection = getBlockMoveShortcut(event);
        if (blockMoveDirection && activeEditor && !readOnly) {
          event.preventDefault();
          if (moveCurrentBlock(activeEditor, blockMoveDirection)) {
            announceBlockMove(blockMoveDirection);
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
        return activeEditor
          .chain()
          .focus()
          .addRowAfter()
          .command(({ tr }) => {
            selectFirstCellInLastTableRow(tr);
            return true;
          })
          .run();
      },
      handlePaste: (_view, event): boolean => {
        if (!readOnly) isInitializing.current = false;
        if (readOnly) return false;
        const clipboard = event.clipboardData;
        const imageFiles = Array.from(clipboard?.files ?? [])
          .filter(isEditorFile)
          .filter((file) => isEditorImageFile(file));
        const pastedItems = Array.from(clipboard?.items ?? [])
          .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
          .map((item) => item.getAsFile())
          .filter((file): file is File => Boolean(file));
        const files = [...imageFiles, ...pastedItems]
          .filter(isEditorFile)
          .filter((file, index, all) => all.findIndex((candidate) => candidate === file) === index);
        if (files.length === 0) return false;
        event.preventDefault();
        void handlePickedFiles(files);
        return true;
      },
      handleDrop: (_view, event, _slice, moved): boolean => {
        if (!readOnly) isInitializing.current = false;
        if (readOnly || moved) return false;
        const files = Array.from(event.dataTransfer?.files ?? []).filter(isEditorFile);
        if (files.length === 0) return false;
        event.preventDefault();
        void handlePickedFiles(files);
        return true;
      },
      handleDOMEvents: {
        // Real input must reach the page/checkpoint even inside the initial100ms.
        // Programmatic initial schema normalization still uses the existing hold.
        beforeinput: () => { if (!readOnly) isInitializing.current = false; return false; },
        click: (view, event) => {
          if (!(event.target instanceof Element)) return false;
          const inlineComment = event.target.closest<HTMLElement>("[data-inline-comment-id]");
          if (inlineComment && view.dom.contains(inlineComment)) {
            onInlineCommentOpen?.(inlineComment.dataset.inlineCommentId || "");
            return true;
          }
          const anchor = event.target.closest("a[href]");
          if (!anchor || !view.dom.contains(anchor)) return false;

          event.preventDefault();
          const activeEditor = editorInstanceRef.current;
          const href = anchor.getAttribute("href") || "";
          const range = activeEditor ? getLinkRangeAtAnchor(activeEditor, anchor, href) : null;
          const rect = anchor.getBoundingClientRect();

          if (range) {
            setSlash(null);
            setContextMenu(null);
            setLinkDialog(null);
            setUploadMenu(null);
            linkMenuTriggerRef.current = anchor instanceof HTMLElement ? anchor : null;
            setLinkMenu({
              ...range,
              href,
              position: getRectFloatingPosition(rect, 384, 40),
            });
            return true;
          }

          window.open(href, "_blank", "noopener,noreferrer");
          return true;
        },
      },
    },
    onCreate: ({ editor }) => {
      editorInstanceRef.current = editor;
      editor.on("destroy", () => clearEditorInstanceIfCurrent(editorInstanceRef, editor));
      onEditorReady?.(editor);
      // Allow onChange after a short delay (lets initial content settle)
      setTimeout(() => {
        isInitializing.current = false;
      }, 100);
    },
    onUpdate: ({ editor }) => {
      syncInlineSlashMenu(editor);
      if (isInitializing.current) return;
      updateBlockMenu(editor);
      updateTableToolbar(editor);
      updateActiveSection(editor);
      const html = editor.getHTML();
      const md = htmlToMarkdown(html);
      onChange(md);
    },
    onSelectionUpdate: ({ editor }) => {
      syncInlineSlashMenu(editor);
      updateBlockMenu(editor);
      updateSelectionMenu(editor);
      updateTableToolbar(editor);
      updateActiveSection(editor);
      const source = htmlToMarkdown(editor.getHTML());
      const anchor = editorPositionToMarkdownOffset(editor,editor.state.selection.anchor,source);
      const head = editorPositionToMarkdownOffset(editor,editor.state.selection.head,source);
      onSourceSelectionChange?.(anchor === null || head === null ? null : {source,anchor,head});
      if (contextMenuOpeningRef.current) {
        contextMenuOpeningRef.current = false;
        return;
      }
      setContextMenu(null);
    },
    onFocus: ({ editor }) => {
      updateBlockMenu(editor);
      updateSelectionMenu(editor);
      updateTableToolbar(editor);
      updateActiveSection(editor);
    },
    onBlur: () => {
      window.setTimeout(() => {
        if (!blockMenuOpenRef.current) {
          setBlockMenu(null);
        }
        const activeElement = document.activeElement;
        const focusInsideLinkMenu = Boolean(
          activeElement && linkMenuRef.current?.contains(activeElement),
        );
        if (!focusInsideLinkMenu) {
          setLinkMenu(null);
        }
        const focusInsideSelectionToolbar = Boolean(
          activeElement &&
          (selectionToolbarRef.current?.contains(activeElement) ||
            selectionTypeMenuRef.current?.contains(activeElement)),
        );
        if (!focusInsideSelectionToolbar) {
          setSelectionToolbarPosition(null);
          setSelectionTypeMenuOpen(false);
        }
        const activeEditor = editorInstanceRef.current;
        if (activeEditor && !activeEditor.isDestroyed) {
          // A table is selected independently of browser focus. Keep its dock
          // mounted through title/sidebar/toolbar focus handoffs and dismiss it
          // only when the editor selection actually leaves the table.
          updateTableToolbar(activeEditor);
        }
      }, 120);
    },
  });

  // The editor hook can publish a replacement instance before the older instance's
  // destroy lifecycle finishes. Keep synchronous toolbar and palette commands pointed
  // at the instance rendered by this component; the guarded destroy listener prevents
  // the older instance from clearing this reference afterward.
  if (editor && !editor.isDestroyed) {
    editorInstanceRef.current = editor;
  }

  const captureAssetInsertion = useAssetInsertionScope(editor, {
    ...assetContext, draft_id: attachmentDraftId, readOnly,
  });
  const captureAssetInsertionRef = useRef(captureAssetInsertion);
  captureAssetInsertionRef.current = captureAssetInsertion;
  captureAttachmentHistoryRef.current = () => {
    const selection = attachmentSelectionRef.current;
    const originalEditor = editorInstanceRef.current;
    const context = captureAttachmentContext();
    const verify = () => {
      if (!originalEditor || originalEditor.isDestroyed || editorInstanceRef.current !== originalEditor ||
        attachmentSelectionRef.current !== selection) throw new Error("Original attachment draft changed; retain the replacement for review.");
      verifyAttachmentContext(context,attachmentApiBase(),attachmentApi.getToken());
    };
    verify();
    const verifyEdit = !selection.readOnly ? captureAssetInsertion() : undefined;
    return {draftId:selection.draftId,verify,verifyEdit};
  };
  useEffect(() => {
    fileReplacementRef.current = null;
    imageReplacementRef.current = null;
    setUploadMenu(null);
    setImageAttachmentHistory(null);
  }, [assetContext.space_id,assetContext.document_id,attachmentDraftId,readOnly,setUploadMenu]);

  useEffect(() => {
    if (!editor || !selectionToolbarPosition || editor.isDestroyed) return;

    const repositionSelectionToolbar = () => {
      if (
        editor.isDestroyed ||
        !shouldShowSelectionToolbar({
          mobile: showMobileCommandButton,
          readOnly,
          persistentToolbar: Boolean(desktopToolbarHost),
          selection: editor.state.selection,
          inTable: editor.isActive("table"),
        })
      ) {
        setSelectionToolbarPosition(null);
        setSelectionTypeMenuOpen(false);
        return;
      }
      setSelectionToolbarPosition(getSelectionFloatingPosition(editor, 372, 40));
    };

    window.addEventListener("resize", repositionSelectionToolbar);
    window.addEventListener("scroll", repositionSelectionToolbar, true);
    window.visualViewport?.addEventListener("resize", repositionSelectionToolbar);
    window.visualViewport?.addEventListener("scroll", repositionSelectionToolbar);
    return () => {
      window.removeEventListener("resize", repositionSelectionToolbar);
      window.removeEventListener("scroll", repositionSelectionToolbar, true);
      window.visualViewport?.removeEventListener("resize", repositionSelectionToolbar);
      window.visualViewport?.removeEventListener("scroll", repositionSelectionToolbar);
    };
  }, [desktopToolbarHost, editor, readOnly, selectionToolbarPosition, showMobileCommandButton]);

  useEffect(() => {
    if (!desktopToolbarHost || readOnly || showMobileCommandButton) return;
    setSelectionToolbarPosition(null);
    setSelectionTypeMenuOpen(false);
  }, [desktopToolbarHost, readOnly, showMobileCommandButton]);

  useEffect(() => {
    if (!editor || editor.isDestroyed || !commentsAvailable || !spaceId || !docId) {
      setDocumentComments([]);
      return;
    }

    let cancelled = false;
    const load = async () => {
      try {
        const nextComments = await listDocumentComments(spaceId, docId);
        if (!cancelled) setDocumentComments(nextComments);
      } catch {
        if (!cancelled) setDocumentComments([]);
      }
    };
    void load();
    const unsubscribe = subscribeToDocumentComments(spaceId, docId, () => void load());
    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, [commentsAvailable, docId, editor, spaceId]);

  useEffect(() => {
    if (
      !editor ||
      editor.isDestroyed ||
      getCurrentLaunchMode() === "local" ||
      !assetContext.space_id ||
      !assetContext.document_id
    ) {
      return;
    }

    let cancelled = false;
    const selection = attachmentSelectionRef.current;
    const revision = externalContentRevision.current;
    const hydrateAttachments = async () => {
      const context = captureAttachmentContext();
      const current = () => {
        if (cancelled || selection !== attachmentSelectionRef.current || revision !== externalContentRevision.current ||
          editor.isDestroyed || editorInstanceRef.current !== editor) return false;
        try { verifyAttachmentContext(context,attachmentApiBase(),attachmentApi.getToken()); return true; }
        catch { return false; }
      };
      const verifyMigration = !readOnly && editor.isEditable ? captureAssetInsertion() : null;
      const attachmentIds = new Set<string>();
      const imageAttachmentIds = new Set<string>();
      const embeddedImages: Array<{ pos: number; src: string; alt: string; node: ProseMirrorNode }> = [];
      editor.state.doc.descendants((node) => {
        const attachmentId = String(node.attrs.attachmentId || "").trim();
        const needsImageHydration = node.type.name === "image" && !String(node.attrs.src || "");
        const needsFileHydration =
          node.type.name === "fileAttachment" && Number(node.attrs.size || 0) === 0;
        if (attachmentId && (needsImageHydration || needsFileHydration)) {
          attachmentIds.add(attachmentId);
          if (needsImageHydration) imageAttachmentIds.add(attachmentId);
        }
      });
      editor.state.doc.descendants((node, pos) => {
        const src = String(node.attrs.src || "");
        if (verifyMigration && node.type.name === "image" && !node.attrs.attachmentId && /^data:image\//i.test(src)) {
          embeddedImages.push({ pos, src, alt: String(node.attrs.alt || ""), node });
        }
      });
      if (attachmentIds.size === 0 && embeddedImages.length === 0) return;

      const attachments = await dataClient
        .listDocumentAttachments(assetContext.space_id!, assetContext.document_id!)
        .catch(() => []);
      if (!current()) return;
      const byId = new Map(attachments.map((attachment) => [attachment.id, attachment]));
      const urls = new Map<string, string>();
      const migrated = new Map<number, { src: string; attachmentId: string; node: ProseMirrorNode }>();
      await Promise.all(
        Array.from(imageAttachmentIds).map(async (attachmentId) => {
          const dataUrl = await dataClient.previewPinnedDocumentAttachmentVersionImage(assetContext.space_id!,assetContext.document_id!,attachmentId).catch(() => null);
          if (dataUrl) {if(!current()){releaseBrowserAttachmentImage(dataUrl);return;}urls.set(attachmentId,dataUrl);}
        }),
      );
      try {
      await Promise.all(
        embeddedImages.map(async ({ pos, src, alt, node }) => {
          verifyMigration?.();
          if (!current() || editor.state.doc.nodeAt(pos) !== node) return;
          const mimeType = src.match(/^data:([^;,]+)/i)?.[1] || "image/png";
          const saved = await dataClient
            .saveDocumentDataAsset({
              name: normalizeImageCaption(alt) || `image-${Date.now()}-${pos}.png`,
              mime_type: mimeType,
              data_url: src,
              space_id: assetContext.space_id,
              document_id: assetContext.document_id,
              draft_id: attachmentDraftId,
            })
            .catch(() => null);
          verifyMigration?.();
          if (current() && editor.state.doc.nodeAt(pos) === node && saved && "id" in saved)
            migrated.set(pos, { src, attachmentId: saved.id, node });
        }),
      );
      } catch { urls.forEach(releaseBrowserAttachmentImage); return; }
      const hasAttachmentMetadata = Array.from(attachmentIds).some((id) => byId.has(id));
      if (
        !current() ||
        (urls.size === 0 && migrated.size === 0 && !hasAttachmentMetadata) ||
        editor.isDestroyed
      ) { urls.forEach(releaseBrowserAttachmentImage); return; }

      if (migrated.size) {
        try { verifyMigration?.(); } catch { urls.forEach(releaseBrowserAttachmentImage); return; }
      }
      const transaction = editor.state.tr;
      let migratedAny = false;
      editor.state.doc.descendants((node, pos) => {
        const attachmentId = String(node.attrs.attachmentId || "").trim();
        const hydratedSrc = urls.get(attachmentId);
        const attachment = byId.get(attachmentId);
        const migratedImage = migrated.get(pos);
        if (node.type.name === "fileAttachment" && attachment) {
          const nextAttributes = {
            ...node.attrs,
            name: attachment.original_name || attachment.filename,
            size: attachment.size_bytes,
            url: String(node.attrs.url || ""),
            attachmentId: attachment.id,
          };
          if (
            node.attrs.name !== nextAttributes.name ||
            node.attrs.size !== nextAttributes.size ||
            node.attrs.url !== nextAttributes.url
          ) {
            transaction.setNodeMarkup(pos, undefined, nextAttributes);
          }
          return;
        }
        if (node.type.name !== "image") return;
        if (migratedImage && migratedImage.node === node) {
          migratedAny = true;
          transaction.setNodeMarkup(pos, undefined, {
            ...node.attrs,
            src: migratedImage.src,
            attachmentId: migratedImage.attachmentId,
          });
        } else if (hydratedSrc && node.attrs.src !== hydratedSrc) {
          transaction.setNodeMarkup(pos, undefined, { ...node.attrs, src: hydratedSrc });
        }
      });
      if (transaction.docChanged && current()) {
        // Display-only preview metadata must not emit an authoring/save update.
        if (!migratedAny) transaction.setMeta("preventUpdate",true);
        urls.forEach(ownAttachmentImageUrl);
        editor.view.dispatch(transaction);
      } else urls.forEach(releaseBrowserAttachmentImage);
    };

    void hydrateAttachments().catch(() => { /* Retain source and upload receipts; hydration grants no adoption. */ });
    return () => {
      cancelled = true;
    };
  }, [assetContext.document_id, assetContext.space_id, content, editor, ownAttachmentImageUrl, attachmentDraftId, readOnly, captureAssetInsertion]);

  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    updateInlineCommentDecorations(editor, documentComments, inlineCommentsVisible);
  }, [documentComments, editor, inlineCommentsVisible]);

  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    const ranges: DraftPeerEditorRange[] = [];
    for (const cursor of draftPeerCursors) {
      const anchor = markdownOffsetToEditorPosition(editor,cursor.source,cursor.anchor);
      const head = markdownOffsetToEditorPosition(editor,cursor.source,cursor.head);
      if (anchor !== null && head !== null) ranges.push({connectionId:cursor.connectionId,displayName:cursor.displayName,anchor,head});
    }
    updateDraftPeerCursorDecorations(editor,ranges);
  }, [draftPeerCursors,editor]);

  useEffect(() => {
    if (!editor || editor.isDestroyed || readOnly) return;

    const handleDocumentKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !isCommandPaletteShortcut(event)) return;
      const target = event.target;
      if (!(target instanceof Node) || !editor.view.dom.contains(target)) return;
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
    if (!editor || editor.isDestroyed || readOnly) return;

    // ProseMirror handles the key first. Stopping the native event afterwards keeps
    // workspace/document shortcuts from stealing Space or Enter while the writing
    // canvas owns a caret, including an empty paragraph where the event target is
    // the editor root rather than a text node.
    const handleEditorCanvasKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      if (!(target instanceof Element) || !editor.view.dom.contains(target)) return;
      if (!target.closest("[contenteditable='true']")) return;
      event.stopPropagation();
    };

    editor.view.dom.addEventListener("keydown", handleEditorCanvasKeyDown);
    return () => editor.view.dom.removeEventListener("keydown", handleEditorCanvasKeyDown);
  }, [editor, readOnly]);

  useEffect(() => {
    if (!editor || editor.isDestroyed || readOnly) return;

    const syncEditorContextControls = () => {
      updateTableToolbar(editor);
      // Code-frame controls share the focused block position. Recalculate them
      // during a document scroll instead of leaving a fixed header behind.
      updateBlockMenu(editor);
    };
    window.addEventListener("resize", syncEditorContextControls);
    window.addEventListener("scroll", syncEditorContextControls, true);
    return () => {
      window.removeEventListener("resize", syncEditorContextControls);
      window.removeEventListener("scroll", syncEditorContextControls, true);
    };
  }, [editor, readOnly, updateBlockMenu, updateTableToolbar]);

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

  useEffect(() => {
    const wasOpen = previousMobileKeyboardOpenRef.current;
    const keyboardOpen = mobileKeyboardInset > 0;
    previousMobileKeyboardOpenRef.current = keyboardOpen;
    const viewport = window.visualViewport;
    const layoutHeight = Math.max(window.innerHeight, document.documentElement.clientHeight);
    const viewportContracted = Boolean(
      viewport && isMobileViewportContracted(layoutHeight, viewport.height),
    );
    if (!editor || editor.isDestroyed) {
      return;
    }

    const keyboardJustOpened = keyboardOpen && !wasOpen;
    const keyboardJustClosed = shouldRestoreEditorSelectionAfterKeyboardClose({
      wasOpen,
      keyboardOpen,
      viewportContracted,
    });
    if (!keyboardJustOpened && !keyboardJustClosed) return;

    // WebKit may update the visual viewport after the selection is focused.
    // Re-apply the selected block after two frames so the keyboard cannot cover
    // the caret, and restore the nearest position once the keyboard closes.
    let secondFrame = 0;
    const firstFrame = window.requestAnimationFrame(() => {
      secondFrame = window.requestAnimationFrame(() => {
        if (!editor.isDestroyed) {
          scrollEditorSelectionIntoView(editor, { preferCenter: keyboardJustOpened });
        }
      });
    });
    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame) window.cancelAnimationFrame(secondFrame);
    };
  }, [editor, mobileKeyboardInset]);

  const externalSelectionRefreshRef = useRef<(active: TipTapEditor) => void>(() => {});
  externalSelectionRefreshRef.current = (active) => {
    updateTableToolbar(active); updateActiveSection(active);
    updateInlineCommentDecorations(active, documentComments, inlineCommentsVisible);
  };
  useEffect(() => {
    if (!editor || editor.isDestroyed || !onExternalContentReady) return;
    const apply: ApplyExternalEditorContent = (source, isCurrent, expectedContent) => {
      if (isInitializing.current || editor.isDestroyed || editor.view.composing || pendingEditorInsertions.current || document.querySelector('[role="dialog"]') || !isCurrent()) return false;
      const expectedHtml = markdownToHtml(expectedContent, false, externalTemplateFieldsRef.current);
      const expectedElement = document.createElement("div"); expectedElement.innerHTML = expectedHtml;
      const expectedDocument = ProseMirrorDOMParser.fromSchema(editor.schema).parse(expectedElement);
      if (editor.state.doc.textContent !== expectedDocument.textContent || htmlToMarkdown(editor.getHTML()) !== htmlToMarkdown(expectedHtml)) return false;
      const element = document.createElement("div");
      element.innerHTML = markdownToHtml(source, false, externalTemplateFieldsRef.current);
      const nextDocument = ProseMirrorDOMParser.fromSchema(editor.schema).parse(element);
      if (editor.isDestroyed || editor.view.composing || !isCurrent()) return false;
      const focused = editor.isFocused;
      let head = Math.min(editor.state.selection.head, nextDocument.content.size);
      const resolved = nextDocument.resolve(head);
      const beforeText = resolved.nodeBefore?.text, afterText = resolved.nodeAfter?.text;
      const before = beforeText?.charCodeAt(beforeText.length - 1), after = afterText?.charCodeAt(0);
      if (before !== undefined && after !== undefined && before >= 0xd800 && before <= 0xdbff && after >= 0xdc00 && after <= 0xdfff) head -= 1;
      // New peer source clears undo history. Preserve focus and collapse the caret
      // near its prior document position; this is not a Markdown cursor mapping.
      const nextState = EditorState.create({ schema: editor.schema, doc: nextDocument,
        plugins: editor.state.plugins, selection: TextSelection.near(nextDocument.resolve(head)) });
      externalContentRevision.current += 1;
      editor.view.updateState(nextState);
      setBlockMenu(null); setContextMenu(null); setLinkMenu(null);
      setSelectionToolbarPosition(null); externalSelectionRefreshRef.current(editor);
      if (focused) editor.view.focus();
      return true;
    };
    onExternalContentReady(apply);
    return () => onExternalContentReady(null);
  }, [editor, onExternalContentReady]);

  // Update content from outside (e.g., initial load)
  useEffect(() => {
    if (!editor || editor.isDestroyed || editor.isFocused) return;

    const currentHtml = editor.getHTML();
    const newHtml = markdownToHtml(content, false, templateFields);

    // Compare portable Markdown as well as rendered HTML. Uploaded images keep a
    // session-local preview URL while Markdown stores their attachment reference,
    // so the HTML forms intentionally differ when the content is already current.
    if (
      currentHtml !== newHtml &&
      shouldApplyExternalEditorContent(htmlToMarkdown(currentHtml), content)
    ) {
      // This is a parent-to-editor sync, not a user edit. Do not emit an
      // update that serializes the same content back to the parent and can
      // create a markdown/HTML feedback loop.
      editor.commands.setContent(newHtml, { emitUpdate: false });
    }
  }, [content, editor, templateFields]);

  // Update editable state
  useEffect(() => {
    if (editor && !editor.isDestroyed) {
      editor.setEditable(!readOnly);
    }
  }, [readOnly, editor]);

  if (!editor) return null;

  const templateInsertionPortalTarget = typeof document !== "undefined" ? document.body : null;
  const renderTemplateInsertionPortal = (content: ReactNode) =>
    templateInsertionPortalTarget ? createPortal(content, templateInsertionPortalTarget) : null;

  const activeHeadingLevel =
    ([1, 2, 3, 4, 5, 6] as const).find((level) => editor.isActive("heading", { level })) || "";
  const activeFontFamily = String(editor.getAttributes("fontFamily").fontFamily || "");
  const filteredFontGroups = filterEditorFontGroups(fontFamilyQuery);
  const mobileToolbarPlacement = getMobileToolbarPlacement(mobileKeyboardInset);
  const mobileKeyboardOpen = mobileToolbarPlacement.keyboardOpen;
  const mobileToolbarBottom = mobileToolbarPlacement.toolbarBottom;
  const mobileMenuBottom = mobileToolbarPlacement.menuBottom;
  const activeTableColumn = getActiveTableColumnContext(editor);
  const activeTableStyle = normalizeTableStyle(editor.getAttributes("table").tableStyle);
  const tableCanSort = Boolean(
    activeTableColumn && activeTableColumn.table.childCount - activeTableColumn.headerRowCount > 1,
  );
  const tableActions = [
    {
      label: "Table style",
      group: "style",
      icon: <Paintbrush size={16} />,
      run: () => setTableStyleDialogOpen(true),
    },
    {
      label: "Sort column A to Z",
      group: "sort",
      icon: <ArrowDownAZ size={16} />,
      disabled: !tableCanSort,
      run: () => sortActiveTableByColumn(editor, "ascending"),
    },
    {
      label: "Sort column Z to A",
      group: "sort",
      icon: <ArrowDownZA size={16} />,
      disabled: !tableCanSort,
      run: () => sortActiveTableByColumn(editor, "descending"),
    },
    {
      label: "Toggle header row",
      group: "table",
      icon: <TableProperties size={16} />,
      run: () => editor.chain().focus().toggleHeaderRow().run(),
    },
    {
      label: "Add row above",
      group: "row",
      icon: <BetweenHorizontalStart size={16} />,
      run: () => editor.chain().focus().addRowBefore().run(),
    },
    {
      label: "Add row below",
      group: "row",
      icon: <BetweenHorizontalEnd size={16} />,
      run: () => editor.chain().focus().addRowAfter().run(),
    },
    {
      label: "Add column left",
      group: "column",
      icon: <BetweenVerticalStart size={16} />,
      run: () => editor.chain().focus().addColumnBefore().run(),
    },
    {
      label: "Move column left",
      group: "column",
      icon: <ChevronLeft size={16} />,
      disabled: !activeTableColumn || activeTableColumn.column === 0,
      run: () => moveActiveTableColumn(editor, "left"),
    },
    {
      label: "Add column right",
      group: "column",
      icon: <BetweenVerticalEnd size={16} />,
      run: () => editor.chain().focus().addColumnAfter().run(),
    },
    {
      label: "Move column right",
      group: "column",
      icon: <ChevronRight size={16} />,
      disabled: !activeTableColumn || activeTableColumn.column >= activeTableColumn.map.width - 1,
      run: () => moveActiveTableColumn(editor, "right"),
    },
    {
      label: "Delete row",
      group: "row",
      icon: <Rows3 size={16} />,
      run: () => editor.chain().focus().deleteRow().run(),
    },
    {
      label: "Delete column",
      group: "column",
      icon: <Columns3 size={16} />,
      run: () => editor.chain().focus().deleteColumn().run(),
    },
    {
      label: "Merge or split cells",
      group: "table",
      icon: <TableCellsMerge size={16} />,
      run: () => editor.chain().focus().mergeOrSplit().run(),
    },
    {
      label: "Delete table",
      group: "table",
      icon: <Trash2 size={16} />,
      destructive: true,
      run: () => editor.chain().focus().deleteTable().run(),
    },
  ];

  const applyMobileHeading = (level: "" | 1 | 2 | 3 | 4 | 5 | 6) => {
    if (level) {
      editor.chain().focus().setHeading({ level }).run();
    } else {
      editor.chain().focus().setParagraph().run();
    }
    dismissMobileHeadingMenu(false);
  };

  const applyFontFamily = (fontFamily: string) => {
    if (fontFamily) {
      editor.chain().focus().setMark("fontFamily", { fontFamily }).run();
    } else {
      editor.chain().focus().unsetMark("fontFamily").run();
    }
    dismissFontFamilyMenu(false);
  };

  const claimEditorControlPointer = (event: { preventDefault: () => void }) => {
    event.preventDefault();
    if (!editor.isDestroyed) editor.view.focus();
  };

  const blockTransformOptions: { id: EditorBlockTransform; label: string; active: boolean }[] = [
    {
      id: "paragraph",
      label: "Text",
      active:
        !editor.isActive("heading") &&
        !editor.isActive("blockquote") &&
        !editor.isActive("codeBlock") &&
        !editor.isActive("bulletList") &&
        !editor.isActive("orderedList") &&
        !editor.isActive("taskList"),
    },
    { id: "heading-1", label: "Heading 1", active: editor.isActive("heading", { level: 1 }) },
    { id: "heading-2", label: "Heading 2", active: editor.isActive("heading", { level: 2 }) },
    { id: "heading-3", label: "Heading 3", active: editor.isActive("heading", { level: 3 }) },
    { id: "bullet", label: "Bullet list", active: editor.isActive("bulletList") },
    { id: "numbered", label: "Numbered list", active: editor.isActive("orderedList") },
    { id: "task", label: "Task list", active: editor.isActive("taskList") },
    { id: "quote", label: "Quote", active: editor.isActive("blockquote") },
    { id: "code", label: "Code block", active: editor.isActive("codeBlock") },
  ];

  const transformCurrentBlock = (target: EditorBlockTransform) => {
    applyEditorBlockTransform(editor, target);
    setBlockMenu((current) =>
      current ? { ...current, open: false, transformOpen: false } : current,
    );
  };

  const sectionControlActions = activeSection
    ? [
        {
          id: "insert-before" as const,
          label: `Add ${activeSection.kind === "columns" ? "column block" : "section"} above`,
          icon: <BetweenVerticalStart size={16} />,
          disabled: false,
        },
        {
          id: "move-up" as const,
          label: `Move ${activeSection.kind === "columns" ? "column block" : "section"} up`,
          icon: <ArrowUp size={16} />,
          disabled: activeSection.index === 0,
        },
        {
          id: "move-down" as const,
          label: `Move ${activeSection.kind === "columns" ? "column block" : "section"} down`,
          icon: <ArrowDown size={16} />,
          disabled: activeSection.index >= activeSection.siblingCount - 1,
        },
        {
          id: "insert-after" as const,
          label: `Add ${activeSection.kind === "columns" ? "column block" : "section"} below`,
          icon: <BetweenVerticalEnd size={16} />,
          disabled: false,
        },
      ]
    : [];

  return (
    <ContextMenu
      open={Boolean(contextMenu)}
      onOpenChange={(open) => {
        if (open) openEditorContextMenu();
        else setContextMenu(null);
      }}
    >
      <div
        data-starlane-editor-canvas="true"
        data-mobile-editor-toolbar={showMobileCommandButton ? "true" : undefined}
        className="starlane-editor-canvas"
        onPointerDown={(event) => {
          const target = event.target;
          if (!(target instanceof Element)) return;

          const targetInsideEditor = editor.view.dom.contains(target);
          const targetInsideChrome = Boolean(target.closest(EDITOR_CANVAS_CHROME_SELECTOR));
          const activeElement = document.activeElement;
          if (
            activeElement instanceof HTMLElement &&
            !editor.view.dom.contains(activeElement) &&
            !targetInsideChrome
          ) {
            // A sidebar button can retain :focus-visible when the pointer lands
            // on a non-focusable editor node. Release that external focus before
            // ProseMirror handles the click so the sidebar cannot stay highlighted.
            activeElement.blur();
          }
          if (
            !shouldClaimEditorCanvasPointer({
              readOnly,
              primaryButton: event.button === 0,
              targetInsideEditor,
              targetInsideChrome,
            })
          ) {
            return;
          }

          // The editor owns its surrounding writing canvas. Clicking blank page
          // space must move focus into ProseMirror instead of leaving the last
          // sidebar button armed for Space/Enter.
          event.preventDefault();
          const position = editor.view.posAtCoords({
            left: event.clientX,
            top: event.clientY,
          });
          editor.commands.focus(position?.pos ?? "end");
        }}
        onClick={(event) => {
          const target = event.target as HTMLElement;
          const wikiLink = target.closest<HTMLAnchorElement>(
            `a[href^="${WIKI_DOCUMENT_HREF_PREFIX}"]`,
          );
          const wikiDocumentId = wikiLink
            ? parseWikiDocumentHref(wikiLink.getAttribute("href") || "")
            : null;
          if (wikiDocumentId) {
            event.preventDefault();
            setLinkMenu(null);
            onDocumentNavigate?.(wikiDocumentId);
            return;
          }
          if (
            !readOnly &&
            editor.view.dom.contains(target) &&
            !target.closest(EDITOR_CANVAS_CHROME_SELECTOR) &&
            !editor.isFocused
          ) {
            // Some WebKit/browser paths leave the previously focused sidebar
            // button active after a ProseMirror click. Focus the view after its
            // click handlers choose the caret so the selection is preserved.
            editor.view.focus();
          }
        }}
        onPointerUp={(event) => {
          const target = event.target;
          if (
            readOnly ||
            event.button !== 0 ||
            !(target instanceof Element) ||
            !editor.view.dom.contains(target) ||
            target.closest(EDITOR_CANVAS_CHROME_SELECTOR)
          ) {
            return;
          }

          // Pointer-up runs after ProseMirror has chosen the caret. Reclaim focus
          // on the next frame so WebKit cannot leave the previously active
          // sidebar or toolbar control armed. Native pointer selection owns
          // scrolling here; forcing scrollIntoView makes keyboard-open taps jump.
          window.requestAnimationFrame(() => {
            if (!editor.isDestroyed) {
              if (!editor.isFocused) editor.view.focus();
            }
          });
        }}
        style={
          {
            "--editor-zoom": cssVar,
            fontSize: `calc(1em * var(--editor-zoom))`,
            paddingBottom: showMobileCommandButton
              ? mobileKeyboardOpen
                ? "3.75rem"
                : "calc(env(safe-area-inset-bottom) + 4.75rem + var(--starlane-mobile-statusbar-height, 0px))"
              : readOnly
                ? undefined
                : "4.75rem",
            scrollPaddingBottom: showMobileCommandButton
              ? mobileKeyboardOpen
                ? "3.75rem"
                : "calc(env(safe-area-inset-bottom) + 4.75rem + var(--starlane-mobile-statusbar-height, 0px))"
              : readOnly
                ? undefined
                : "4.75rem",
          } as React.CSSProperties
        }
      >
        <ContextMenuTrigger asChild>
          <div onContextMenu={(event) => prepareEditorContextMenu(event.nativeEvent)}>
            <EditorContent editor={editor} />
          </div>
        </ContextMenuTrigger>
        {blockAnnouncement.message ? (
          <p key={blockAnnouncement.id} className="sr-only" role="status" aria-live="polite">
            {blockAnnouncement.message}
          </p>
        ) : null}
        <EditorBlockGutter
          editor={editor}
          enabled={shouldShowEditorBlockGutter({
            showBlockChrome,
            mobileToolbar: showMobileCommandButton,
            readOnly,
          })}
          onInsertBelow={(target) => openInsertBlockPaletteForTarget(editor, target, "below")}
          onMoveComplete={announceBlockMove}
          onOpenMenu={openGutterBlockMenu}
        />
        {desktopToolbarHost && !readOnly && !showMobileCommandButton ? (
          <PersistentDocumentToolbar
            editor={editor}
            host={desktopToolbarHost}
            commentsAvailable={commentsAvailable}
            insertOpen={Boolean(slashState?.searchable)}
            onAction={runToolbarAction}
            onInsert={(trigger) => {
              commandMenuTriggerRef.current = trigger;
              openSearchableCommandPalette(editor, undefined, trigger);
            }}
          />
        ) : null}
        {selectionToolbarPosition &&
          !showMobileCommandButton &&
          !desktopToolbarHost &&
          !readOnly &&
          createPortal(
            <div
              ref={selectionToolbarRef}
              data-starlane-selection-toolbar="true"
              className={`${CONTROL_CHROME_RAIL_CLASS} rounded-button shadow-card fixed z-[70] h-10 p-1`}
              style={{
                top: selectionToolbarPosition.top,
                left: selectionToolbarPosition.left,
              }}
              role="toolbar"
              aria-label="Text formatting"
              onPointerDown={claimEditorControlPointer}
              onKeyDown={(event) => {
                if (event.key !== "Escape") return;
                event.preventDefault();
                setSelectionTypeMenuOpen(false);
                setSelectionToolbarPosition(null);
                editor.commands.focus();
              }}
            >
              <Popover open={selectionTypeMenuOpen} onOpenChange={setSelectionTypeMenuOpen}>
                <PopoverTrigger asChild>
                  <button
                    type="button"
                    className={`${CONTROL_CHROME_ACTION_CLASS} flex h-8 min-w-20 items-center gap-1 px-2 text-xs`}
                    aria-label="Change block type"
                    aria-haspopup="menu"
                    aria-expanded={selectionTypeMenuOpen}
                  >
                    <Type size={15} aria-hidden="true" />
                    <span>
                      {blockTransformOptions.find((option) => option.active)?.label ?? "Text"}
                    </span>
                    <ChevronRight
                      size={13}
                      className={selectionTypeMenuOpen ? "rotate-90" : undefined}
                      aria-hidden="true"
                    />
                  </button>
                </PopoverTrigger>
                <PopoverContent
                  ref={selectionTypeMenuRef}
                  align="start"
                  side="bottom"
                  sideOffset={6}
                  className="w-44 p-1"
                  role="menu"
                  aria-label="Block type"
                  onKeyDown={handleSelectionTypeMenuKeyDown}
                  onOpenAutoFocus={(event) => event.preventDefault()}
                  onCloseAutoFocus={(event) => {
                    event.preventDefault();
                    if (!editor.isDestroyed) editor.view.focus();
                  }}
                >
                  {blockTransformOptions.map((option) => (
                    <button
                      key={option.id}
                      type="button"
                      role="menuitemradio"
                      aria-checked={option.active}
                      className={`${CONTROL_CHROME_ACTION_CLASS} flex h-8 w-full items-center justify-between px-2 text-left text-xs`}
                      onPointerDown={claimEditorControlPointer}
                      onClick={() => {
                        applyEditorBlockTransform(editor, option.id);
                        setSelectionTypeMenuOpen(false);
                      }}
                    >
                      <span>{option.label}</span>
                      {option.active ? <Check size={14} aria-hidden="true" /> : null}
                    </button>
                  ))}
                </PopoverContent>
              </Popover>
              <span className="bg-border mx-1 h-5 w-px shrink-0" aria-hidden="true" />
              {[
                {
                  label: "Bold",
                  icon: <Bold size={16} aria-hidden="true" />,
                  active: editor.isActive("bold"),
                  run: () => editor.chain().focus().toggleBold().run(),
                },
                {
                  label: "Italic",
                  icon: <Italic size={16} aria-hidden="true" />,
                  active: editor.isActive("italic"),
                  run: () => editor.chain().focus().toggleItalic().run(),
                },
                {
                  label: "Strikethrough",
                  icon: <Strikethrough size={16} aria-hidden="true" />,
                  active: editor.isActive("strike"),
                  run: () => editor.chain().focus().toggleStrike().run(),
                },
                {
                  label: "Inline code",
                  icon: <Code size={16} aria-hidden="true" />,
                  active: editor.isActive("code"),
                  run: () => editor.chain().focus().toggleCode().run(),
                },
              ].map((action) => (
                <button
                  key={action.label}
                  type="button"
                  title={action.label}
                  aria-label={action.label}
                  aria-pressed={action.active}
                  className={`${action.active ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : CONTROL_CHROME_ACTION_CLASS} flex h-8 w-8 items-center justify-center`}
                  onClick={action.run}
                >
                  {action.icon}
                </button>
              ))}
              <button
                type="button"
                title="Edit link"
                aria-label="Edit link"
                aria-pressed={editor.isActive("link")}
                className={`${editor.isActive("link") ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : CONTROL_CHROME_ACTION_CLASS} flex h-8 w-8 items-center justify-center`}
                onClick={() => openSelectionLinkEditor(editor)}
              >
                <LinkIcon size={16} aria-hidden="true" />
              </button>
              <button
                type="button"
                title="Clear formatting"
                aria-label="Clear formatting"
                className={`${CONTROL_CHROME_ACTION_CLASS} flex h-8 w-8 items-center justify-center`}
                onClick={() => editor.chain().focus().unsetAllMarks().run()}
              >
                <Paintbrush size={16} aria-hidden="true" />
              </button>
              {commentsAvailable ? (
                <button
                  type="button"
                  title="Comment on selection"
                  aria-label="Comment on selection"
                  className={`${CONTROL_CHROME_ACTION_CLASS} flex h-8 w-8 items-center justify-center`}
                  onClick={() => openCommentDialog(editor)}
                >
                  <MessageSquare size={16} aria-hidden="true" />
                </button>
              ) : null}
            </div>,
            document.body,
          )}
        {tableToolbarPosition && !readOnly && !tableStyleDialogOpen && (
          <div
            data-starlane-table-toolbar="true"
            className={
              showMobileCommandButton
                ? `${MOBILE_TABLE_TOOLBAR_BASE_CLASS} ${
                    mobileKeyboardOpen
                      ? MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS
                      : MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS
                  }`
                : "fixed z-[60]"
            }
            style={
              showMobileCommandButton
                ? { bottom: mobileMenuBottom }
                : { top: tableToolbarPosition.top, left: tableToolbarPosition.left }
            }
            role="toolbar"
            aria-label="Table controls"
          >
            <div
              className={
                showMobileCommandButton
                  ? `${MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS} ${
                      mobileKeyboardOpen ? "rounded-input" : "rounded-button shadow-card"
                    }`
                  : TABLE_EDITOR_TOOLBAR_SURFACE_CLASS
              }
            >
              <TooltipProvider delayDuration={350}>
                {tableActions.map((action, index) => (
                  <Tooltip key={action.label}>
                    <TooltipTrigger asChild>
                      <button
                        type="button"
                        title={action.label}
                        aria-label={action.label}
                        disabled={action.disabled}
                        onPointerDown={claimEditorControlPointer}
                        onClick={() => {
                          action.run();
                          window.requestAnimationFrame(() => updateTableToolbar(editor));
                        }}
                        className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                          index > 0 && tableActions[index - 1]?.group !== action.group
                            ? "border-border ml-1 border-l"
                            : ""
                        } ${
                          action.destructive
                            ? "text-danger hover:bg-danger/10"
                            : "text-text-muted hover:bg-surface-raised hover:text-text"
                        } ${
                          action.disabled
                            ? "cursor-not-allowed opacity-40 hover:bg-transparent"
                            : ""
                        }`}
                      >
                        {action.icon}
                      </button>
                    </TooltipTrigger>
                    <TooltipContent side="top">{action.label}</TooltipContent>
                  </Tooltip>
                ))}
              </TooltipProvider>
            </div>
          </div>
        )}
        {tableStyleDialogOpen && !readOnly && (
          <TableStyleDialog
            initialStyle={activeTableStyle}
            initialPlacement={normalizeTablePlacement(editor.getAttributes("table").placement)}
            onClose={() => setTableStyleDialogOpen(false)}
            onApply={(style: TableStyleId, placement: TablePlacement) => {
              editor
                .chain()
                .focus()
                .updateAttributes("table", { tableStyle: style, placement })
                .run();
              setTableStyleDialogOpen(false);
              window.requestAnimationFrame(() => updateTableToolbar(editor));
            }}
          />
        )}
        {uploadWarning ? (
          <EditorUploadWarning message={uploadWarning} onDismiss={() => setUploadWarning(null)} />
        ) : null}
        {slashState && (
          <SlashCommandPalette
            query={slashState.query}
            position={slashState.position}
            searchable={slashState.searchable}
            templates={templates}
            templateFields={templateFields}
            templateAuthoring={templateAuthoring}
            templateContext={templateContext}
            onSelect={applySlashCommand}
            onClose={() => setSlash(null)}
            triggerRef={slashPaletteTriggerRef}
          />
        )}
        {templateInsertionOverlay?.kind === "schema-loading" && renderTemplateInsertionPortal(
          <DialogShell
            labelledBy="template-schema-loading-title"
            describedBy="template-schema-loading-description"
            maxWidth="sm"
            onClose={clearTemplateInsertionOverlay}
          >
            <DialogHeader>
              <div>
                <DialogTitle id="template-schema-loading-title">Loading reusable sections</DialogTitle>
                <DialogDescription id="template-schema-loading-description">
                  Checking this space’s properties before resolving section fields.
                </DialogDescription>
              </div>
            </DialogHeader>
            <DialogBody>
              <p role="status" className="text-sm text-text-muted">Loading property choices…</p>
            </DialogBody>
            <DialogFooter>
              <Button type="button" variant="secondary" onClick={clearTemplateInsertionOverlay}>
                Cancel
              </Button>
            </DialogFooter>
          </DialogShell>,
        )}
        {templateInsertionOverlay?.kind === "schema-error" && renderTemplateInsertionPortal(
          <DialogShell
            labelledBy="template-schema-error-title"
            describedBy="template-schema-error-description"
            maxWidth="sm"
            onClose={clearTemplateInsertionOverlay}
          >
            <DialogHeader>
              <div>
                <DialogTitle id="template-schema-error-title">Property choices unavailable</DialogTitle>
                <DialogDescription id="template-schema-error-description">
                  This section can’t be resolved until the current space’s property choices load.
                </DialogDescription>
              </div>
            </DialogHeader>
            <DialogBody>
              <p role="alert" className="text-sm text-danger">Retry loading property choices to continue.</p>
            </DialogBody>
            <DialogFooter>
              <Button type="button" variant="secondary" onClick={clearTemplateInsertionOverlay}>
                Cancel
              </Button>
              <Button type="button" onClick={retryTemplateSchemaFlow}>Retry</Button>
            </DialogFooter>
          </DialogShell>,
        )}
        {templateInsertionOverlay?.kind === "picker" && renderTemplateInsertionPortal(
          <Suspense fallback={null}>
            <TemplatePickerDialog
              operation="insert"
              templates={templates}
              schemas={templateInsertionOverlay.schemas}
              destination={templateContext.title || "Current page"}
              renderContext={templateContext}
              allowBlank={false}
              onClose={clearTemplateInsertionOverlay}
              onSelect={handleTemplatePickerSelect}
            />
          </Suspense>,
        )}
        {templateInsertionOverlay?.kind === "section" && renderTemplateInsertionPortal(
          <Suspense fallback={null}>
            <EditorTemplateSectionDialog
              template={templateInsertionOverlay.template}
              renderContext={templateContext}
              currentPageLabel={templateContext.title || "this page"}
              schemas={templateInsertionOverlay.schemas}
              onInsert={insertTemplateMarkdown}
              onClose={clearTemplateInsertionOverlay}
            />
          </Suspense>,
        )}
        {templateInsertionOverlay?.kind === "guidance" && renderTemplateInsertionPortal(
          <Suspense fallback={null}>
            <EditorTemplateGuidanceDialog
              content={templateInsertionOverlay.content}
              onChange={(content) => setTemplateInsertionOverlay({ kind: "guidance", content })}
              onCancel={clearTemplateInsertionOverlay}
              onSubmit={(text) => {
                const markerId = globalThis.crypto?.randomUUID?.()
                  ?? `guidance_${Date.now().toString(36)}_${Math.random().toString(36).slice(2)}`;
                return insertTemplateMarkdown(serializeTemplateMarker({ kind: "guidance", id: markerId, text }));
              }}
            />
          </Suspense>,
        )}
        {commentDialog && (
          <EditorCommentDialog
            content={commentContent}
            error={commentError}
            saving={commentSaving}
            target={commentDialog}
            onCancel={closeCommentDialog}
            onChange={setCommentContent}
            onSubmit={() => void submitComment()}
          />
        )}
        {imageAttachmentHistory && imageAttachmentHistory.editor===editor && imageAttachmentHistory.spaceId===spaceId && imageAttachmentHistory.docId===docId && imageAttachmentHistory.draftId===attachmentDraftId ? (
          <AttachmentHistoryDialog
            spaceId={imageAttachmentHistory.spaceId}
            documentId={imageAttachmentHistory.docId} draftId={imageAttachmentHistory.draftId}
            versionId={imageAttachmentHistory.versionId}
            replacementKind="image"
            verifySelection={imageAttachmentHistory.verify}
            onReplaced={!readOnly && imageAttachmentHistory.draftId ? async result=>{
              imageAttachmentHistory.verify();
              if(!imageAttachmentHistory.editor.isEditable||!result.version.mime_type.startsWith("image/"))throw new Error("Original image replacement cannot be adopted in this draft.");
              const preview=await dataClient.previewPinnedDocumentAttachmentVersionImage(imageAttachmentHistory.spaceId,imageAttachmentHistory.docId,result.version.version_id);
              try{imageAttachmentHistory.verify();if(!imageAttachmentHistory.editor.isEditable)throw new Error("Original image draft is no longer editable.");imageAttachmentHistory.replace(ownAttachmentImageUrl(preview),result.version.version_id);setImageAttachmentHistory(current=>current===imageAttachmentHistory?null:current);}
              catch(error){releaseBrowserAttachmentImage(preview);throw error;}
            }:undefined}
            onClose={()=>setImageAttachmentHistory(null)}
          />
        ):null}
        {uploadMenu && !readOnly && (
          <EditorUploadDialog
            triggerRef={slashPaletteTriggerRef}
            retainedContext={getCurrentLaunchMode()!=="local" && assetContext.space_id && assetContext.document_id ? {spaceId:assetContext.space_id,documentId:assetContext.document_id,draftId:attachmentDraftId,referencedVersionIds:()=>{
              captureAssetInsertion()();
              const current=editorInstanceRef.current, ids=new Set<string>();if(!current||current.isDestroyed||activeAttachmentPage.current.space_id!==assetContext.space_id||activeAttachmentPage.current.document_id!==assetContext.document_id)return ids;
              const cover=coverAttachmentRef.current;if(cover && /^starlane-attachment:\/\/[0-9a-f-]{36}$/.test(cover))ids.add(cover.slice("starlane-attachment://".length));
              current.state.doc.descendants(node=>{if((node.type.name==="image"||node.type.name==="fileAttachment")&&typeof node.attrs.attachmentId==="string")ids.add(node.attrs.attachmentId);});return ids;
            },onUse:async(attachment)=>{
              const verify = captureAssetInsertion();
              verify();
              if(fileReplacementRef.current||imageReplacementRef.current)throw new Error("Open Insert file to use the original retained upload; replacement targets cannot adopt it.");
              const currentEditor=editorInstanceRef.current;if(!currentEditor||currentEditor.isDestroyed||!currentEditor.isEditable||activeAttachmentPage.current.space_id!==assetContext.space_id||activeAttachmentPage.current.document_id!==assetContext.document_id)throw new Error("Original upload page changed.");
              let existing=false;currentEditor.state.doc.descendants(node=>{if(node.attrs.attachmentId===attachment.id)existing=true;});
              if(!existing)await handleExistingAsset(attachment,true);
              verify();
            }} : undefined}
            maxFileMb={maxPickedFileMb}
            existingAssets={existingAssets}
            existingLoading={existingAssetsLoading}
            existingError={existingAssetsError}
            onFilesSelected={handlePickedFiles}
            onSelectExisting={(asset) => void handleExistingAsset(asset).catch(error=>setExistingAssetsError(error instanceof Error?error.message:"Original asset could not be inserted."))}
            onRefreshExisting={() => void refreshExistingAssets()}
            onCancel={() => {
              fileReplacementRef.current = null;
              setUploadMenu(null);
            }}
          />
        )}
        {tagDialog && !readOnly && (
          <EditorTagDialog
            value={tagDialog.text}
            normalizedTag={normalizeTagName(tagDialog.text)}
            onChange={(text) =>
              setTagDialog((current) => (current ? { ...current, text } : current))
            }
            onCancel={() => setTagDialog(null)}
            onSubmit={applyTagDialog}
          />
        )}
        {linkMenu && (
          <>
            <button
              type="button"
              aria-label="Close link menu"
              className="fixed inset-0 z-[65] cursor-default"
              onMouseDown={(event) => event.preventDefault()}
              onClick={closeAndRestoreLinkMenu}
            />
            <div
              ref={linkMenuRef}
              className="starlane-menu-surface fixed z-[70] max-w-[calc(100vw-1rem)] p-1"
              style={{ top: linkMenu.position.top, left: linkMenu.position.left }}
              role="group"
              aria-label={readOnly ? "Link details" : "Link editor"}
              tabIndex={-1}
            >
              {readOnly ? (
                <div className="flex items-center gap-1">
                  <span className="text-text-muted max-w-72 truncate px-2 text-xs">
                    {linkMenu.href}
                  </span>
                  <button
                    type="button"
                    title="Open link"
                    aria-label="Open link"
                    onClick={() => {
                      const href = normalizeEditorLinkHref(linkMenu.href);
                      if (href) window.open(href, "_blank", "noopener,noreferrer");
                      closeAndRestoreLinkMenu();
                    }}
                    className={`${CONTROL_CHROME_ACTION_CLASS} flex h-8 w-8 items-center justify-center`}
                  >
                    <ExternalLink size={15} aria-hidden="true" />
                  </button>
                </div>
              ) : (
                <form
                  className="flex w-[min(24rem,calc(100vw-1.5rem))] items-center gap-1"
                  onSubmit={(event) => {
                    event.preventDefault();
                    applyLinkMenu();
                  }}
                >
                  <Input
                    ref={linkMenuInputRef}
                    value={linkMenu.draftHref}
                    onChange={(event) =>
                      setLinkMenu((current) =>
                        current ? { ...current, draftHref: event.target.value } : current,
                      )
                    }
                    className="bg-surface h-8 min-w-0 flex-1 text-xs"
                    aria-label="Link URL"
                    aria-invalid={
                      Boolean(linkMenu.draftHref.trim()) &&
                      !normalizeEditorLinkHref(linkMenu.draftHref)
                    }
                    aria-describedby={
                      Boolean(linkMenu.draftHref.trim()) &&
                      !normalizeEditorLinkHref(linkMenu.draftHref)
                        ? linkMenuErrorId
                        : undefined
                    }
                    placeholder="https://example.com"
                    autoComplete="off"
                  />
                  {Boolean(linkMenu.draftHref.trim()) &&
                  !normalizeEditorLinkHref(linkMenu.draftHref) ? (
                    <span id={linkMenuErrorId} className="sr-only" role="alert">
                      Use an HTTPS, HTTP, email, telephone, Starlane, or relative link.
                    </span>
                  ) : null}
                  <button
                    type="button"
                    title="Open link"
                    aria-label="Open link"
                    disabled={!normalizeEditorLinkHref(linkMenu.draftHref)}
                    onClick={() => {
                      const href = normalizeEditorLinkHref(linkMenu.draftHref);
                      if (href) window.open(href, "_blank", "noopener,noreferrer");
                    }}
                    className={`${CONTROL_CHROME_ACTION_CLASS} flex h-8 w-8 items-center justify-center`}
                  >
                    <ExternalLink size={15} aria-hidden="true" />
                  </button>
                  {linkMenu.href ? (
                    <button
                      type="button"
                      title="Remove link"
                      aria-label="Remove link"
                      onClick={() => removeLink(linkMenu)}
                      className={`${CONTROL_CHROME_ACTION_CLASS} flex h-8 w-8 items-center justify-center`}
                    >
                      <Unlink size={15} aria-hidden="true" />
                    </button>
                  ) : null}
                  <button
                    type="submit"
                    title={linkMenu.href ? "Update link" : "Add link"}
                    aria-label={linkMenu.href ? "Update link" : "Add link"}
                    disabled={!normalizeEditorLinkHref(linkMenu.draftHref)}
                    className={`${CONTROL_CHROME_ACTION_CLASS} h-8 px-2 text-xs font-medium`}
                  >
                    {linkMenu.href ? "Update" : "Add"}
                  </button>
                </form>
              )}
            </div>
          </>
        )}
        {linkDialog && (
          <EditorLinkDialog
            text={linkDialog.text}
            href={linkDialog.href}
            onTextChange={(text) =>
              setLinkDialog((current) => (current ? { ...current, text } : current))
            }
            onHrefChange={(href) =>
              setLinkDialog((current) => (current ? { ...current, href } : current))
            }
            onCancel={() => setLinkDialog(null)}
            onSubmit={applyLinkDialog}
          />
        )}
        {bookmarkDialog && (
          <EditorBookmarkDialog
            href={bookmarkDialog.href}
            onHrefChange={(href) =>
              setBookmarkDialog((current) =>
                current
                  ? {
                      ...current,
                      href,
                      title: current.title || getBookmarkDomain(normalizeEditorLinkHref(href)),
                    }
                  : current,
              )
            }
            onCancel={() => setBookmarkDialog(null)}
            onSubmit={applyBookmarkDialog}
          />
        )}
        {wikiLinkDialog && (
          <EditorWikiLinkDialog
            documents={wikiLinkDialog.documents}
            currentDocumentId={docId}
            query={wikiLinkDialog.query}
            loading={wikiLinkDialog.loading}
            error={wikiLinkDialog.error}
            onQueryChange={(query) =>
              setWikiLinkDialog((current) => (current ? { ...current, query } : current))
            }
            onSelect={(document: EditorWikiLinkDocument) => insertWikiLink(document)}
            onRetry={loadWikiLinkDocuments}
            onCancel={() => setWikiLinkDialog(null)}
          />
        )}
        {mentionDialog && (
          <EditorMentionDialog
            members={mentionDialog.members}
            query={mentionDialog.query}
            loading={mentionDialog.loading}
            error={mentionDialog.error}
            onQueryChange={(query) =>
              setMentionDialog((current) => (current ? { ...current, query } : current))
            }
            onSelect={(member: EditorMentionMember) => insertMention(member)}
            onCancel={() => setMentionDialog(null)}
          />
        )}
        {utilityTargetDialog && (
          <EditorUtilityTargetDialog
            kind={utilityTargetDialog.kind}
            targets={utilityTargetDialog.targets}
            query={utilityTargetDialog.query}
            loading={utilityTargetDialog.loading}
            error={utilityTargetDialog.error}
            onQueryChange={(query) =>
              setUtilityTargetDialog((current) => (current ? { ...current, query } : current))
            }
            onSelect={insertUtilityTarget}
            onCancel={() => setUtilityTargetDialog(null)}
          />
        )}
        {!readOnly && showMobileCommandButton && (
          <div
            data-starlane-editor-toolbar="true"
            className={`${MOBILE_EDITOR_TOOLBAR_BASE_CLASS} ${
              mobileKeyboardOpen
                ? MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS
                : MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS
            }`}
            style={{ bottom: mobileToolbarBottom }}
            role="toolbar"
            aria-label="Editor toolbar"
          >
            <div
              className={`${MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS} ${
                mobileKeyboardOpen ? "rounded-input" : "rounded-button shadow-card"
              }`}
            >
              <TooltipProvider delayDuration={350}>
                {mobileToolbarActions.map((action) => {
                  if (action === "blockControls") {
                    const currentBlock = getCurrentTopLevelBlock(editor);
                    const blockControlsAvailable =
                      Boolean(
                        currentBlock &&
                        currentBlock.node.type.name !== "section" &&
                        currentBlock.node.type.name !== "columns",
                      ) || Boolean(blockMenu?.target);
                    return blockControlsAvailable ? (
                      <Tooltip key={action}>
                        <TooltipTrigger asChild>
                          <button
                            type="button"
                            title="Block controls"
                            aria-label="Block controls"
                            aria-haspopup="menu"
                            aria-expanded={Boolean(blockMenu?.open)}
                            onPointerDown={claimEditorControlPointer}
                            onClick={(event) => runToolbarAction(action, event.currentTarget)}
                            className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                              blockMenu?.open
                                ? CONTROL_CHROME_ACTIVE_ACTION_CLASS
                                : `${getEditorToolbarActionTone(action)} active:bg-surface-raised hover:bg-surface-raised hover:text-text`
                            }`}
                          >
                            <MoreHorizontal size={18} />
                          </button>
                        </TooltipTrigger>
                        <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                          Block controls
                        </TooltipContent>
                      </Tooltip>
                    ) : null;
                  }
                  if (action === "sectionControls") {
                    const sectionControlsLabel =
                      activeSection?.kind === "columns"
                        ? "Column block controls"
                        : "Section controls";
                    return (
                      <Tooltip key={action}>
                        <Popover open={sectionControlsOpen} onOpenChange={setSectionControlsOpen}>
                          <PopoverTrigger asChild>
                            <button
                              ref={sectionControlsTriggerRef}
                              type="button"
                              title={sectionControlsLabel}
                              aria-label={sectionControlsLabel}
                              aria-haspopup="menu"
                              aria-expanded={sectionControlsOpen}
                              onPointerDown={claimEditorControlPointer}
                              className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                                sectionControlsOpen
                                  ? CONTROL_CHROME_ACTIVE_ACTION_CLASS
                                  : `${getEditorToolbarActionTone(action)} active:bg-surface-raised hover:bg-surface-raised hover:text-text`
                              }`}
                            >
                              {activeSection?.kind === "columns" ? (
                                <Columns2 size={18} />
                              ) : (
                                <Rows3 size={18} />
                              )}
                            </button>
                          </PopoverTrigger>
                          {activeSection && (
                            <PopoverContent
                              side={showMobileCommandButton ? "top" : "bottom"}
                              align="end"
                              sideOffset={6}
                              className="max-h-[min(34rem,calc(100dvh-6rem))] w-64 overflow-y-auto p-1"
                              role="menu"
                              aria-label={sectionControlsLabel}
                            >
                              <div
                                ref={sectionControlsMenuRef}
                                tabIndex={-1}
                                onKeyDown={handleSectionControlsMenuKeyDown}
                              >
                                {sectionControlActions.map((sectionAction) => (
                                  <button
                                    key={sectionAction.id}
                                    type="button"
                                    role="menuitem"
                                    disabled={sectionAction.disabled}
                                    onMouseDown={claimEditorControlPointer}
                                    onClick={() => runSectionControl(sectionAction.id)}
                                    className="rounded-button text-text-muted hover:bg-surface hover:text-text flex min-h-11 w-full items-center gap-3 px-3 py-2 text-left text-sm disabled:cursor-not-allowed disabled:opacity-40"
                                  >
                                    {sectionAction.icon}
                                    <span>{sectionAction.label}</span>
                                  </button>
                                ))}
                                <div className="bg-border my-1 h-px" />
                                <p className="text-text-muted px-3 pt-2 pb-1 text-xs font-medium">
                                  Layout
                                </p>
                                {SECTION_LAYOUT_OPTIONS.filter(
                                  (option) =>
                                    activeSection.kind !== "columns" || option.id !== "single",
                                ).map((option) => (
                                  <button
                                    key={option.id}
                                    type="button"
                                    role="menuitemradio"
                                    aria-checked={activeSection.layout === option.id}
                                    onMouseDown={claimEditorControlPointer}
                                    onClick={() => chooseActiveSectionLayout(option.id)}
                                    className={`rounded-button flex min-h-11 w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm ${
                                      activeSection.layout === option.id
                                        ? "bg-primary/15 text-primary"
                                        : "text-text-muted hover:bg-surface hover:text-text"
                                    }`}
                                  >
                                    <span>{option.label}</span>
                                    {activeSection.layout === option.id && <Check size={13} />}
                                  </button>
                                ))}
                                {activeSection.kind === "section" && (
                                  <>
                                    <div className="bg-border my-1 h-px" />
                                    <p className="text-text-muted px-3 pt-2 pb-1 text-xs font-medium">
                                      Appearance
                                    </p>
                                    <label className="text-text-muted flex min-h-11 items-center justify-between gap-3 px-3 py-2 text-sm">
                                      Border color
                                      <input
                                        type="color"
                                        value={activeSection.style.borderColor ?? "#334155"}
                                        aria-label="Section border color"
                                        onChange={(event) =>
                                          updateActiveSectionStyle({
                                            borderColor: event.target.value,
                                          })
                                        }
                                      />
                                    </label>
                                  </>
                                )}
                                <label className="text-text-muted flex min-h-11 items-center justify-between gap-3 px-3 py-2 text-sm">
                                  Content background
                                  <input
                                    type="color"
                                    value={
                                      activeSection.style.contentBackgroundColor ??
                                      activeSection.style.backgroundColor ??
                                      "#111827"
                                    }
                                    aria-label="Section content background color"
                                    onChange={(event) =>
                                      updateActiveSectionStyle({
                                        backgroundColor: event.target.value,
                                        contentBackgroundColor: event.target.value,
                                      })
                                    }
                                  />
                                </label>
                                <label className="text-text-muted flex min-h-11 items-center justify-between gap-3 px-3 py-2 text-sm">
                                  Header background
                                  <input
                                    type="color"
                                    value={activeSection.style.headerBackgroundColor ?? "#1f2937"}
                                    aria-label="Section header background color"
                                    onChange={(event) =>
                                      updateActiveSectionStyle({
                                        headerBackgroundColor: event.target.value,
                                      })
                                    }
                                  />
                                </label>
                                <label className="grid gap-1 px-3 py-2 text-sm">
                                  <span className="text-text-muted">Header label</span>
                                  <Input
                                    value={activeSection.style.headerText ?? ""}
                                    placeholder="Optional section header"
                                    aria-label="Section header label"
                                    className="bg-bg h-9 px-2 text-xs"
                                    onChange={(event) =>
                                      updateActiveSectionStyle({
                                        headerText: event.target.value,
                                      })
                                    }
                                  />
                                </label>
                                <div className="bg-border my-1 h-px" />
                                <button
                                  type="button"
                                  role="menuitem"
                                  onMouseDown={claimEditorControlPointer}
                                  onClick={() => runSectionControl("delete")}
                                  className="rounded-button text-danger hover:bg-surface flex min-h-11 w-full items-center gap-3 px-3 py-2 text-left text-sm"
                                >
                                  <Trash2 size={16} />
                                  <span>
                                    Delete{" "}
                                    {activeSection.kind === "columns" ? "column block" : "section"}
                                  </span>
                                </button>
                              </div>
                            </PopoverContent>
                          )}
                        </Popover>
                        <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                          {sectionControlsLabel}
                        </TooltipContent>
                      </Tooltip>
                    );
                  }
                  const meta = getMobileToolbarAction(action, editor);
                  if (action === "alignment") {
                    return (
                      <Tooltip key={action}>
                        <AnchoredAlignmentMenu
                          open={alignmentMenuOpen}
                          onOpenChange={setAlignmentMenuOpen}
                          onSelect={applyTextAlignment}
                        >
                          <TooltipTrigger asChild>
                            <button
                              ref={alignmentMenuTriggerRef}
                              type="button"
                              title={meta.label}
                              aria-label={meta.label}
                              aria-haspopup="menu"
                              aria-expanded={alignmentMenuOpen}
                              onPointerDown={claimEditorControlPointer}
                              className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                                meta.active
                                  ? CONTROL_CHROME_ACTIVE_ACTION_CLASS
                                  : `${getEditorToolbarActionTone(action)} active:bg-surface-raised hover:bg-surface-raised hover:text-text`
                              }`}
                            >
                              {meta.icon}
                            </button>
                          </TooltipTrigger>
                        </AnchoredAlignmentMenu>
                        <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                          {meta.label}
                        </TooltipContent>
                      </Tooltip>
                    );
                  }
                  if (action === "emoji") {
                    return (
                      <Tooltip key={action}>
                        <AnchoredEmojiMenu
                          open={emojiMenuOpen}
                          onOpenChange={setEmojiMenuOpen}
                          onSelect={(emoji) => {
                            editor.chain().focus().insertContent(emoji).run();
                            setEmojiMenuOpen(false);
                          }}
                        >
                          <TooltipTrigger asChild>
                            <button
                              ref={emojiMenuTriggerRef}
                              type="button"
                              title={meta.label}
                              aria-label={meta.label}
                              aria-haspopup="menu"
                              aria-expanded={emojiMenuOpen}
                              onPointerDown={claimEditorControlPointer}
                              className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                                meta.active
                                  ? CONTROL_CHROME_ACTIVE_ACTION_CLASS
                                  : `${getEditorToolbarActionTone(action)} active:bg-surface-raised hover:bg-surface-raised hover:text-text`
                              }`}
                            >
                              {meta.icon}
                            </button>
                          </TooltipTrigger>
                        </AnchoredEmojiMenu>
                        <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                          {meta.label}
                        </TooltipContent>
                      </Tooltip>
                    );
                  }
                  if (action === "heading") {
                    return (
                      <Tooltip key={action}>
                        <Popover
                          open={mobileHeadingMenuOpen}
                          onOpenChange={setMobileHeadingMenuOpen}
                        >
                          <PopoverTrigger asChild>
                            <button
                              ref={mobileHeadingMenuTriggerRef}
                              type="button"
                              title={meta.label}
                              aria-label={meta.label}
                              aria-haspopup="menu"
                              aria-expanded={mobileHeadingMenuOpen}
                              onPointerDown={claimEditorControlPointer}
                              className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                                meta.active
                                  ? CONTROL_CHROME_ACTIVE_ACTION_CLASS
                                  : `${getEditorToolbarActionTone(action)} active:bg-surface-raised hover:bg-surface-raised hover:text-text`
                              }`}
                            >
                              {meta.icon}
                            </button>
                          </PopoverTrigger>
                          <PopoverContent
                            side={showMobileCommandButton ? "top" : "bottom"}
                            align="end"
                            sideOffset={6}
                            className="w-44 p-1"
                            role="menu"
                            aria-label="Heading level"
                          >
                            <div
                              ref={mobileHeadingMenuRef}
                              tabIndex={-1}
                              onKeyDown={handleMobileHeadingMenuKeyDown}
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
                                  onMouseDown={claimEditorControlPointer}
                                  onClick={() =>
                                    applyMobileHeading(option.value as "" | 1 | 2 | 3 | 4 | 5 | 6)
                                  }
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
                          </PopoverContent>
                        </Popover>
                        <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                          {meta.label}
                        </TooltipContent>
                      </Tooltip>
                    );
                  }
                  if (action === "color") {
                    return (
                      <Tooltip key={action}>
                        <Popover open={textColorMenuOpen} onOpenChange={setTextColorMenuOpen}>
                          <PopoverTrigger asChild>
                            <button
                              ref={textColorMenuTriggerRef}
                              type="button"
                              title={meta.label}
                              aria-label={meta.label}
                              aria-haspopup="menu"
                              aria-expanded={textColorMenuOpen}
                              onPointerDown={claimEditorControlPointer}
                              className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                                meta.active
                                  ? CONTROL_CHROME_ACTIVE_ACTION_CLASS
                                  : `${getEditorToolbarActionTone(action)} active:bg-surface-raised hover:bg-surface-raised hover:text-text`
                              }`}
                            >
                              {meta.icon}
                            </button>
                          </PopoverTrigger>
                          <PopoverContent
                            side={showMobileCommandButton ? "top" : "bottom"}
                            align="end"
                            sideOffset={6}
                            className="w-56 p-2"
                            role="menu"
                            aria-label="Text color"
                          >
                            <div
                              ref={textColorMenuRef}
                              tabIndex={-1}
                              onKeyDown={handleTextColorMenuKeyDown}
                            >
                              <p className="text-text-muted px-2 pb-2 text-xs font-medium">
                                Text color
                              </p>
                              <div className="grid grid-cols-5 gap-1">
                                {EDITOR_TEXT_COLOR_OPTIONS.map((option) => (
                                  <button
                                    key={option.label}
                                    type="button"
                                    role="menuitemradio"
                                    aria-label={option.label}
                                    aria-checked={
                                      option.value ===
                                      (editor.getAttributes("textColor").color || null)
                                    }
                                    onMouseDown={claimEditorControlPointer}
                                    onClick={() => {
                                      if (option.value) {
                                        editor
                                          .chain()
                                          .focus()
                                          .setMark("textColor", { color: option.value })
                                          .run();
                                      } else {
                                        editor.chain().focus().unsetMark("textColor").run();
                                      }
                                      dismissTextColorMenu(false);
                                    }}
                                    className="focus-ring rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text flex size-9 items-center justify-center border text-xs"
                                  >
                                    <span
                                      aria-hidden="true"
                                      className="border-border size-5 rounded-full border"
                                      style={{ backgroundColor: option.value ?? "var(--text)" }}
                                    >
                                      {option.value === null ? "A" : null}
                                    </span>
                                  </button>
                                ))}
                              </div>
                            </div>
                          </PopoverContent>
                        </Popover>
                        <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                          {meta.label}
                        </TooltipContent>
                      </Tooltip>
                    );
                  }
                  if (action === "highlight") {
                    return (
                      <Tooltip key={action}>
                        <Popover
                          open={highlightColorMenuOpen}
                          onOpenChange={setHighlightColorMenuOpen}
                        >
                          <PopoverTrigger asChild>
                            <button
                              ref={highlightColorMenuTriggerRef}
                              type="button"
                              title={meta.label}
                              aria-label={meta.label}
                              aria-haspopup="menu"
                              aria-expanded={highlightColorMenuOpen}
                              onPointerDown={claimEditorControlPointer}
                              className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                                meta.active
                                  ? CONTROL_CHROME_ACTIVE_ACTION_CLASS
                                  : `${getEditorToolbarActionTone(action)} active:bg-surface-raised hover:bg-surface-raised hover:text-text`
                              }`}
                            >
                              {meta.icon}
                            </button>
                          </PopoverTrigger>
                          <PopoverContent
                            side={showMobileCommandButton ? "top" : "bottom"}
                            align="end"
                            sideOffset={6}
                            className="w-56 p-2"
                            role="menu"
                            aria-label="Highlight color"
                          >
                            <div
                              ref={highlightColorMenuRef}
                              tabIndex={-1}
                              onKeyDown={handleHighlightColorMenuKeyDown}
                            >
                              <p className="text-text-muted px-2 pb-2 text-xs font-medium">
                                Highlight color
                              </p>
                              <div className="grid grid-cols-4 gap-1">
                                {EDITOR_HIGHLIGHT_COLOR_OPTIONS.map((option) => (
                                  <button
                                    key={option.label}
                                    type="button"
                                    role="menuitemradio"
                                    aria-label={option.label}
                                    aria-checked={
                                      option.value ===
                                      (editor.getAttributes("highlight").color || null)
                                    }
                                    onMouseDown={claimEditorControlPointer}
                                    onClick={() => {
                                      if (option.value) {
                                        editor
                                          .chain()
                                          .focus()
                                          .setHighlight({ color: option.value })
                                          .run();
                                      } else {
                                        editor.chain().focus().unsetHighlight().run();
                                      }
                                      dismissHighlightColorMenu(false);
                                    }}
                                    className="focus-ring rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text flex size-9 items-center justify-center border text-xs"
                                  >
                                    <span
                                      aria-hidden="true"
                                      className="border-border size-5 rounded-full border"
                                      style={{ backgroundColor: option.value ?? "transparent" }}
                                    >
                                      {option.value === null ? "×" : null}
                                    </span>
                                  </button>
                                ))}
                              </div>
                            </div>
                          </PopoverContent>
                        </Popover>
                        <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                          {meta.label}
                        </TooltipContent>
                      </Tooltip>
                    );
                  }
                  if (action === "fontSize") {
                    return (
                      <Tooltip key={action}>
                        <Popover open={fontSizeMenuOpen} onOpenChange={setFontSizeMenuOpen}>
                          <PopoverTrigger asChild>
                            <button
                              ref={fontSizeMenuTriggerRef}
                              type="button"
                              title={meta.label}
                              aria-label={meta.label}
                              aria-haspopup="menu"
                              aria-expanded={fontSizeMenuOpen}
                              onPointerDown={claimEditorControlPointer}
                              className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                                meta.active
                                  ? CONTROL_CHROME_ACTIVE_ACTION_CLASS
                                  : `${getEditorToolbarActionTone(action)} active:bg-surface-raised hover:bg-surface-raised hover:text-text`
                              }`}
                            >
                              {meta.icon}
                            </button>
                          </PopoverTrigger>
                          <PopoverContent
                            side={showMobileCommandButton ? "top" : "bottom"}
                            align="end"
                            sideOffset={6}
                            className="w-48 p-1"
                            role="menu"
                            aria-label="Font size"
                          >
                            <div
                              ref={fontSizeMenuRef}
                              tabIndex={-1}
                              onKeyDown={handleFontSizeMenuKeyDown}
                            >
                              <p className="text-text-muted px-3 py-2 text-xs font-medium">
                                Font size
                              </p>
                              {EDITOR_FONT_SIZE_OPTIONS.map((size) => (
                                <button
                                  key={size}
                                  type="button"
                                  role="menuitemradio"
                                  aria-checked={editor.getAttributes("fontSize").fontSize === size}
                                  onMouseDown={claimEditorControlPointer}
                                  onClick={() => {
                                    editor
                                      .chain()
                                      .focus()
                                      .setMark("fontSize", { fontSize: size })
                                      .run();
                                    dismissFontSizeMenu(false);
                                  }}
                                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center justify-between px-3 py-2 text-left text-sm"
                                >
                                  <span>{size}</span>
                                  {editor.getAttributes("fontSize").fontSize === size ? (
                                    <Check size={13} />
                                  ) : null}
                                </button>
                              ))}
                            </div>
                          </PopoverContent>
                        </Popover>
                        <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                          {meta.label}
                        </TooltipContent>
                      </Tooltip>
                    );
                  }
                  if (action === "font") {
                    return (
                      <Tooltip key={action}>
                        <Popover open={fontFamilyMenuOpen} onOpenChange={setFontFamilyMenuOpen}>
                          <PopoverTrigger asChild>
                            <button
                              ref={fontFamilyMenuTriggerRef}
                              type="button"
                              title={meta.label}
                              aria-label={meta.label}
                              aria-haspopup="menu"
                              aria-expanded={fontFamilyMenuOpen}
                              onPointerDown={claimEditorControlPointer}
                              className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                                meta.active
                                  ? CONTROL_CHROME_ACTIVE_ACTION_CLASS
                                  : `${getEditorToolbarActionTone(action)} active:bg-surface-raised hover:bg-surface-raised hover:text-text`
                              }`}
                            >
                              {meta.icon}
                            </button>
                          </PopoverTrigger>
                          <PopoverContent
                            side={showMobileCommandButton ? "top" : "bottom"}
                            align="end"
                            sideOffset={6}
                            className="max-h-[min(32rem,calc(100dvh-6rem))] w-72 overflow-hidden p-0"
                            role="menu"
                            aria-label="Font family"
                          >
                            <div
                              ref={fontFamilyMenuRef}
                              tabIndex={-1}
                              onKeyDown={handleFontFamilyMenuKeyDown}
                              className="flex max-h-[min(32rem,calc(100dvh-6rem))] flex-col"
                            >
                              <div className="border-border border-b px-3 py-2.5">
                                <p className="text-text text-sm font-medium">Font family</p>
                                <p className="text-text-muted mt-0.5 text-xs">
                                  {`${getEditorFontFamilyCount()} bundled families · available offline`}
                                </p>
                              </div>
                              <div className="border-border border-b p-2">
                                <SearchInput
                                  ref={fontFamilySearchRef}
                                  value={fontFamilyQuery}
                                  onChange={(event) => setFontFamilyQuery(event.target.value)}
                                  onMouseDown={claimEditorControlPointer}
                                  placeholder="Search fonts or categories"
                                  aria-label="Search font families"
                                  className="h-9 text-sm"
                                />
                              </div>
                              <div className="min-h-0 overflow-y-auto p-1">
                                {filteredFontGroups.length > 0 ? (
                                  filteredFontGroups.map((group) => (
                                    <div key={group.label} role="group" aria-label={group.label}>
                                      <p className="text-text-muted px-2 pt-2 pb-1 text-[11px] font-medium">
                                        {group.label}
                                      </p>
                                      {group.options.map((option) => (
                                        <button
                                          key={option.label}
                                          type="button"
                                          role="menuitemradio"
                                          aria-checked={activeFontFamily === option.value}
                                          onMouseDown={claimEditorControlPointer}
                                          onClick={() => applyFontFamily(option.value)}
                                          className={`rounded-button flex min-h-10 w-full items-center justify-between gap-3 px-2 text-left text-sm transition-colors duration-100 ${
                                            activeFontFamily === option.value
                                              ? "bg-primary/15 text-primary"
                                              : "text-text-muted hover:bg-surface-raised hover:text-text"
                                          }`}
                                        >
                                          <span style={{ fontFamily: option.value || "Inter" }}>
                                            {option.label}
                                          </span>
                                          {activeFontFamily === option.value && <Check size={13} />}
                                        </button>
                                      ))}
                                    </div>
                                  ))
                                ) : (
                                  <p
                                    className="text-text-muted px-3 py-6 text-center text-sm"
                                    role="status"
                                  >
                                    No matching fonts
                                  </p>
                                )}
                              </div>
                            </div>
                          </PopoverContent>
                        </Popover>
                        <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                          {meta.label}
                        </TooltipContent>
                      </Tooltip>
                    );
                  }
                  return (
                    <Tooltip key={action}>
                      <TooltipTrigger asChild>
                        <button
                          ref={action === "commands" ? commandMenuTriggerRef : undefined}
                          type="button"
                          title={meta.label}
                          aria-label={meta.label}
                          aria-pressed={meta.active}
                          aria-haspopup={action === "commands" ? "menu" : undefined}
                          aria-expanded={
                            action === "commands" ? Boolean(slashState?.searchable) : undefined
                          }
                          onPointerDown={claimEditorControlPointer}
                          onClick={() => runToolbarAction(action)}
                          className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${
                            meta.active
                              ? CONTROL_CHROME_ACTIVE_ACTION_CLASS
                              : `${getEditorToolbarActionTone(action)} active:bg-surface-raised hover:bg-surface-raised hover:text-text`
                          }`}
                        >
                          {meta.icon}
                        </button>
                      </TooltipTrigger>
                      <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                        {meta.label}
                      </TooltipContent>
                    </Tooltip>
                  );
                })}
                {editor.isActive("codeBlock") && (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <button
                        type="button"
                        title="Code block language"
                        aria-label="Code block language"
                        onPointerDown={claimEditorControlPointer}
                        onClick={() => setMobileCodeLanguageOpen(true)}
                        className={`${MOBILE_EDITOR_TOOLBAR_ACTION_CLASS} ${getEditorToolbarActionTone("codeBlock")} hover:bg-surface-raised hover:text-text`}
                      >
                        <FileCode size={18} />
                      </button>
                    </TooltipTrigger>
                    <TooltipContent side={showMobileCommandButton ? "top" : "bottom"}>
                      Code block language
                    </TooltipContent>
                  </Tooltip>
                )}
              </TooltipProvider>
            </div>
          </div>
        )}
        {mobileCodeLanguageOpen && !readOnly && (
          <DialogShell
            labelledBy="mobile-code-language-title"
            maxWidth="sm"
            onClose={() => setMobileCodeLanguageOpen(false)}
          >
            <div className="p-3">
              <div className="border-border mb-2 border-b pb-2">
                <h2 id="mobile-code-language-title" className="text-text text-sm font-medium">
                  Code block language
                </h2>
                <p className="text-text-muted mt-1 text-xs">
                  Choose syntax highlighting for this block.
                </p>
              </div>
              <div
                role="menu"
                aria-label="Code block language"
                className="max-h-72 overflow-y-auto"
              >
                {CODE_LANGUAGE_OPTIONS.map((option) => {
                  const active =
                    (editor.getAttributes("codeBlock").language || "") === option.value;
                  return (
                    <button
                      key={option.value || "plain"}
                      type="button"
                      role="menuitemradio"
                      aria-checked={active}
                      onClick={() => {
                        editor
                          .chain()
                          .focus()
                          .updateAttributes("codeBlock", { language: option.value })
                          .run();
                        setMobileCodeLanguageOpen(false);
                      }}
                      className={`rounded-button flex min-h-11 w-full items-center justify-between px-3 text-left text-sm transition-colors duration-100 ${
                        active
                          ? "bg-surface-raised text-text"
                          : "text-text-muted hover:bg-surface hover:text-text"
                      }`}
                    >
                      {option.label}
                      {active && <Check size={16} className="text-primary" />}
                    </button>
                  );
                })}
              </div>
            </div>
          </DialogShell>
        )}
        <input
          ref={imageInputRef}
          type="file"
          accept="image/*"
          multiple
          className="hidden"
          onChange={(event) => {
            void handlePickedFiles(event.target.files);
            event.target.value = "";
          }}
        />
        {blockMenu && (
          <Popover
            open={blockMenu.open}
            onOpenChange={(open) =>
              setBlockMenu((current) => (current ? { ...current, open } : current))
            }
          >
            {blockMenu.open && (
              <PopoverContent
                side="bottom"
                align="start"
                sideOffset={0}
                className="w-44 p-1"
                style={{
                  position: "fixed",
                  top: blockMenu.menuPosition.top,
                  left: blockMenu.menuPosition.left,
                }}
                onCloseAutoFocus={(event) => {
                  event.preventDefault();
                  blockMenuFocusTriggerRef.current?.focus();
                }}
                role="menu"
                aria-label="Block actions"
              >
                <div ref={blockMenuRef} onKeyDown={handleBlockMenuKeyDown}>
                  {[
                    ["Add block above", () => openInsertBlockPalette(editor, "above")],
                    ["Add block below", () => openInsertBlockPalette(editor, "below")],
                  ].map(([label, action]) => (
                    <button
                      key={label as string}
                      type="button"
                      role="menuitem"
                      onMouseDown={claimEditorControlPointer}
                      onClick={() => (action as () => void)()}
                      className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100"
                    >
                      {label as string}
                    </button>
                  ))}
                  <div className="bg-border my-1 h-px" />
                  {[
                    [
                      "Move block up",
                      () => {
                        if (moveCurrentBlock(editor, "up")) announceBlockMove("up");
                      },
                      blockMenu.canMoveUp,
                    ],
                    [
                      "Move block down",
                      () => {
                        if (moveCurrentBlock(editor, "down")) announceBlockMove("down");
                      },
                      blockMenu.canMoveDown,
                    ],
                    ["Duplicate block", () => duplicateCurrentBlock(editor), true],
                  ].map(([label, action, enabled]) => (
                    <button
                      key={label as string}
                      type="button"
                      role="menuitem"
                      disabled={!enabled}
                      onMouseDown={claimEditorControlPointer}
                      onClick={() => {
                        (action as () => void)();
                        closeBlockMenu();
                      }}
                      className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-40"
                    >
                      {label as string}
                    </button>
                  ))}
                  <div className="bg-border my-1 h-px" />
                  <Popover
                    open={blockMenu.transformOpen}
                    onOpenChange={(open) =>
                      setBlockMenu((current) =>
                        current ? { ...current, transformOpen: open } : current,
                      )
                    }
                  >
                    <PopoverTrigger asChild>
                      <button
                        type="button"
                        role="menuitem"
                        aria-haspopup="menu"
                        aria-expanded={blockMenu.transformOpen}
                        onMouseDown={claimEditorControlPointer}
                        className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100"
                      >
                        Turn into...
                      </button>
                    </PopoverTrigger>
                    <PopoverContent
                      side="right"
                      align="start"
                      sideOffset={4}
                      className="w-40 p-1"
                      role="menu"
                      aria-label="Turn block into"
                    >
                      {blockTransformOptions.map((option) => (
                        <button
                          key={option.id}
                          type="button"
                          role="menuitemradio"
                          aria-checked={option.active}
                          disabled={option.active}
                          onClick={() => transformCurrentBlock(option.id)}
                          className="starlane-menu-item disabled:bg-surface disabled:text-text block w-full text-left disabled:cursor-default"
                        >
                          {option.label}
                        </button>
                      ))}
                    </PopoverContent>
                  </Popover>
                  <div className="bg-border my-1 h-px" />
                  <button
                    type="button"
                    role="menuitem"
                    onMouseDown={claimEditorControlPointer}
                    onClick={() => {
                      if (!focusBlockMenuTarget(editor)) return;
                      deleteCurrentBlock(editor);
                      closeBlockMenu();
                    }}
                    className="rounded-button text-danger hover:bg-danger/10 block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100"
                  >
                    Delete block
                  </button>
                </div>
              </PopoverContent>
            )}
          </Popover>
        )}
        {contextMenu && (
          <ContextMenuContent aria-label="Editor context menu">
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
                <button
                  type="button"
                  role="menuitem"
                  disabled={readOnly}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    openLinkDialog(editor);
                    setContextMenu(null);
                  }}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text disabled:hover:text-text-muted block w-full px-2 py-1.5 text-left text-xs transition-colors duration-100 disabled:opacity-40 disabled:hover:bg-transparent"
                >
                  Link...
                </button>
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
          </ContextMenuContent>
        )}
      </div>
    </ContextMenu>
  );
}

/**
 * Simple markdown -> HTML conversion for TipTap.
 * Handles the most common markdown patterns.
 */
export type TemplateMarkerRenderMode = "edit" | "preview" | "omit";

function isInsideMarkdownCode(markdown: string, position: number): boolean {
  const prefix = markdown.slice(0, position);
  const lines = prefix.split("\n");
  const currentLine = lines.pop() || "";
  let fence: { character: "`" | "~"; length: number } | null = null;
  for (const line of lines) {
    const candidate = line.match(/^ {0,3}(`{3,}|~{3,})/);
    if (!fence && candidate) {
      fence = { character: candidate[1][0] as "`" | "~", length: candidate[1].length };
      continue;
    }
    if (fence && candidate && candidate[1][0] === fence.character && candidate[1].length >= fence.length) {
      fence = null;
    }
  }
  if (fence) return true;
  return /^(?: {0,3}\t| {4})/.test(currentLine);
}

function decodeTemplateValueSource(value: string): string {
  return decodeTemplateValueHtmlText(value);
}

function protectTemplateValueSpans(markdown: string): string {
  const pattern = /<span\s+data-starlane-template-value=(?:"v1"|'v1')(?:\s+style=(?:"white-space:pre-wrap"|'white-space:pre-wrap'))?\s*>([\s\S]*?)<\/span>/gi;
  const matches = Array.from(markdown.matchAll(pattern));
  if (!matches.length) return markdown;
  let cursor = 0;
  let output = "";
  for (const match of matches) {
    const index = match.index;
    if (index == null || isInsideMarkdownCode(markdown, index)) continue;
    output += markdown.slice(cursor, index);
    output += `\u0000STV1:${encodeTemplateValue(decodeTemplateValueSource(match[1]))}\u0000`;
    cursor = index + match[0].length;
  }
  return cursor ? output + markdown.slice(cursor) : markdown;
}

function renderTemplateMarkerHtml(
  marker: ParsedTemplateMarker,
  fields: readonly SlashTemplateField[],
  mode: TemplateMarkerRenderMode,
): string {
  if (mode === "omit") return "";
  if (marker.kind === "opaque") {
    if (mode === "preview") return "";
    return `<div data-starlane-template-marker="opaque" data-template-marker-raw="${escapeTemplateMarkerAttribute(marker.raw)}"></div>`;
  }
  if (marker.kind === "guidance") {
    const text = escapeHtml(marker.text);
    if (mode === "preview") {
      return `<aside class="template-guidance-preview" aria-label="Writing guidance"><strong>Writing guidance</strong><p>${text}</p></aside>`;
    }
    return `<div data-starlane-template-marker="guidance" data-template-marker-id="${escapeTemplateMarkerAttribute(marker.id)}" data-template-marker-text="${escapeTemplateMarkerAttribute(marker.text)}"></div>`;
  }

  const label = fields.find((field) => field.id === marker.field_id)?.label || "Named input";
  if (mode === "preview") return `<span data-starlane-template-field-preview="true">${escapeHtml(label)}</span>`;
  return `<div data-starlane-template-marker="field" data-template-marker-id="${escapeTemplateMarkerAttribute(marker.id)}" data-template-marker-field-id="${escapeTemplateMarkerAttribute(marker.field_id)}" data-template-marker-field-label="${escapeTemplateMarkerAttribute(label)}"></div>`;
}

export function markdownToHtml(
  md: string,
  insideSection = false,
  templateFields: readonly SlashTemplateField[] = [],
  templateMarkerMode: TemplateMarkerRenderMode = "edit",
): string {
  if (!md || !md.trim()) {
    return "<p></p>";
  }

  const normalizedMarkdown = protectTemplateValueSpans(md.replace(/\r\n?/g, "\n"));
  const sectionSegments = splitMarkdownSectionSegments(normalizedMarkdown);
  if (sectionSegments.some((segment) => segment.type === "section")) {
    return sectionSegments
      .map((segment) => {
        if (segment.type !== "section") {
          return markdownToHtml(segment.content, true, templateFields, templateMarkerMode);
        }

        if (!insideSection && isPlainLegacyWritingSection(segment)) {
          return markdownToHtml(segment.columns[0] ?? "", true, templateFields, templateMarkerMode);
        }

        const style = [
          segment.layout !== "single"
            ? `--section-grid-template-columns:${getSectionGridTemplateColumns(segment.layout)};`
            : "",
          segment.style.borderColor ? `border-color:${segment.style.borderColor};` : "",
          segment.style.contentBackgroundColor || segment.style.backgroundColor
            ? `background-color:${segment.style.contentBackgroundColor || segment.style.backgroundColor};`
            : "",
          segment.style.headerBackgroundColor
            ? `--section-header-bg:${segment.style.headerBackgroundColor};`
            : "",
        ].join("");
        const attributes = [
          `data-layout="${segment.layout}"`,
          segment.style.borderColor ? `data-border-color="${segment.style.borderColor}"` : "",
          segment.style.backgroundColor
            ? `data-background-color="${segment.style.backgroundColor}"`
            : "",
          segment.style.contentBackgroundColor
            ? `data-content-background-color="${segment.style.contentBackgroundColor}"`
            : "",
          segment.style.headerBackgroundColor
            ? `data-header-background-color="${segment.style.headerBackgroundColor}"`
            : "",
          segment.style.headerText
            ? `data-header-text="${escapeHtml(segment.style.headerText)}" data-section-label="${escapeHtml(segment.style.headerText)}"${getSectionLabelSlug(segment.style.headerText) ? ` data-section-label-slug="${getSectionLabelSlug(segment.style.headerText)}"` : ""}`
            : "",
          style ? `style="${style}"` : "",
        ]
          .filter(Boolean)
          .join(" ");
        const containerAttribute =
          segment.kind === "columns"
            ? 'data-starlane-columns="true"'
            : 'data-starlane-section="true"';
        return `<section ${containerAttribute} ${attributes}>${segment.columns
          .map(
            (column, columnIndex) =>
              `<div data-starlane-section-column="true" data-column-index="${columnIndex + 1}">${markdownToHtml(
                column,
                true,
                templateFields,
                templateMarkerMode,
              )}</div>`,
          )
          .join("")}</section>`;
      })
      .join("");
  }

  const blocks = splitMarkdownBlocks(normalizedMarkdown);
  const result: string[] = [];

  for (let blockIndex = 0; blockIndex < blocks.length; blockIndex += 1) {
    const rawBlock = blocks[blockIndex];
    if (/^(?: {0,3}\t| {4})/.test(rawBlock)) {
      const code = rawBlock.replace(/^(?: {0,3}\t| {4})/gm, "").replace(/\n+$/, "");
      result.push(`<pre><code>${escapeHtml(code)}</code></pre>`);
      continue;
    }
    const trimmed = rawBlock.trim();
    if (!trimmed) continue;

    if (isTableOfContentsDirective(trimmed)) {
      result.push('<div data-table-of-contents="true"></div>');
      continue;
    }

    const childPagesVariant = parseChildPagesDirective(trimmed);
    if (childPagesVariant) {
      result.push(
        `<div data-child-pages="true" data-child-pages-hydrate="true" data-child-pages-variant="${childPagesVariant}"></div>`,
      );
      continue;
    }

    const callout = parseCalloutDirective(trimmed);
    if (callout) {
      result.push(
        `<aside data-callout="true" data-callout-kind="${callout.kind}" role="note" aria-label="${callout.kind} callout">${inlineFormat(
          escapeHtml(callout.content),
        ).replace(/\n/g, "<br>")}</aside>`,
      );
      continue;
    }

    const expand = parseExpandDirective(trimmed);
    if (expand) {
      result.push(
        `<div data-expand-block="true" data-expand-title="${escapeHtmlAttribute(expand.title)}"><div data-expand-content="true">${markdownToHtml(expand.content, true, templateFields, templateMarkerMode)}</div></div>`,
      );
      continue;
    }

    const mathExpression = parseMathBlockDirective(trimmed);
    if (mathExpression != null) {
      result.push(
        `<div data-math-block="true" data-expression="${escapeHtmlAttribute(mathExpression)}" aria-label="Math block"></div>`,
      );
      continue;
    }

    const bookmarkMatch = trimmed.match(
      /^\[bookmark:([^\]]+)\]\((https?:\/\/[^\s)]+)(?:\s+"([^"]*)")?(?:\s+image="([^"]+)")?\)$/i,
    );
    if (bookmarkMatch) {
      result.push(
        renderBookmarkCardHtml(
          bookmarkMatch[2],
          bookmarkMatch[1].trim(),
          bookmarkMatch[3]?.trim() || "",
          bookmarkMatch[4]?.trim() || "",
        ),
      );
      continue;
    }

    const carouselMatch = trimmed.match(/^```carousel\n[\s\S]*?\n```$/i);
    if (carouselMatch) {
      const carousel = parseCarouselDirective(carouselMatch[0]);
      if (carousel) {
        result.push(
          `<div data-carousel-block="true" data-carousel-title="${escapeHtmlAttribute(carousel.title)}" data-carousel-items="${escapeHtmlAttribute(serializeCarouselItems(carousel.items))}"></div>`,
        );
        continue;
      }
    }

    const galleryMatch = trimmed.match(/^```gallery\n[\s\S]*?\n```$/i);
    if (galleryMatch) {
      const gallery = parseGalleryDirective(galleryMatch[0]);
      if (gallery) {
        result.push(
          `<div data-gallery-block="true" data-gallery-title="${escapeHtmlAttribute(gallery.title)}" data-gallery-layout="${gallery.layout}" data-gallery-items="${escapeHtmlAttribute(serializeGalleryItems(gallery.items))}"></div>`,
        );
        continue;
      }
    }

    const codeTabsMatch = trimmed.match(/^```code-tabs\n[\s\S]*?\n```$/i);
    if (codeTabsMatch) {
      const parsed = parseCodeTabsSource(
        codeTabsMatch[0].replace(/^```code-tabs\n/i, "").replace(/\n```$/i, ""),
      );
      const tabs = normalizeCodeTabs(parsed.tabs);
      const activeTab = tabs[0];
      const previewTabs = tabs
        .map(
          (tab, index) =>
            `<span role="tab" aria-selected="${index === 0 ? "true" : "false"}" class="code-tabs-preview__tab${index === 0 ? " is-active" : ""}">${escapeHtml(tab.label)}</span>`,
        )
        .join("");
      const preview = activeTab
        ? `<div data-code-tabs-preview="true" class="code-tabs-preview"><div class="code-tabs-preview__tabs" role="tablist">${previewTabs}</div><pre class="code-tabs-preview__code"><code class="language-${escapeHtmlAttribute(activeTab.language || "text")}">${highlightedCodeToHtml(activeTab.code, activeTab.language)}</code></pre></div>`
        : "";
      result.push(
        `<div data-code-tabs-block="true" data-code-tabs-title="${escapeHtmlAttribute(parsed.title || "")}" data-code-tabs-items="${escapeHtmlAttribute(JSON.stringify(tabs))}">${preview}</div>`,
      );
      continue;
    }

    // Code block. Keep raw text until now so blank lines and indentation survive.
    const codeMatch = trimmed.match(/^```([^\n]*)\n([\s\S]*?)\n?```$/);
    if (codeMatch) {
      const { language: lang, title, placement } = parseCodeFenceInfo(codeMatch[1]);
      result.push(
        `<pre${placement ? ` data-code-placement="${placement}"` : ""}${title ? ` data-code-title="${escapeHtmlAttribute(title)}"` : ""}><code${lang ? ` class="language-${escapeHtmlAttribute(lang)}"` : ""}>${highlightedCodeToHtml(
          codeMatch[2],
          lang,
        )}</code></pre>`,
      );
      continue;
    }

    const templateMarker = parseTemplateMarker(trimmed);
    if (templateMarker) {
      const html = renderTemplateMarkerHtml(templateMarker, templateFields, templateMarkerMode);
      if (html) result.push(html);
      continue;
    }

    // Headings
    // Empty ATX headings are valid Markdown and are produced briefly when a
    // user inserts a heading block before typing. Preserve that block shape
    // across autosave instead of reparsing `##` as a paragraph.
    const headingMatch = trimmed.match(/^(#{1,6})(?:[ \t]+(.*))?$/);
    if (headingMatch && trimmed.split("\n").length === 1) {
      const level = headingMatch[1].length;
      result.push(`<h${level}>${inlineFormat(escapeHtml(headingMatch[2] ?? ""))}</h${level}>`);
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

    // Table. Optional Starlane presentation metadata stays in a harmless HTML
    // comment so other Markdown readers can ignore it and render the table.
    const tableBlock = parseMarkdownTableBlock(trimmed);
    if (tableBlock) {
      result.push(markdownTableToHtml(tableBlock.markdown, tableBlock.style, tableBlock.placement));
      continue;
    }

    // Lists
    if (/^\s*(?:[-*]|\d+\.)\s/.test(trimmed)) {
      result.push(markdownListToHtml(trimmed));
      continue;
    }

    // Paragraph (default)
    const lines = trimmed.split("\n");
    const attachmentMatch =
      lines.length === 1
        ? trimmed.match(/^\[([^\]]+)\]\((\S+?)(?:\s+"starlane:size=(\d+)")?\)$/i)
        : null;
    if (
      attachmentMatch &&
      (/^starlane-attachment:\/\//i.test(attachmentMatch[2]) || attachmentMatch[3])
    ) {
      const name = escapeHtml(attachmentMatch[1]);
      const nameAttribute = escapeHtmlAttribute(attachmentMatch[1]);
      const href = attachmentMatch[2];
      const attachmentIdMatch = href.match(/^starlane-attachment:\/\/([^\s)]+)$/i);
      const attachmentId = attachmentIdMatch ? escapeHtmlAttribute(attachmentIdMatch[1]) : "";
      const safeUrl = attachmentId ? "" : sanitizeRenderedUrl(href, "link") || "";
      const size = Number.parseInt(attachmentMatch[3] || "0", 10) || 0;
      result.push(
        `<div data-type="file-attachment" data-name="${nameAttribute}" data-size="${size}" data-url="${safeUrl}"${attachmentId ? ` data-attachment-id="${attachmentId}"` : ""} class="file-attachment-card" contenteditable="false"><button type="button" data-file-attachment-download="true" aria-label="Download ${nameAttribute}" class="file-attachment-download"><span class="file-attachment-card__content"><strong>${name}</strong><span>${formatFileSize(size)}</span></span><span class="file-attachment-card__action">Download</span></button></div>`,
      );
      continue;
    }
    const paraLines = lines.map((l) => inlineFormat(escapeHtml(l))).join("<br>");
    result.push(`<p>${paraLines}</p>`);
  }

  const html = result.join("") || "<p></p>";
  return html;
}

type MarkdownSectionSegment =
  | {
      type: "markdown";
      content: string;
    }
  | {
      type: "section";
      content: string;
      kind: "section" | "columns";
      layout: SectionLayout;
      style: SectionStyle;
      columns: string[];
    };

function isPlainLegacyWritingSection(
  segment: Extract<MarkdownSectionSegment, { type: "section" }>,
): boolean {
  return (
    segment.kind === "section" &&
    segment.layout === "single" &&
    segment.columns.length === 1 &&
    !segment.style.borderColor &&
    !segment.style.backgroundColor &&
    !segment.style.contentBackgroundColor &&
    !segment.style.headerBackgroundColor &&
    !segment.style.headerText &&
    splitMarkdownSectionSegments(segment.columns[0] ?? "").every(
      (child) => child.type === "markdown",
    )
  );
}

export function splitMarkdownSectionSegments(md: string): MarkdownSectionSegment[] {
  const segments: MarkdownSectionSegment[] = [];
  const markdownLines: string[] = [];
  const sectionLines: string[] = [];
  const containerEnds: string[] = [];
  let containerKind: "section" | "columns" = "section";
  let sectionLayout: SectionLayout = "single";
  let sectionStyle: SectionStyle = {};
  let inFence = false;
  let inMathBlock = false;

  const flushMarkdown = () => {
    if (markdownLines.length === 0) return;
    const content = markdownLines.join("\n");
    if (content.trim()) segments.push({ type: "markdown", content });
    markdownLines.length = 0;
  };

  for (const line of md.split("\n")) {
    const trimmed = line.trim();
    if (/^```/.test(trimmed)) {
      (containerEnds.length > 0 ? sectionLines : markdownLines).push(line);
      inFence = !inFence;
      continue;
    }

    if (!inFence && /^\$\$\s*$/.test(trimmed)) {
      (containerEnds.length > 0 ? sectionLines : markdownLines).push(line);
      inMathBlock = !inMathBlock;
      continue;
    }

    const parsedOptions = !inFence && !inMathBlock ? parseSectionStartOptions(trimmed) : null;
    const parsedColumns = !inFence && !inMathBlock ? parseColumnsStartMarker(trimmed) : null;
    if (parsedOptions || parsedColumns) {
      if (containerEnds.length === 0) {
        flushMarkdown();
        containerKind = parsedColumns ? "columns" : "section";
        sectionLayout = parsedColumns ?? parsedOptions!.layout;
        sectionStyle = parsedOptions?.style ?? {};
      } else {
        sectionLines.push(line);
      }
      containerEnds.push(parsedColumns ? COLUMNS_END_MARKER : SECTION_END_MARKER);
      continue;
    }

    if (
      !inFence &&
      !inMathBlock &&
      containerEnds.length > 0 &&
      trimmed === containerEnds[containerEnds.length - 1]
    ) {
      containerEnds.pop();
      if (containerEnds.length === 0) {
        const columns = splitMarkdownSectionColumns(sectionLines.join("\n"));
        const inferredLayout =
          containerKind === "section" && sectionLayout === "single"
            ? inferSectionLayoutFromColumnCount(columns.length)
            : sectionLayout;
        const expectedColumnCount = getSectionColumnCount(inferredLayout);
        while (columns.length < expectedColumnCount) columns.push("");
        if (columns.length > expectedColumnCount) {
          columns[expectedColumnCount - 1] = columns.slice(expectedColumnCount - 1).join("\n\n");
          columns.length = expectedColumnCount;
        }
        segments.push({
          type: "section",
          content: sectionLines.join("\n").trim(),
          kind: containerKind,
          layout: inferredLayout,
          style: sectionStyle,
          columns,
        });
        sectionLines.length = 0;
        sectionLayout = "single";
        sectionStyle = {};
        containerKind = "section";
      } else {
        sectionLines.push(line);
      }
      continue;
    }

    (containerEnds.length > 0 ? sectionLines : markdownLines).push(line);
  }

  if (containerEnds.length > 0) {
    markdownLines.push(
      containerKind === "columns"
        ? getColumnsStartMarker(sectionLayout)
        : getSectionStartMarker(sectionLayout),
      ...sectionLines,
    );
    sectionLines.length = 0;
  }
  flushMarkdown();
  return segments.length > 0 ? segments : [{ type: "markdown", content: md }];
}

export function splitMarkdownSectionColumns(md: string): string[] {
  const columns: string[][] = [[]];
  const containerEnds: string[] = [];
  let inFence = false;
  let inMathBlock = false;

  for (const line of md.split("\n")) {
    const trimmed = line.trim();
    if (/^```/.test(trimmed)) {
      columns[columns.length - 1]?.push(line);
      inFence = !inFence;
      continue;
    }
    if (!inFence && /^\$\$\s*$/.test(trimmed)) {
      columns[columns.length - 1]?.push(line);
      inMathBlock = !inMathBlock;
      continue;
    }

    const nestedSection = !inFence && !inMathBlock ? parseSectionStartMarker(trimmed) : null;
    const nestedColumns = !inFence && !inMathBlock ? parseColumnsStartMarker(trimmed) : null;
    if (nestedSection || nestedColumns) {
      containerEnds.push(nestedColumns ? COLUMNS_END_MARKER : SECTION_END_MARKER);
    }
    if (
      !inFence &&
      !inMathBlock &&
      containerEnds.length > 0 &&
      trimmed === containerEnds[containerEnds.length - 1]
    ) {
      containerEnds.pop();
    }

    if (
      !inFence &&
      !inMathBlock &&
      containerEnds.length === 0 &&
      (trimmed === SECTION_COLUMN_MARKER ||
        (trimmed.startsWith(SECTION_COLUMN_MARKER_PREFIX) && /index=\d+\s*-->$/.test(trimmed)))
    ) {
      columns.push([]);
      continue;
    }
    columns[columns.length - 1]?.push(line);
  }

  return columns.map((column) => column.join("\n").trim());
}

function splitMarkdownBlocks(md: string): string[] {
  const blocks: string[] = [];
  const lines = md.split("\n");
  let current: string[] = [];
  let inFence = false;
  let inMathBlock = false;

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

    if (!inFence && /^\$\$\s*$/.test(line.trim())) {
      current.push(line);
      inMathBlock = !inMathBlock;
      continue;
    }

    if (!inFence && !inMathBlock && line.trim() === "") {
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

function sanitizeFontFamily(value: string): string {
  return value.replace(/[^\w\s"',.-]/g, "").trim();
}

export function highlightedCodeToHtml(code: string, language: string): string {
  if (!language) return escapeHtml(code);

  try {
    const tree = editorLowlight.highlight(language, code) as {
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

export function parseMarkdownTableBlock(
  block: string,
): { markdown: string; style: TableStyleId; placement?: TablePlacement } | null {
  const lines = block
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  if (lines.length === 0) return null;

  const directiveStyle = parseTableStyleDirective(lines[0]);
  const markdown = (directiveStyle ? lines.slice(1) : lines).join("\n");
  if (!isMarkdownTable(markdown)) return null;

  return {
    markdown,
    style: directiveStyle ?? "directory",
    ...(directiveStyle && parseTablePlacementDirective(lines[0]) !== "center"
      ? { placement: parseTablePlacementDirective(lines[0]) }
      : {}),
  };
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

function markdownTableToHtml(
  block: string,
  style: TableStyleId,
  placement?: TablePlacement,
): string {
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

  return `<table data-table-style="${style}"${placement && placement !== "center" ? ` data-table-placement="${placement}"` : ""}><thead>${head}</thead><tbody>${body}</tbody></table>`;
}

/** Apply inline markdown formatting */
function sanitizeRenderedUrl(value: string, kind: "link" | "image"): string | null {
  const decoded = value.replace(/&amp;/g, "&").trim();
  const allowed =
    kind === "link"
      ? /^(https?:|mailto:|starlane:|starlane-asset:|asset:\/\/localhost\/)/i
      : /^(https?:|starlane:|starlane-asset:|starlane-attachment:|asset:\/\/localhost\/|data:image\/(?:png|gif|jpe?g|webp);base64,)/i;
  return allowed.test(decoded) ? escapeHtmlAttribute(decoded) : null;
}

function inlineFormat(text: string): string {
  const prepared = text;
  const formatted =
    prepared
      // Preserved font-family spans. The content is escaped before this point, so only
      // allow a constrained style value back through.
      .replace(
        /&lt;span style="font-family:\s*([^"]+)"&gt;([\s\S]*?)&lt;\/span&gt;/g,
        (_match, fontFamily, inner) =>
          `<span style="font-family: ${escapeHtmlAttribute(sanitizeFontFamily(fontFamily))}">${inner}</span>`,
      )
      .replace(
        /&lt;span style="color:\s*([^;"]+)"&gt;([\s\S]*?)&lt;\/span&gt;/g,
        (_match, color, inner) => {
          const safeColor = sanitizeTextColor(color);
          return safeColor ? `<span style="color: ${safeColor}">${inner}</span>` : inner;
        },
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
      .replace(
        /!\[([^\]]*)\]\((\S+?)(?:\s+"starlane:align=(left|center|right)")?\)/g,
        (_match, alt, url, rawAlignment) => {
          const attachmentMatch = String(url).match(/^starlane-attachment:\/\/([^\s)]+)$/i);
          const attachmentAttribute = attachmentMatch
            ? ` data-attachment-id="${escapeHtmlAttribute(attachmentMatch[1])}"`
            : "";
          const safeUrl = attachmentMatch ? "" : sanitizeRenderedUrl(url, "image");
          const alignment = normalizeImageAlignment(rawAlignment);
          const alignmentAttribute =
            alignment === "left" ? "" : ` data-starlane-align="${alignment}"`;
          return safeUrl || attachmentMatch
            ? `<img src="${safeUrl || ""}" alt="${alt}"${attachmentAttribute}${alignmentAttribute} />`
            : alt;
        },
      )
      // Portable member mentions. Handle these before standard links so the leading @ and member ID survive.
      .replace(/@\[([^\]\n]+)\]\(([A-Za-z0-9_-]{1,128})\)/g, (_match, displayName, userId) => {
        return `<span data-type="content-mention" data-user-id="${escapeHtmlAttribute(userId)}" class="editor-content-mention">@${displayName}</span>`;
      })
      // Portable page links. Stable-ID links navigate within the active space;
      // title-only legacy links remain visibly unresolved instead of pretending
      // to be an external URL.
      .replace(
        /\[\[([^\]\n|]+?)(?:\|([A-Za-z0-9_-]{1,128}))?\]\]/g,
        (_match, rawTitle, documentId) => {
          const title = String(rawTitle).trim();
          if (!title) return _match;
          return documentId
            ? `<a href="${escapeHtmlAttribute(buildWikiDocumentHref(documentId))}" class="starlane-link starlane-wiki-link" target="_self" title="Open ${escapeHtmlAttribute(title)}">${title}</a>`
            : `<span class="starlane-wiki-link starlane-wiki-link--unresolved" data-starlane-wiki-title="${escapeHtmlAttribute(title)}" title="Page target unavailable">${title}</span>`;
        },
      )
      // Links
      .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_match, label, url) => {
        const safeUrl = sanitizeRenderedUrl(url, "link");
        return safeUrl
          ? `<a href="${safeUrl}" class="starlane-link" target="_blank" rel="noopener noreferrer">${label}</a>`
          : label;
      })
      // Content tags
      .replace(/(^|[\s(])#([A-Za-z][\w-]{0,63})(?=\b)/g, (_match, prefix, tagName) => {
        const safeTag = escapeHtmlAttribute(tagName);
        return `${prefix}<span data-type="content-tag" data-tag="${safeTag}" class="editor-content-tag">#${safeTag}</span>`;
      });
  return formatted.replace(/\u0000STV1:([A-Za-z0-9_-]+)\u0000/g, (_match, encoded: string) => {
    const value = escapeHtml(decodeTemplateValue(encoded));
    return `<span data-starlane-template-value="v1" style="white-space:pre-wrap">${value}</span>`;
  });
}

/**
 * Convert TipTap HTML back to markdown for storage.
 * Uses DOMParser for safe HTML parsing (no innerHTML on live DOM).
 */
export function stripRuntimeBlockIdentity(html: string): string {
  return html.replace(/\sdata-block-(?:id|type)=(?:"[^"]*"|'[^']*')/g, "");
}

export function htmlToMarkdown(html: string): string {
  const parser = new DOMParser();
  const doc = parser.parseFromString(stripRuntimeBlockIdentity(html), "text/html");
  return nodeToMarkdown(doc.body).trim();
}

export function shouldApplyExternalEditorContent(
  currentMarkdown: string,
  nextMarkdown: string,
): boolean {
  return currentMarkdown.trim() !== nextMarkdown.trim();
}

/** Parses the portable fence header: `typescript title="workspace.ts"`. */
export function parseCodeFenceInfo(info: string): {
  language: string;
  title?: string;
  placement?: BlockPlacement;
} {
  const match = info
    .trim()
    .match(
      /^(?:((?!(?:title|placement)=)[\w-]+))?(?:\s*title=(?:"([^"]*)"|'([^']*)'))?(?:\s*placement=(left|center|right|expand))?\s*$/,
    );
  if (!match) return { language: info.trim().match(/^[\w-]*/)?.[0] || "" };
  const title = (match[2] ?? match[3] ?? "").trim().slice(0, 120);
  return {
    language: match[1] || "",
    ...(title ? { title } : {}),
    ...(match[4] ? { placement: normalizeBlockPlacement(match[4]) } : {}),
  };
}

export function serializeCodeFenceInfo(
  language: string,
  title?: string | null,
  placement?: unknown,
): string {
  const normalizedLanguage = language.trim().replace(/[^\w-]/g, "");
  const normalizedTitle = (title || "")
    .replace(/["\r\n]+/g, " ")
    .trim()
    .slice(0, 120);
  const alignment = normalizeBlockPlacement(placement);
  return `${normalizedLanguage}${normalizedTitle ? ` title="${normalizedTitle}"` : ""}${alignment === "center" ? "" : ` placement=${alignment}`}`;
}

export function serializeMarkdownImage(
  src: string,
  alt: string,
  alignment: unknown = "left",
): string {
  return `![${alt}](${src}${getImageAlignmentMarkdownTitle(normalizeImageAlignment(alignment))})`;
}

export function serializeFileAttachmentMarkdown(
  name: string,
  href?: string | null,
  size?: number | null,
): string {
  const label = name.trim() || "Attachment";
  const url = href?.trim() || "";
  const normalizedSize = Number.isFinite(size) && Number(size) > 0 ? Math.round(Number(size)) : 0;
  return url
    ? `[${label}](${url}${normalizedSize ? ` "starlane:size=${normalizedSize}"` : ""})`
    : label;
}

export function serializeBookmarkCardDirective(
  title: string | null,
  href: string | null,
  description?: string | null,
  imageUrl?: string | null,
): string {
  const safeDescription = description?.replace(/"/g, "'").trim();
  const safeImageUrl = imageUrl?.trim();
  return `[bookmark:${title || "Bookmark"}](${href || ""}${safeDescription ? ` "${safeDescription}"` : ""}${safeImageUrl ? ` image="${safeImageUrl.replace(/"/g, "'")}"` : ""})`;
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

    if (
      tag === "section" &&
      (el.hasAttribute("data-starlane-section") || el.hasAttribute("data-starlane-columns"))
    ) {
      const layout = normalizeSectionLayout(el.getAttribute("data-layout"));
      const columns = Array.from(
        el.querySelectorAll<HTMLElement>(":scope > [data-starlane-section-column]"),
      ).map((column) => nodeToMarkdown(column).trim());
      const content = columns.length > 0 ? columns : [nodeToMarkdown(el).trim()];
      const serialized = el.hasAttribute("data-starlane-columns")
        ? serializeEditorColumnsMarkdown(content, layout)
        : serializeEditorSectionMarkdown(content, layout, {
            borderColor: el.getAttribute("data-border-color") || undefined,
            backgroundColor: el.getAttribute("data-background-color") || undefined,
            contentBackgroundColor: el.getAttribute("data-content-background-color") || undefined,
            headerBackgroundColor: el.getAttribute("data-header-background-color") || undefined,
            headerText: el.getAttribute("data-header-text") || undefined,
          });
      parts.push(`${serialized}\n\n`);
      continue;
    }

    if (tag === "div" && el.hasAttribute("data-starlane-template-marker")) {
      const kind = el.getAttribute("data-starlane-template-marker");
      if (kind === "guidance") {
        parts.push(
          `${serializeTemplateMarker({
            kind: "guidance",
            id: el.getAttribute("data-template-marker-id") || "guidance",
            text: el.getAttribute("data-template-marker-text") || "",
          })}\n\n`,
        );
      } else if (kind === "field") {
        parts.push(
          `${serializeTemplateMarker({
            kind: "field",
            id: el.getAttribute("data-template-marker-id") || "field",
            field_id: el.getAttribute("data-template-marker-field-id") || "unbound",
          })}\n\n`,
        );
      } else {
        parts.push(`${el.getAttribute("data-template-marker-raw") || ""}\n\n`);
      }
      continue;
    }

    if (tag === "div" && el.hasAttribute("data-carousel-block")) {
      const title = el.getAttribute("data-carousel-title") || DEFAULT_CAROUSEL_TITLE;
      const items = normalizeCarouselItems(el.getAttribute("data-carousel-items"));
      parts.push(`${serializeCarouselDirective(title, items)}\n\n`);
      continue;
    }

    if (tag === "div" && el.hasAttribute("data-gallery-block")) {
      const title = el.getAttribute("data-gallery-title") || DEFAULT_GALLERY_TITLE;
      const layout = normalizeGalleryLayout(el.getAttribute("data-gallery-layout"));
      const items = normalizeCarouselItems(el.getAttribute("data-gallery-items"));
      parts.push(`${serializeGalleryDirective(title, layout, items)}\n\n`);
      continue;
    }

    if (tag === "div" && el.hasAttribute("data-code-tabs-block")) {
      let tabs: unknown = [];
      try {
        tabs = JSON.parse(el.getAttribute("data-code-tabs-items") || "[]");
      } catch {
        tabs = [];
      }
      parts.push(
        `${serializeCodeTabs(el.getAttribute("data-code-tabs-title") || undefined, normalizeCodeTabs(tabs))}\n\n`,
      );
      continue;
    }

    if (tag === "div" && el.hasAttribute("data-bookmark-card")) {
      parts.push(
        `${serializeBookmarkCardDirective(
          el.getAttribute("title"),
          el.getAttribute("href"),
          el.getAttribute("description"),
          el.getAttribute("image-url"),
        )}\n\n`,
      );
      continue;
    }

    if (tag === "div" && el.hasAttribute("data-expand-block")) {
      const content = el.querySelector<HTMLElement>("[data-expand-content]");
      parts.push(
        `${serializeExpandDirective(el.getAttribute("data-expand-title") || "", content ? nodeToMarkdown(content).trim() : "")}\n\n`,
      );
      continue;
    }

    if (tag === "aside" && el.hasAttribute("data-callout")) {
      parts.push(
        `${serializeCalloutDirective(
          normalizeCalloutKind(el.getAttribute("data-callout-kind")),
          inlineToMd(el),
        )}\n\n`,
      );
      continue;
    }

    if (tag === "div" && el.hasAttribute("data-math-block")) {
      parts.push(`${serializeMathBlockDirective(el.getAttribute("data-expression") || "")}\n\n`);
      continue;
    }

    if (tag === "div" && el.hasAttribute("data-table-of-contents")) {
      parts.push(`${TABLE_OF_CONTENTS_DIRECTIVE}\n\n`);
      continue;
    }

    if (tag === "div" && el.hasAttribute("data-child-pages")) {
      parts.push(
        `${el.getAttribute("data-child-pages-variant") === "collapsible" ? COLLAPSIBLE_CHILD_PAGES_DIRECTIVE : CHILD_PAGES_DIRECTIVE}\n\n`,
      );
      continue;
    }

    if (tag === "div" && el.getAttribute("data-type") === "file-attachment") {
      const name = el.getAttribute("data-name") || el.textContent?.trim() || "Attachment";
      const size = Number(el.getAttribute("data-size") || 0);
      const attachmentId = el.getAttribute("data-attachment-id")?.trim();
      const href = attachmentId
        ? `starlane-attachment://${attachmentId}`
        : el.getAttribute("data-url") || el.querySelector("a[href]")?.getAttribute("href") || "";
      parts.push(`${serializeFileAttachmentMarkdown(name, href, size)}\n\n`);
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
        const title = el.getAttribute("data-code-title") || code?.getAttribute("data-code-title");
        parts.push(
          `\`\`\`${serializeCodeFenceInfo(lang, title, el.getAttribute("data-code-placement"))}\n${text}\n\`\`\`\n\n`,
        );
        break;
      }
      case "hr":
        parts.push("---\n\n");
        break;
      case "img": {
        const src = el.getAttribute("src") || "";
        const attachmentId = el.getAttribute("data-attachment-id");
        const alt = el.getAttribute("alt") || "";
        const alignment = normalizeImageAlignment(el.getAttribute("data-starlane-align"));
        const serializedSrc = attachmentId ? `starlane-attachment://${attachmentId}` : src;
        parts.push(`${serializeMarkdownImage(serializedSrc, alt, alignment)}\n\n`);
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
      case "a": {
        const wikiDocumentId = parseWikiDocumentHref(c.getAttribute("href") || "");
        result += wikiDocumentId
          ? serializeWikiLinkMarkdown(inner, wikiDocumentId)
          : `[${inner}](${c.getAttribute("href") || ""})`;
        break;
      }
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
        if (c.getAttribute("data-starlane-template-value") === "v1") {
          result += serializeTemplateValue(inner);
          break;
        }
        if (c.hasAttribute("data-starlane-wiki-title")) {
          result += serializeWikiLinkMarkdown(c.getAttribute("data-starlane-wiki-title") || inner);
          break;
        }
        if (c.getAttribute("data-type") === "content-tag") {
          result += c.textContent || inner;
          break;
        }
        if (c.getAttribute("data-type") === "content-mention") {
          const userId = c.getAttribute("data-user-id") || "";
          const displayName = (c.textContent || inner).replace(/^@/, "");
          result += userId ? `@[${displayName}](${userId})` : `@${displayName}`;
          break;
        }
        const fontFamily = sanitizeFontFamily(c.style.fontFamily || "");
        const fontSize = c.style.fontSize.replace(/[^0-9a-z.% -]/gi, "").trim();
        const textColor = sanitizeTextColor(c.style.color || "");
        if (textColor) {
          result += `<span style="color: ${textColor}">${inner}</span>`;
          break;
        }
        if (fontFamily) {
          result += `<span style="font-family: ${fontFamily}">${inner}</span>`;
        } else if (fontSize) {
          result += `<span style="font-size: ${fontSize}">${inner}</span>`;
        } else {
          result += inner;
        }
        break;
      }
      case "input":
        break;
      case "img": {
        const src = c.getAttribute("src") || "";
        const attachmentId = c.getAttribute("data-attachment-id");
        const alt = c.getAttribute("alt") || "";
        const alignment = normalizeImageAlignment(c.getAttribute("data-starlane-align"));
        const serializedSrc = attachmentId ? `starlane-attachment://${attachmentId}` : src;
        result += serializeMarkdownImage(serializedSrc, alt, alignment);
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
  const markdown = lines.join("\n");
  const directive = serializeTableStyleDirective(
    table.getAttribute("data-table-style"),
    table.getAttribute("data-table-placement"),
  );
  return directive ? `${directive}\n${markdown}` : markdown;
}
