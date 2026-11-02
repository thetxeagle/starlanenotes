import { api, captureOfflineServerContext, getApiBase, isTauri } from "./api";
import {
  sameOfflineReplayOrigin,
  verifyOfflineReplayContext,
  isOfflineReplayOrigin,
  type OfflineReplayOrigin,
} from "./offlineReplayOrigin";
import { prepareServerSpacePortableArchive } from "./serverPortableArchive";
import { prepareServerSpaceStaticArchive } from "./serverStaticArchive";
import type { StaticSiteExportOptions } from "./staticSiteExport";
import type { ArchiveWritable, PreparedArchive } from "./streamingZip";
type Destination = {
  name: string;
  createWritable(options: { keepExistingData: false }): Promise<ArchiveWritable>;
};
type Picker = (options: {
  suggestedName: string;
  types: Array<{ description: string; accept: Record<string, string[]> }>;
}) => Promise<Destination>;
const prefix = "starlane_browser_portable_archive_v1:";
const picker = () => (window as Window & { showSaveFilePicker?: Picker }).showSaveFilePicker;
export const supportsBrowserPortableArchive = () => !isTauri() && typeof picker() === "function";
interface Pending {
  origin: OfflineReplayOrigin;
  spaceId: string;
  name: string;
}
export function pendingBrowserPortableArchives(
  spaceId: string,
): Array<{ id: string; name: string }> {
  const context = captureOfflineServerContext();
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  try {
    if (localStorage.length > 10000) throw new Error("Destination review storage exceeds bound");
    const records: Array<{ id: string; name: string }> = [];
    for (let i = 0; i < localStorage.length; i++) {
      const id = localStorage.key(i);
      if (!id?.startsWith(prefix)) continue;
      const raw = localStorage.getItem(id);
      if (!raw || raw.length > 65536) throw new Error("Destination review record invalid");
      const record = JSON.parse(raw) as Pending;
      if (
        !record ||
        typeof record.spaceId !== "string" ||
        typeof record.name !== "string" ||
        !isOfflineReplayOrigin(record.origin)
      )
        throw new Error("Destination review record invalid");
      if (record.spaceId === spaceId && sameOfflineReplayOrigin(record.origin, context.origin)) {
        records.push({ id, name: record.name });
        if (records.length > 100) throw new Error("Too many destination outcomes require review");
      }
    }
    return records;
  } catch {
    return [
      {
        id: "",
        name: "Archive destination outcomes require manual review; local review records are unavailable or invalid.",
      },
    ];
  }
}
/** Explicit user acknowledgement; never erase malformed records or another original context. */
export function acknowledgeBrowserPortableArchiveReview(spaceId: string): void {
  const context = captureOfflineServerContext();
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  const pending = pendingBrowserPortableArchives(spaceId);
  if (pending.some((p) => !p.id))
    throw new Error("Manual recovery of archive destination records required");
  for (const item of pending) {
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    localStorage.removeItem(item.id);
  }
}
/** Invoke directly in the click handler: destination chooser precedes asynchronous enumeration. */
export function saveBrowserServerSpaceArchive(spaceId: string): Promise<void> {
  return saveServerSpaceArchive(spaceId,"markdown",{});
}
export function saveBrowserServerSpaceStaticArchive(spaceId: string, options: StaticSiteExportOptions = {}): Promise<void> {
  return saveServerSpaceArchive(spaceId,"static",{...options});
}
async function saveServerSpaceArchive(spaceId: string, format: "markdown" | "static", options: StaticSiteExportOptions): Promise<void> {
  if (isTauri())
    throw new Error(
      "Native portable archive Save is unavailable until the native archive owner is supported. Use a browser with direct file streaming.",
    );
  const choose = picker();
  if (!choose) throw new Error("This browser cannot stream a portable archive to a selected file.");
  const context = captureOfflineServerContext();
  const verify = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  verify();
  if (pendingBrowserPortableArchives(spaceId).length)
    throw new Error(
      "Inspect the named archive destination files and explicitly record that review before another Save.",
    );
  const destinationPromise = choose.call(window, {
    suggestedName: format === "static" ? "starlane-static-site.zip" : "starlane-space.zip",
    types: [{ description: format === "static" ? "Static site ZIP" : "Portable Markdown ZIP", accept: { "application/zip": [".zip"] } }],
  });
  const destination = await destinationPromise;
  verify();
  let writable: ArchiveWritable | null = null;
  let prepared: PreparedArchive | null = null;
  let closing = false;
  const id = prefix + crypto.randomUUID();
  try {
    writable = await destination.createWritable({ keepExistingData: false });
    verify();
    prepared = format === "static"
      ? await prepareServerSpaceStaticArchive(spaceId,writable,options)
      : await prepareServerSpacePortableArchive(spaceId, writable);
    verify();
    localStorage.setItem(
      id,
      JSON.stringify({ origin: context.origin, spaceId, name: destination.name } satisfies Pending),
    );
    verify();
    closing = true;
    await prepared.publish();
    verify();
    localStorage.removeItem(id);
  } catch (error) {
    if (!closing) {
      if (prepared) await prepared.discard().catch(() => undefined);
      else if (writable) await writable.abort(error).catch(() => undefined);
    }
    if (closing)
      throw new Error(
        "Archive Save outcome unknown. Inspect the selected destination file before another Save.",
      );
    throw error;
  }
}
