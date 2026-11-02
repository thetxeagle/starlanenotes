import { useCallback, useEffect, useRef, useState } from "react";
import type { Editor as TipTapEditor } from "@tiptap/react";
import { isTauri } from "@/lib/api";
import {
  captureAssetInsertionRange,
  AssetInsertionRangeError,
  type AssetInsertionRange,
  type AssetInsertionPosition,
} from "@/lib/assetInsertionRange";
import { dataClient, type LocalAssetContext } from "@/lib/dataClient";
import { directUploadAcknowledgementId, markDirectUploadAdopted } from "@/lib/directUploadReview";
import {
  AssetInsertionScopeError,
  useAssetInsertionScope,
  type VerifyAssetInsertion,
} from "./useAssetInsertionScope";

interface FileDropResult {
  isDragging: boolean;
  dropRef: React.RefObject<HTMLDivElement | null>;
  hasPendingFileDrops: () => boolean;
  fileDropError: string | null;
}

/** Keep source adoption fenced through every asynchronous read/upload/insertion. */
export async function runPendingFileDrop<T>(
  pending: { current: number },
  run: () => Promise<T>,
): Promise<T> {
  pending.current += 1;
  try {
    return await run();
  } finally {
    pending.current -= 1;
  }
}

const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg"]);

function getExtension(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot >= 0 ? name.slice(dot + 1).toLowerCase() : "";
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function insertImageAtCursor(
  editor: TipTapEditor,
  src: string,
  alt: string,
  attachmentId?: string | null,
  position?: AssetInsertionPosition,
) {
  const insertAt = position
    ? position.from === position.to
      ? position.from
      : position
    : editor.state.selection.to;
  return editor
    .chain()
    .focus()
    .insertContentAt(insertAt, [
      { type: "image", attrs: { src, alt, attachmentId: attachmentId || null } },
      { type: "paragraph" },
    ])
    .run();
}

function insertTextAtCursor(editor: TipTapEditor, text: string, position: AssetInsertionPosition) {
  const insertAt = position.from === position.to ? position.from : position;
  return editor.chain().focus().insertContentAt(insertAt, text).run();
}

function insertAttachmentCard(
  editor: TipTapEditor,
  name: string,
  size: number,
  url?: string,
  attachmentId?: string | null,
  position?: AssetInsertionPosition,
) {
  const insertAt = position
    ? position.from === position.to
      ? position.from
      : position
    : editor.state.selection.to;
  const sizeStr = formatFileSize(size);
  const title = escapeHtml(name);
  const label = url
    ? `<a href="${escapeHtml(url)}" class="text-primary underline">${title}</a>`
    : `<strong>${title}</strong>`;
  const html =
    `<div data-type="file-attachment" data-name="${title}" data-size="${size}" data-url="${escapeHtml(url || "")}"${attachmentId ? ` data-attachment-id="${escapeHtml(attachmentId)}"` : ""} class="file-attachment-card" contenteditable="false">` +
    `<p>${label} <em>(${sizeStr})</em></p>` +
    `</div>`;
  return editor.chain().focus().insertContentAt(insertAt, html).run();
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

function readFileAsText(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsText(file);
  });
}

function editable(editor: TipTapEditor) {
  if (editor.isDestroyed || !editor.isEditable) {
    throw new Error("The original editor is no longer editable; file insertion stopped.");
  }
}

export async function handleDroppedFile(
  file: File,
  editor: TipTapEditor,
  context: LocalAssetContext,
  verifyScope: VerifyAssetInsertion = () => editable(editor),
  capturedRange?: AssetInsertionRange,
) {
  verifyScope();
  editable(editor);
  const range = capturedRange ?? captureAssetInsertionRange(editor);
  const verify = () => {
    verifyScope();
    editable(editor);
    range.current();
  };
  try {
    verify();
    const ext = getExtension(file.name);
    if (ext === "md") {
      const text = await readFileAsText(file);
      verify();
      if (!insertTextAtCursor(editor, text, range.current()))
        throw new Error("File text could not be inserted.");
      range.advance();
      verify();
      return;
    }
    const dataUrl = await readFileAsDataUrl(file);
    verify();
    const asset = await dataClient.saveDocumentDataAsset({
      name: file.name,
      mime_type: file.type || "application/octet-stream",
      data_url: dataUrl,
      ...context,
    });
    verify();
    const serverAttachment = "filename" in asset;
    const inserted = IMAGE_EXTENSIONS.has(ext)
      ? insertImageAtCursor(
          editor,
          serverAttachment ? dataUrl : asset.url,
          serverAttachment ? asset.original_name || asset.filename : asset.name,
          serverAttachment ? asset.id : null,
          range.current(),
        )
      : insertAttachmentCard(
          editor,
          serverAttachment ? asset.original_name || asset.filename : asset.name,
          serverAttachment ? asset.size_bytes : asset.size,
          serverAttachment ? dataUrl : asset.url,
          serverAttachment ? asset.id : null,
          range.current(),
        );
    if (!inserted)
      throw new Error("Uploaded file could not be inserted; retain its original upload review.");
    range.advance();
    verify();
    if (serverAttachment && context.space_id && context.document_id) {
      const operation = directUploadAcknowledgementId(asset);
      if (operation) {
        await markDirectUploadAdopted(
          { space_id: context.space_id, document_id: context.document_id,draft_id:context.draft_id },
          operation,
        );
        verify();
      }
    }
  } finally {
    if (!capturedRange) range.close();
  }
}

/** Native fallback is allowed only while the original operation still owns insertion. */
export async function handleNativeDroppedFile(
  filePath: string,
  editor: TipTapEditor,
  context: LocalAssetContext,
  verifyScope: VerifyAssetInsertion,
  readText?: (path: string) => Promise<string>,
  capturedRange?: AssetInsertionRange,
) {
  verifyScope();
  editable(editor);
  const range = capturedRange ?? captureAssetInsertionRange(editor);
  const verify = () => {
    verifyScope();
    editable(editor);
    range.current();
  };
  try {
    verify();
    const name = filePath.split("/").pop() || filePath;
    const ext = getExtension(name);
    if (ext === "md") {
      let text: string;
      try {
        if (readText) {
          text = await readText(filePath);
        } else {
          const modulePath = "@tauri-apps/plugin-fs";
          const fs = (await import(/* @vite-ignore */ modulePath)) as {
            readTextFile: (path: string) => Promise<string>;
          };
          verify();
          text = await fs.readTextFile(filePath);
        }
      } catch (error) {
        if (error instanceof AssetInsertionScopeError || error instanceof AssetInsertionRangeError)
          throw error;
        verify();
        if (!insertAttachmentCard(editor, name, 0, undefined, null, range.current()))
          throw new Error("File fallback could not be inserted.");
        range.advance();
        verify();
        return;
      }
      verify();
      if (!insertTextAtCursor(editor, text, range.current()))
        throw new Error("File text could not be inserted.");
      range.advance();
      verify();
      return;
    }
    let asset: Awaited<ReturnType<typeof dataClient.importAssetFromPath>>;
    try {
      asset = await dataClient.importAssetFromPath(filePath, context);
    } catch (error) {
      if (error instanceof AssetInsertionScopeError || error instanceof AssetInsertionRangeError)
        throw error;
      // Never catch lost ownership into an insertion in the newly selected draft.
      verify();
      const inserted = IMAGE_EXTENSIONS.has(ext)
        ? insertImageAtCursor(
            editor,
            `asset://localhost/${encodeURIComponent(filePath)}`,
            name,
            null,
            range.current(),
          )
        : insertAttachmentCard(editor, name, 0, undefined, null, range.current());
      if (!inserted) throw new Error("File fallback could not be inserted.");
      range.advance();
      verify();
      return;
    }
    verify();
    const inserted = IMAGE_EXTENSIONS.has(ext)
      ? insertImageAtCursor(editor, asset.url, asset.name, null, range.current())
      : insertAttachmentCard(editor, asset.name, asset.size, asset.url, null, range.current());
    if (!inserted) throw new Error("File could not be inserted.");
    range.advance();
    verify();
  } finally {
    if (!capturedRange) range.close();
  }
}

/** Stop a group immediately on failure; its original successful uploads remain retained. */
export async function runFileDropGroup<T>(
  items: readonly T[],
  verify: VerifyAssetInsertion,
  run: (item: T) => Promise<void>,
) {
  for (const item of items) {
    verify();
    await run(item);
    verify();
  }
}

/** Late async listener registration is immediately cleaned after its owner stops. */
export function createFileDropListenerOwner() {
  let stopped = false;
  const cleanups: Array<() => void> = [];
  return {
    current: () => !stopped,
    retain(cleanup: () => void) {
      if (stopped) cleanup();
      else cleanups.push(cleanup);
    },
    stop() {
      stopped = true;
      for (const cleanup of cleanups.splice(0)) cleanup();
    },
  };
}

export function isNestedFileDropBoundary(target: EventTarget | null): boolean {
  const closest = (target as { closest?: (selector: string) => Element | null } | null)?.closest;
  return (
    typeof closest === "function" &&
    Boolean(closest.call(target, '[data-starlane-file-drop-boundary="true"]'))
  );
}

/**
 * Hook that manages file drag-and-drop onto the editor area.
 * Supports HTML5 drag-and-drop and Tauri's native file drop events.
 */
export function useFileDrop(
  editor: TipTapEditor | null,
  context: LocalAssetContext = {},
  selection: { draft_id?: string; readOnly?: boolean } = {},
): FileDropResult {
  const [isDragging, setIsDragging] = useState(false);
  const [fileDropError, setFileDropError] = useState<string | null>(null);
  const capture = useAssetInsertionScope(editor, { ...context, ...selection });
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const reportError = useCallback((error: unknown) => {
    if (mounted.current)
      setFileDropError(
        error instanceof Error
          ? error.message
          : "File insertion failed; retain the original file and review any completed upload.",
      );
  }, []);
  const dropRef = useRef<HTMLDivElement>(null);
  const dragCounter = useRef(0);
  const pendingFileDrops = useRef(0);
  const hasPendingFileDrops = useCallback(() => pendingFileDrops.current > 0, []);

  // HTML5 drag-and-drop handlers
  const onDragEnter = useCallback((e: DragEvent) => {
    if (isNestedFileDropBoundary(e.target)) return;
    e.preventDefault();
    e.stopPropagation();
    dragCounter.current++;
    if (e.dataTransfer?.types.includes("Files")) {
      setIsDragging(true);
    }
  }, []);

  const onDragOver = useCallback((e: DragEvent) => {
    if (isNestedFileDropBoundary(e.target)) return;
    e.preventDefault();
    e.stopPropagation();
  }, []);

  const onDragLeave = useCallback((e: DragEvent) => {
    if (isNestedFileDropBoundary(e.target)) return;
    e.preventDefault();
    e.stopPropagation();
    dragCounter.current--;
    if (dragCounter.current <= 0) {
      dragCounter.current = 0;
      setIsDragging(false);
    }
  }, []);

  const onDrop = useCallback(
    async (e: DragEvent) => {
      if (isNestedFileDropBoundary(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      dragCounter.current = 0;
      setIsDragging(false);

      if (!editor || editor.isDestroyed) return;

      const files = e.dataTransfer?.files;
      if (!files || files.length === 0) return;

      try {
        const verify = capture();
        setFileDropError(null);
        const range = captureAssetInsertionRange(editor);
        try {
          await runPendingFileDrop(pendingFileDrops, () =>
            runFileDropGroup(Array.from(files), verify, (file) =>
              handleDroppedFile(file, editor, context, verify, range),
            ),
          );
        } finally {
          range.close();
        }
      } catch (error) {
        reportError(error);
      }
    },
    [editor, context, capture, reportError],
  );

  // Attach HTML5 listeners to the drop zone element
  useEffect(() => {
    const el = dropRef.current;
    if (!el) return;

    el.addEventListener("dragenter", onDragEnter);
    el.addEventListener("dragover", onDragOver);
    el.addEventListener("dragleave", onDragLeave);
    el.addEventListener("drop", onDrop);

    return () => {
      el.removeEventListener("dragenter", onDragEnter);
      el.removeEventListener("dragover", onDragOver);
      el.removeEventListener("dragleave", onDragLeave);
      el.removeEventListener("drop", onDrop);
    };
  }, [onDragEnter, onDragOver, onDragLeave, onDrop]);

  // Tauri-specific drag-drop event listener
  useEffect(() => {
    if (!isTauri() || !editor) return;

    const listeners = createFileDropListenerOwner();
    async function setup() {
      try {
        const modulePath = "@tauri-apps/api/event";
        const mod = (await import(/* @vite-ignore */ modulePath)) as {
          listen: (
            event: string,
            handler: (e: { payload: { paths: string[] } }) => void,
          ) => Promise<() => void>;
        };
        if (!listeners.current()) return;
        listeners.retain(
          await mod.listen("tauri://drag-over", () => {
            if (listeners.current()) setIsDragging(true);
          }),
        );
        if (!listeners.current()) return;
        listeners.retain(
          await mod.listen("tauri://drag-leave", () => {
            if (listeners.current()) setIsDragging(false);
          }),
        );
        if (!listeners.current()) return;
        listeners.retain(
          await mod.listen("tauri://drag-drop", (event) => {
            if (!listeners.current()) return;
            setIsDragging(false);
            if (!editor || editor.isDestroyed) return;
            // The event callback handles rejection itself; native dispatch does not await it.
            void (async () => {
              try {
                const verify = capture();
                setFileDropError(null);
                const range = captureAssetInsertionRange(editor);
                try {
                  await runPendingFileDrop(pendingFileDrops, () =>
                    runFileDropGroup(event.payload.paths, verify, (path) =>
                      handleNativeDroppedFile(path, editor, context, verify, undefined, range),
                    ),
                  );
                } finally {
                  range.close();
                }
              } catch (error) {
                reportError(error);
              }
            })();
          }),
        );
      } catch {
        // API absence or partial registration must not retain live listeners.
        listeners.stop();
      }
    }
    void setup();
    return () => listeners.stop();
  }, [editor, context, capture, reportError]);

  return { isDragging, dropRef, hasPendingFileDrops, fileDropError };
}
