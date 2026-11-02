import { api, captureOfflineServerContext, getApiBase } from "./api";
import {
  sameOfflineReplayOrigin,
  verifyOfflineReplayContext,
  type OfflineReplayOrigin,
} from "./offlineReplayOrigin";
export interface BrowserSelectedAttachmentVersion {
  rawVersion: string;
  origin: OfflineReplayOrigin;
  serverPage: { space_id: string; document_id: string };
}
export interface AttachmentExportWritable {
  write(bytes: Uint8Array): Promise<void>;
  abort(reason?: unknown): Promise<void>;
  close(): Promise<void>;
}
export interface PreparedBrowserAttachmentExport {
  byteLength: number;
  publish(): Promise<void>;
  discard(): Promise<void>;
}
const CHUNK = 65536,
  MAXIMUM = 100 * 1024 * 1024;
/** Caller owns user destination selection. This does not promise picker no-clobber semantics. */
export async function prepareBrowserAttachmentExport(
  selection: BrowserSelectedAttachmentVersion,
  writable: AttachmentExportWritable,
  refreshSelection: () => Promise<BrowserSelectedAttachmentVersion>,
): Promise<PreparedBrowserAttachmentExport> {
  let context: ReturnType<typeof captureOfflineServerContext>;
  try {
    context = captureOfflineServerContext();
  } catch (error) {
    await writable.abort(error).catch(() => undefined);
    throw error;
  }
  const verify = () => {
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    if (!sameOfflineReplayOrigin(context.origin, selection.origin))
      throw new Error("Attachment export original session differs");
  };
  const fresh = async () => {
    verify();
    const current = await refreshSelection();
    verify();
    if (
      current.rawVersion !== selection.rawVersion ||
      !sameOfflineReplayOrigin(current.origin, selection.origin) ||
      current.serverPage.space_id !== selection.serverPage.space_id ||
      current.serverPage.document_id !== selection.serverPage.document_id
    )
      throw new Error("Attachment export selected version changed");
  };
  let reader: ReadableStreamDefaultReader<Uint8Array> | undefined,
    finished = false;
  const abort = async (reason?: unknown) => {
    if (finished) return;
    finished = true;
    await writable.abort(reason);
  };
  try {
    verify();
    const v = JSON.parse(selection.rawVersion) as Record<string, unknown>;
    const uuid = (value: unknown): value is string =>
      typeof value === "string" &&
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
    if (
      !uuid(v.version_id) ||
      !uuid(v.logical_attachment_id) ||
      !Number.isSafeInteger(v.byte_length) ||
      (v.byte_length as number) < 0 ||
      (v.byte_length as number) > MAXIMUM ||
      typeof v.binary_sha256 !== "string" ||
      !/^[a-f0-9]{64}$/.test(v.binary_sha256)
    )
      throw new Error("Attachment export exact bounded metadata required");
    await fresh();
    const path = `/spaces/${encodeURIComponent(selection.serverPage.space_id)}/documents/${encodeURIComponent(selection.serverPage.document_id)}/attachment-families/${v.logical_attachment_id}/versions/${v.version_id}/file`;
    const response = await fetch(context.origin.serverBase + path, {
      headers: { Authorization: `Bearer ${context.bearer}` },
      credentials: "omit",
      redirect: "error",
    });
    verify();
    if (!response.ok || !response.body)
      throw new Error("Attachment export current exact bytes unavailable");
    const declared = response.headers.get("content-length");
    if (declared !== null && declared !== String(v.byte_length))
      throw new Error("Attachment export declared length differs");
    reader = response.body.getReader();
    let length = 0;
    for (;;) {
      const next = await reader.read();
      verify();
      if (next.done) break;
      length += next.value.byteLength;
      if (length > (v.byte_length as number))
        throw new Error("Attachment export exceeds selected length");
      for (let offset = 0; offset < next.value.length; offset += CHUNK) {
        verify();
        await writable.write(next.value.subarray(offset, offset + CHUNK));
        verify();
      }
    }
    if (length !== v.byte_length) throw new Error("Attachment export selected bytes incomplete");
    await fresh();
    return {
      byteLength: length,
      discard: () => abort(),
      publish: async () => {
        if (finished) throw new Error("Attachment export already finalized");
        try {
          await fresh();
          verify();
        } catch (error) {
          await abort(error);
          throw error;
        }
        finished = true;
        try {
          await writable.close();
          verify();
        } catch {
          throw new Error(
            "Attachment export outcome unknown after destination close; inspect the selected file before retrying",
          );
        }
      },
    };
  } catch (error) {
    await abort(error).catch(() => undefined);
    throw error;
  } finally {
    if (reader) {
      await reader.cancel().catch(() => undefined);
      reader.releaseLock();
    }
  }
}
