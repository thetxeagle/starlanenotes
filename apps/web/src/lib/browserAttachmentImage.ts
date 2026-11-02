import { api, captureOfflineServerContext, getApiBase } from "./api";
import { verifyOfflineReplayContext, sameOfflineReplayOrigin } from "./offlineReplayOrigin";
import {
  fetchExactAttachmentVersion,
  type SelectedAttachmentVersion,
} from "./nativeAttachmentProduct";
import { prepareBrowserAttachmentExport } from "./browserAttachmentExport";

import {
  ownBrowserImagePreview,
  collectAbandonedBrowserImagePreviews,
  type BrowserImagePreviewLease,
} from "./browserAttachmentImageOwnership";

const activeUrls = new Map<string, BrowserImagePreviewLease>();
export function releaseBrowserAttachmentImage(url: string): void {
  const owned = activeUrls.get(url);
  if (owned) {
    activeUrls.delete(url);
    URL.revokeObjectURL(url);
    void owned.release().catch(() => undefined);
  }
}
if (typeof window !== "undefined")
  window.addEventListener("pagehide", () => {
    for (const url of [...activeUrls.keys()]) releaseBrowserAttachmentImage(url);
  });

/** Browser-managed File snapshot; never response.blob/arrayBuffer/dataURL. Decoder RAM is separate. */
export async function previewBrowserAttachmentImage(
  selection: SelectedAttachmentVersion,
): Promise<string> {
  const context = captureOfflineServerContext();
  const check = () => {
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    if (!sameOfflineReplayOrigin(context.origin, selection.origin))
      throw new Error("Original image context changed");
  };
  check();
  const metadata = JSON.parse(selection.rawVersion) as { version_id: string; mime_type: string };
  if (
    !["image/png", "image/jpeg", "image/webp", "image/gif", "image/avif", "image/svg+xml"].includes(
      metadata.mime_type,
    )
  )
    throw new Error("Original version is not a supported image");
  if (!navigator.storage?.getDirectory)
    throw new Error(
      "This browser cannot stream exact image previews. Save this version or use the desktop app.",
    );
  const root = await navigator.storage.getDirectory();
  check();
  await collectAbandonedBrowserImagePreviews(root).catch(() => undefined);
  check();
  const lease = await ownBrowserImagePreview(root);
  const name = lease.name;
  let handle: FileSystemFileHandle | null = null;
  let writer: FileSystemWritableFileStream | null = null;
  let published = false;
  let ownedUrl: string | null = null;
  try {
    check();
    handle = await lease.root.getFileHandle(name, { create: true });
    check();
    writer = await handle.createWritable();
    check();
    const streamWriter = writer;
    const sink = {
      write: async (bytes: Uint8Array) => {
        await streamWriter.write(Uint8Array.from(bytes));
      },
      close: () => streamWriter.close(),
      abort: () => streamWriter.abort(),
    };
    const prepared = await prepareBrowserAttachmentExport(selection, sink, () =>
      fetchExactAttachmentVersion(selection.serverPage, metadata.version_id),
    );
    await prepared.publish();
    published = true;
    check();
    const file = await handle.getFile();
    check();
    const typed = file.slice(0, file.size, metadata.mime_type);
    const url = URL.createObjectURL(typed);
    activeUrls.set(url, lease);
    ownedUrl = url;
    return url;
  } finally {
    if (writer && !published) await writer.abort().catch(() => undefined);
    if (!ownedUrl) await lease.release().catch(() => undefined);
  }
}
