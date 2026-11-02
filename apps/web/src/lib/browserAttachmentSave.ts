import { api, captureOfflineServerContext, getApiBase } from "./api";
import { sameOfflineReplayOrigin, verifyOfflineReplayContext } from "./offlineReplayOrigin";
import {
  fetchExactAttachmentVersion,
  type SelectedAttachmentVersion,
} from "./nativeAttachmentProduct";
import {
  prepareBrowserAttachmentExport,
  type AttachmentExportWritable,
} from "./browserAttachmentExport";
interface Destination {
  name: string;
  createWritable(options: { keepExistingData: false }): Promise<AttachmentExportWritable>;
}
type Picker = (options: { suggestedName: string }) => Promise<Destination>;
const picker = () => (window as Window & { showSaveFilePicker?: Picker }).showSaveFilePicker;
export function supportsBrowserAttachmentSave() {
  return typeof picker() === "function";
}
function key(selection: SelectedAttachmentVersion) {
  const v = JSON.parse(selection.rawVersion);
  return (
    "starlane_browser_attachment_export_v1:" +
    JSON.stringify([selection.origin, selection.serverPage, v.logical_attachment_id, v.version_id])
  );
}
export function browserAttachmentSavePending(
  selection: SelectedAttachmentVersion,
): Array<{ key: string; destinationName: string }> {
  try {
    const prefix = key(selection) + ":";
    if (localStorage.length > 10000)
      throw new Error("Save review index exceeds its bound; inspect stored export records.");
    const pending: Array<{ key: string; destinationName: string }> = [];
    for (let i = 0; i < localStorage.length; i++) {
      const name = localStorage.key(i);
      if (!name?.startsWith(prefix)) continue;
      const raw = localStorage.getItem(name);
      if (raw === null) throw new Error("Save review index changed.");
      const record = JSON.parse(raw);
      if (
        record.format !== 1 ||
        record.state !== "unknown-before-close" ||
        typeof record.destinationName !== "string"
      )
        throw new Error("Original Save outcome record malformed; retain it for review.");
      pending.push({ key: name, destinationName: record.destinationName });
      if (pending.length > 100) throw new Error("Too many original Save outcomes to review.");
    }
    return pending;
  } catch {
    return [
      {
        key: "",
        destinationName: "Stored Save evidence requires manual review; automatic retry is disabled",
      },
    ];
  }
}
export function browserAttachmentSaveNeedsReview(selection: SelectedAttachmentVersion) {
  return browserAttachmentSavePending(selection).length > 0;
}
export function acknowledgeBrowserAttachmentSaveReview(selection: SelectedAttachmentVersion) {
  const context = captureOfflineServerContext();
  verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  if (!sameOfflineReplayOrigin(context.origin, selection.origin))
    throw new Error("Original export context changed.");
  const pending = browserAttachmentSavePending(selection);
  if (pending.some((record) => !record.key))
    throw new Error(
      "Original Save evidence malformed or unavailable; preserve it for manual review.",
    );
  for (const record of pending) {
    localStorage.removeItem(record.key);
    if (localStorage.getItem(record.key) !== null)
      throw new Error("Export review could not be recorded.");
  }
}
/** Picker runs directly in user action; destination overwrite consent belongs to the browser. */
export async function saveBrowserAttachmentVersion(selection: SelectedAttachmentVersion) {
  const context = captureOfflineServerContext(),
    verify = () => {
      verifyOfflineReplayContext(context, getApiBase(), api.getToken());
      if (!sameOfflineReplayOrigin(context.origin, selection.origin))
        throw new Error("Original selected export context changed.");
    };
  verify();
  const choose = picker();
  if (!choose)
    throw new Error(
      "This browser cannot stream directly to a selected file. Use the desktop app for bounded Save.",
    );
  if (browserAttachmentSaveNeedsReview(selection))
    throw new Error(
      "Previous Save outcome needs review. Inspect the chosen destination before another Save.",
    );
  const v = JSON.parse(selection.rawVersion);
  const destination = await choose.call(window, { suggestedName: v.original_name });
  verify();
  await finishBrowserAttachmentSave(selection, destination, context);
}
async function finishBrowserAttachmentSave(
  selection: SelectedAttachmentVersion,
  destination: Destination,
  context: ReturnType<typeof captureOfflineServerContext>,
) {
  const verify = () => {
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    if (!sameOfflineReplayOrigin(context.origin, selection.origin))
      throw new Error("Original selected export context changed.");
  };
  verify();
  if (browserAttachmentSaveNeedsReview(selection))
    throw new Error(
      "Previous Save outcome needs review. Inspect the chosen destination before another Save.",
    );
  const v = JSON.parse(selection.rawVersion);
  const writable = await destination.createWritable({ keepExistingData: false });
  let prepared: Awaited<ReturnType<typeof prepareBrowserAttachmentExport>> | undefined;
  try {
    verify();
    prepared = await prepareBrowserAttachmentExport(selection, writable, () =>
      fetchExactAttachmentVersion(selection.serverPage, v.version_id),
    );
    verify();
    const operationId = crypto.randomUUID();
    const journal = JSON.stringify({
      format: 1,
      operationId,
      origin: selection.origin,
      serverPage: selection.serverPage,
      rawVersion: selection.rawVersion,
      destinationName: destination.name,
      state: "unknown-before-close",
    });
    const recordKey = key(selection) + ":" + operationId;
    localStorage.setItem(recordKey, journal);
    if (localStorage.getItem(recordKey) !== journal)
      throw new Error("Save outcome record unavailable; destination not finalized.");
    verify();
    await prepared.publish();
    verify();
    localStorage.removeItem(recordKey);
    if (localStorage.getItem(recordKey) !== null)
      throw new Error("Saved file complete; outcome review record could not be cleared.");
  } catch (error) {
    if (prepared) await prepared.discard().catch(() => undefined);
    else await writable.abort(error).catch(() => undefined);
    throw error;
  }
}

/** File-block action chooses destination before metadata awaits, retaining browser user activation. */
export async function saveBrowserPinnedAttachment(
  page: { space_id: string; document_id: string },
  versionId: string,
  suggestedName: string,
) {
  const context = captureOfflineServerContext(),
    verify = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  verify();
  const choose = picker();
  if (!choose)
    throw new Error(
      "This browser cannot stream directly to a selected file. Use the desktop app for bounded Save.",
    );
  const destination = await choose.call(window, { suggestedName });
  verify();
  const selection = await fetchExactAttachmentVersion(page, versionId);
  verify();
  await finishBrowserAttachmentSave(selection, destination, context);
}
