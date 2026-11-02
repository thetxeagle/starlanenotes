import { api, captureOfflineServerContext, getApiBase } from "./api";
import { sameOfflineReplayOrigin, verifyOfflineReplayContext } from "./offlineReplayOrigin";
import { prepareBrowserAttachmentExport, type BrowserSelectedAttachmentVersion } from "./browserAttachmentExport";
import { portableAttachmentReferences, verifyPortableAttachmentReference } from "./portableAttachmentReferences";
import { rewriteExactAssetReferences } from "./nativeArchiveReferences";
import { prepareStreamingZip, type ArchiveEntry, type ArchiveWritable, type PreparedArchive } from "./streamingZip";
import { coverAttachmentReferences } from "./publishedCover";
import type { PublishedDocumentMetadata } from "@starlane-notes/shared-types";
export interface PortableArchivePage {
  publishedMetadata?: PublishedDocumentMetadata;
  documentId: string;
  content: string;
  /** Re-read original page content and access; reject replacement/revision/context changes. */
  verifyCurrent(): Promise<void>;
}
export interface PortableArchiveSelection {
  selected: BrowserSelectedAttachmentVersion;
  refresh(): Promise<BrowserSelectedAttachmentVersion>;
}
/** Portable Markdown archive; caller supplies exact page-authorized selections, never filename/head guesses. */
export async function prepareBrowserPortableArchive(pages: readonly PortableArchivePage[],
  selections: readonly PortableArchiveSelection[], destination: ArchiveWritable): Promise<PreparedArchive> {
  let delegated = false;
  try {
    const context = captureOfflineServerContext();
    const verifySession = async () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    if (pages.length > 1024 || selections.length > 1024) throw new Error("Archive exceeds bounded ownership count");
    const selected = new Map<string, PortableArchiveSelection>();
    const text = (name: string, content: string): ArchiveEntry => {
      const bytes = new TextEncoder().encode(content);
      return {name,byteLength:bytes.length,transfer:async write=>write(bytes)};
    };
    let metadataBytes = 0;
    for (const selection of selections) {
      if (!sameOfflineReplayOrigin(context.origin, selection.selected.origin)) throw new Error("Archive original attachment session differs");
      const rawLength = new TextEncoder().encode(selection.selected.rawVersion).length;
      if (rawLength > 65536) throw new Error("Archive selected metadata exceeds bound");
      const id = JSON.parse(selection.selected.rawVersion).version_id;
      if (typeof id !== "string" || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id) || selected.has(id))
        throw new Error("Archive requires unique exact version selections");
      metadataBytes += rawLength;
      selected.set(id,selection);
    }
    const entries: ArchiveEntry[] = [], references = new Set<string>();
    for (const page of pages) {
      await verifySession(); await page.verifyCurrent(); await verifySession();
      metadataBytes += new TextEncoder().encode(page.content).length + new TextEncoder().encode(JSON.stringify(page.publishedMetadata ?? null)).length;
      if (metadataBytes > 8 * 1024 * 1024) throw new Error("Archive metadata exceeds bounded ownership limit");
      const claims = new Map<string,string>();
      for (const [reference,id] of new Map([...portableAttachmentReferences(page.content,context.origin.serverBase),...coverAttachmentReferences(page.publishedMetadata?.cover_image,context.origin.serverBase)])) {
        if (!selected.has(id)) throw new Error("Archive referenced exact attachment bytes unavailable");
        verifyPortableAttachmentReference(reference,selected.get(id)!.selected.rawVersion,context.origin.serverBase);
        references.add(id);claims.set(reference,`../assets/${id}.bin`);
      }
      entries.push(text(`pages/${encodeURIComponent(page.documentId)}.md`,rewriteExactAssetReferences(page.content,claims)));
      if (page.publishedMetadata) {
        const original=page.publishedMetadata, cover=original.cover_image;
        entries.push(text(`pages/${encodeURIComponent(page.documentId)}.metadata.json`,JSON.stringify({...original,cover_image:cover ? claims.get(cover) ?? cover : cover})));
        entries.push(text(`sources/${encodeURIComponent(page.documentId)}.metadata.json`,JSON.stringify(original)));
      }
      // Preserve source reference bytes separately from rewritten portable Markdown.
      entries.push(text(`sources/${encodeURIComponent(page.documentId)}.md`,page.content));
    }
    if (metadataBytes > 8 * 1024 * 1024) throw new Error("Archive metadata exceeds bounded ownership limit");
    for (const [id, selection] of selected) {
      if (!references.has(id)) throw new Error("Archive contains an unreferenced attachment selection");
      const metadata = JSON.parse(selection.selected.rawVersion);
      entries.push(text(`metadata/${id}.json`,selection.selected.rawVersion));
      entries.push({name:`assets/${id}.bin`,byteLength:metadata.byte_length,transfer:async write=>{
        const owned = await prepareBrowserAttachmentExport(selection.selected,
          {write,abort:async()=>{},close:async()=>{}},selection.refresh);
        await owned.publish();
      }});
    }
    const fresh = async () => {
      await verifySession();
      for (const page of pages) {await page.verifyCurrent();await verifySession();}
      for (const {selected: original,refresh} of selections) {
        const current = await refresh();await verifySession();
        if (current.rawVersion !== original.rawVersion || !sameOfflineReplayOrigin(current.origin,original.origin) ||
          current.serverPage.space_id !== original.serverPage.space_id || current.serverPage.document_id !== original.serverPage.document_id)
          throw new Error("Archive selected attachment association changed");
      }
    };
    await fresh();
    delegated = true;
    const prepared = await prepareStreamingZip(entries,destination,verifySession);
    try {await fresh();} catch(error) {await prepared.discard();throw error;}
    return {byteLength:prepared.byteLength,discard:prepared.discard,publish:async()=>{
      try {await fresh();} catch(error) {await prepared.discard();throw error;}
      await prepared.publish();
    }};
  } catch(error) {
    if (!delegated) await destination.abort(error).catch(()=>undefined);
    throw error;
  }
}
