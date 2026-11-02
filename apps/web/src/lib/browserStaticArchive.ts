import { coverAttachmentReferences } from "./publishedCover";
import { api, captureOfflineServerContext, getApiBase } from "./api";
import { sameOfflineReplayOrigin, verifyOfflineReplayContext } from "./offlineReplayOrigin";
import { prepareBrowserAttachmentExport, type BrowserSelectedAttachmentVersion } from "./browserAttachmentExport";
import { portableAttachmentReferences, verifyPortableAttachmentReference } from "./portableAttachmentReferences";
import type { LocalVaultSnapshot } from "./localVault";
import { buildOwnedStaticSiteTextPlan, type StaticSiteExportOptions, type OwnedStaticSiteAsset, type StaticSiteIntegrityRecord } from "./staticSiteExport";
import { prepareStreamingZip, type ArchiveEntry, type ArchiveWritable, type PreparedArchive } from "./streamingZip";
interface PortableArchivePage {
  publishedMetadata?: import("@starlane-notes/shared-types").PublishedDocumentMetadata;
  documentId: string;
  content: string;
  /** Re-read original page content and access; reject replacement/revision/context changes. */
  verifyCurrent(): Promise<void>;
}
interface PortableArchiveSelection {
  selected: BrowserSelectedAttachmentVersion;
  refresh(): Promise<BrowserSelectedAttachmentVersion>;
}
/** Streamed static site core. Source enumeration and chooser remain caller-owned. */
export async function prepareBrowserStaticArchive(snapshot: LocalVaultSnapshot, options: StaticSiteExportOptions, pages: readonly PortableArchivePage[],
  selections: readonly PortableArchiveSelection[], destination: ArchiveWritable): Promise<PreparedArchive> {
  let delegated = false;
  try {
    const frozenSnapshot=JSON.stringify(snapshot);
    if(new TextEncoder().encode(frozenSnapshot).length>16*1024*1024)throw new Error("Static archive snapshot metadata exceeds bounded ownership");
    snapshot=JSON.parse(frozenSnapshot);
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
    const assets = new Map<string,OwnedStaticSiteAsset>();
    if(snapshot.documents.length!==pages.length || snapshot.assets.length!==0 || snapshot.documents.some(doc=>!pages.some(page=>page.documentId===doc.id && page.content===doc.content)))
      throw new Error("Static archive text must match exact original published pages");
    const seenPages=new Set<string>();
    for (const page of pages) {
      if(seenPages.has(page.documentId))throw new Error("Static archive requires unique original pages");
      seenPages.add(page.documentId);
      await verifySession(); await page.verifyCurrent(); await verifySession();
      metadataBytes += new TextEncoder().encode(page.content).length;
      if (metadataBytes > 8 * 1024 * 1024) throw new Error("Archive metadata exceeds bounded ownership limit");

      for (const [reference,id] of new Map([...portableAttachmentReferences(page.content,context.origin.serverBase),...coverAttachmentReferences(page.publishedMetadata?.cover_image,context.origin.serverBase)])) {
        if (!selected.has(id)) throw new Error("Archive referenced exact attachment bytes unavailable");
        verifyPortableAttachmentReference(reference,selected.get(id)!.selected.rawVersion,context.origin.serverBase);
        references.add(id);
        let asset=assets.get(id);
        if(!asset) {
          const raw=JSON.parse(selected.get(id)!.selected.rawVersion);
          const extension=typeof raw.original_filename==="string" ? raw.original_filename.split(".").pop() : undefined;
          if(!extension || !/^[A-Za-z0-9]{1,16}$/.test(extension) || typeof raw.original_name!=="string" || typeof raw.mime_type!=="string")
            throw new Error("Static archive exact original asset descriptor required");
          asset={id,name:raw.original_name,path:`assets/${id}.${extension.toLowerCase()}`,media_type:raw.mime_type,size:raw.byte_length,references:[]};
          assets.set(id,asset);
        }
        asset.references=[...asset.references,reference];
      }

      if (page.publishedMetadata) {
        const doc=snapshot.documents.find(d=>d.id===page.documentId);
        if (!doc || doc.cover_image !== (page.publishedMetadata.cover_image ?? undefined) || doc.icon !== (page.publishedMetadata.icon ?? undefined)) throw new Error("Static archive selected presentation differs");
        entries.push(text(`sources/${encodeURIComponent(page.documentId)}.metadata.json`,JSON.stringify(page.publishedMetadata)));
      }
      // Preserve source reference bytes separately from rewritten portable Markdown.
      entries.push(text(`sources/${encodeURIComponent(page.documentId)}.md`,page.content));
    }
    if (metadataBytes > 8 * 1024 * 1024) throw new Error("Archive metadata exceeds bounded ownership limit");
    for (const [id, selection] of selected) {
      if (!references.has(id)) throw new Error("Archive contains an unreferenced attachment selection");
      const metadata = JSON.parse(selection.selected.rawVersion);
      entries.push(text(`metadata/${id}.json`,selection.selected.rawVersion));
      entries.push({name:assets.get(id)!.path,byteLength:metadata.byte_length,transfer:async write=>{
        const owned = await prepareBrowserAttachmentExport(selection.selected,
          {write,abort:async()=>{},close:async()=>{}},selection.refresh);
        await owned.publish();
      }});
    }
    const plan=buildOwnedStaticSiteTextPlan(snapshot,[...assets.values()],options);
    let textBytes=0;
    for(const file of plan.bundle.files) {
      if(typeof file.content!=="string")throw new Error("Static archive text plan materialized binary bytes");
      textBytes+=new TextEncoder().encode(file.content).length;
    }
    if(textBytes>16*1024*1024)throw new Error("Static archive rendered text exceeds bounded ownership");
    const integrity=new Map<string,StaticSiteIntegrityRecord>();
    const table=new Uint32Array(256);
    for(let n=0;n<256;n++){let c=n;for(let k=0;k<8;k++)c=c&1?0xedb88320^(c>>>1):c>>>1;table[n]=c;}
    const measured=(entry:ArchiveEntry,media_type:string):ArchiveEntry=>({...entry,transfer:async write=>{
      let crc=0xffffffff,size=0;
      await entry.transfer(async bytes=>{for(const byte of bytes)crc=table[(crc^byte)&255]^(crc>>>8);size+=bytes.length;await write(bytes);});
      integrity.set(entry.name,{path:entry.name,media_type,size,checksum_crc32:((crc^0xffffffff)>>>0).toString(16).padStart(8,"0")});
    }});
    const textEntries=plan.bundle.files.map(file=>measured(text(file.path,file.content as string),file.media_type));
    const bodyEntries=entries.map(entry=>{const asset=[...assets.values()].find(asset=>asset.path===entry.name);return asset ? measured(entry,asset.media_type) : entry;});
    const records=()=>plan.descriptors.map(descriptor=>{
      const record=integrity.get(descriptor.path);if(!record)throw new Error("Static archive actual asset integrity missing");return record;
    });
    // Planning-only fixed-width CRC fields establish lengths; these placeholder bytes are never emitted.
    const plannedTail=plan.finish(plan.descriptors.map(descriptor=>({...descriptor,checksum_crc32:"00000000"})));
    const tails=plannedTail.map((file,index)=>({name:file.path,byteLength:new TextEncoder().encode(file.content as string).length,transfer:async(write:(bytes:Uint8Array)=>Promise<void>)=>{
      const actual=plan.finish(records())[index];await write(new TextEncoder().encode(actual.content as string));
    }}));
    entries.splice(0,entries.length,...textEntries,...bodyEntries,...tails);
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
