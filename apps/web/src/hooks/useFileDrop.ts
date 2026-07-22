import { useCallback, useEffect, useRef, useState } from "react";
import type { Editor as TipTapEditor } from "@tiptap/react";
import { isTauri } from "@/lib/api";
import { dataClient, type LocalAssetContext } from "@/lib/dataClient";

interface FileDropResult {
  isDragging: boolean;
  dropRef: React.RefObject<HTMLDivElement>;
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

function insertImageAtCursor(editor: TipTapEditor, src: string, alt: string) {
  editor.chain().focus().setImage({ src, alt }).run();
}

function insertTextAtCursor(editor: TipTapEditor, text: string) {
  editor.chain().focus().insertContent(text).run();
}

function insertAttachmentCard(editor: TipTapEditor, name: string, size: number, url?: string) {
  const sizeStr = formatFileSize(size);
  const title = escapeHtml(name);
  const label = url
    ? `<a href="${escapeHtml(url)}" class="text-primary underline">${title}</a>`
    : `<strong>${title}</strong>`;
  const html =
    `<div data-type="file-attachment" class="file-attachment-card">` +
    `<p>${label} <em>(${sizeStr})</em></p>` +
    `</div>`;
  editor.chain().focus().insertContent(html).run();
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

export async function handleDroppedFile(
  file: File,
  editor: TipTapEditor,
  context: LocalAssetContext,
) {
  const ext = getExtension(file.name);

  if (IMAGE_EXTENSIONS.has(ext)) {
    const dataUrl = await readFileAsDataUrl(file);
    const asset = await dataClient.saveBrowserAsset({
      name: file.name,
      mime_type: file.type || undefined,
      size: file.size,
      url: dataUrl,
      ...context,
    });
    insertImageAtCursor(editor, asset.url, file.name);
  } else if (ext === "md") {
    const text = await readFileAsText(file);
    insertTextAtCursor(editor, text);
  } else {
    const dataUrl = await readFileAsDataUrl(file);
    const asset = await dataClient.saveBrowserAsset({
      name: file.name,
      mime_type: file.type || undefined,
      size: file.size,
      url: dataUrl,
      ...context,
    });
    insertAttachmentCard(editor, asset.name, asset.size, asset.url);
  }
}

/**
 * Hook that manages file drag-and-drop onto the editor area.
 * Supports HTML5 drag-and-drop and Tauri's native file drop events.
 */
export function useFileDrop(
  editor: TipTapEditor | null,
  context: LocalAssetContext = {},
): FileDropResult {
  const [isDragging, setIsDragging] = useState(false);
  const dropRef = useRef<HTMLDivElement>(null);
  const dragCounter = useRef(0);

  // HTML5 drag-and-drop handlers
  const onDragEnter = useCallback((e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounter.current++;
    if (e.dataTransfer?.types.includes("Files")) {
      setIsDragging(true);
    }
  }, []);

  const onDragOver = useCallback((e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  }, []);

  const onDragLeave = useCallback((e: DragEvent) => {
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
      e.preventDefault();
      e.stopPropagation();
      dragCounter.current = 0;
      setIsDragging(false);

      if (!editor || editor.isDestroyed) return;

      const files = e.dataTransfer?.files;
      if (!files || files.length === 0) return;

      for (const file of Array.from(files)) {
        await handleDroppedFile(file, editor, context);
      }
    },
    [editor, context],
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

    let unlisten: (() => void) | undefined;

    async function setup() {
      try {
        const modulePath = "@tauri-apps/api/event";
        const mod = (await import(/* @vite-ignore */ modulePath)) as {
          listen: (
            event: string,
            handler: (e: {
              payload: { paths: string[]; position: { x: number; y: number } };
            }) => void,
          ) => Promise<() => void>;
        };

        // Listen for drag hover
        const unlistenHover = await mod.listen("tauri://drag-over", () => {
          setIsDragging(true);
        });

        // Listen for drag cancel
        const unlistenCancel = await mod.listen("tauri://drag-leave", () => {
          setIsDragging(false);
        });

        // Listen for file drop
        const unlistenDrop = await mod.listen(
          "tauri://drag-drop",
          async (event: { payload: { paths: string[] } }) => {
            setIsDragging(false);
            if (!editor || editor.isDestroyed) return;

            for (const filePath of event.payload.paths) {
              const name = filePath.split("/").pop() || filePath;
              const ext = getExtension(name);

              if (IMAGE_EXTENSIONS.has(ext)) {
                try {
                  const asset = await dataClient.importAssetFromPath(filePath, context);
                  insertImageAtCursor(editor, asset.url, asset.name);
                } catch {
                  const assetUrl = `asset://localhost/${encodeURIComponent(filePath)}`;
                  insertImageAtCursor(editor, assetUrl, name);
                }
              } else if (ext === "md") {
                // Read file content via fetch on asset protocol
                try {
                  const fsModule = "@tauri-apps/plugin-fs";
                  const fs = (await import(/* @vite-ignore */ fsModule)) as {
                    readTextFile: (path: string) => Promise<string>;
                  };
                  const text = await fs.readTextFile(filePath);
                  insertTextAtCursor(editor, text);
                } catch {
                  // Fallback: insert as attachment
                  insertAttachmentCard(editor, name, 0);
                }
              } else {
                try {
                  const asset = await dataClient.importAssetFromPath(filePath, context);
                  insertAttachmentCard(editor, asset.name, asset.size, asset.url);
                } catch {
                  insertAttachmentCard(editor, name, 0);
                }
              }
            }
          },
        );

        unlisten = () => {
          unlistenHover();
          unlistenCancel();
          unlistenDrop();
        };
      } catch {
        // Tauri API not available
      }
    }

    setup();
    return () => unlisten?.();
  }, [editor, context]);

  return { isDragging, dropRef };
}
