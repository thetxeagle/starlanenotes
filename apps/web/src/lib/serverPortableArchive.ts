import { selectedPublishedMetadata, coverAttachmentReferences } from "./publishedCover";
import { api, captureOfflineServerContext, getApiBase } from "./api";
import { verifyOfflineReplayContext } from "./offlineReplayOrigin";
import { fetchExactAttachmentVersion } from "./nativeAttachmentProduct";
import { portableAttachmentReferences } from "./portableAttachmentReferences";
import { prepareBrowserPortableArchive, type PortableArchivePage, type PortableArchiveSelection } from "./browserPortableArchive";
import type { ArchiveWritable, PreparedArchive } from "./streamingZip";
/** Original-session bounded published-page ownership. Caller selects destination before invoking. */
export async function prepareServerSpacePortableArchive(spaceId: string, destination: ArchiveWritable): Promise<PreparedArchive> {
  let delegated = false;
  try {
    if (!spaceId || new TextEncoder().encode(spaceId).length > 256) throw new Error("Archive space identity invalid");
    const context = captureOfflineServerContext();
    const verify = () => verifyOfflineReplayContext(context, getApiBase(), api.getToken());
    const get = async (path: string, maximum: number): Promise<string> => {
      verify();
      const response = await fetch(context.origin.serverBase + path, {headers:{Authorization:`Bearer ${context.bearer}`},credentials:"omit",redirect:"error"});
      verify();
      if (!response.ok || !response.body) {await response.body?.cancel();throw new Error("Archive original current page unavailable");}
      const reader = response.body.getReader(),decoder = new TextDecoder("utf-8",{fatal:true});let length = 0,result = "";
      try {for (;;) {const next = await reader.read();verify();if(next.done)break;length += next.value.length;if(length > maximum)throw new Error("Archive page metadata exceeds bounded ownership limit");result += decoder.decode(next.value,{stream:true});}result += decoder.decode();return result;}
      finally {await reader.cancel().catch(()=>undefined);reader.releaseLock();}
    };
    const prefix = `/spaces/${encodeURIComponent(spaceId)}/documents`;
    const listingPath = prefix + "?include_nested=true";
    const listing = await get(listingPath,1024*1024),records:unknown = JSON.parse(listing);
    if (!Array.isArray(records) || records.length > 1024) throw new Error("Archive requires a bounded complete page listing");
    const verifyListing = async () => {if(await get(listingPath,1024*1024)!==listing)throw new Error("Archive original page listing changed");};
    const pages:PortableArchivePage[] = [],selections = new Map<string,PortableArchiveSelection>(),ids = new Set<string>();let sourceBytes = 0;
    for (const record of records) {
      const id = record?.id;
      if(typeof id!=="string" || !id || new TextEncoder().encode(id).length>256 || ids.has(id))throw new Error("Archive exact page identity invalid");
      ids.add(id);
      const path = `${prefix}/${encodeURIComponent(id)}/content`,raw = await get(path,8*1024*1024-sourceBytes),content = JSON.parse(raw);
      sourceBytes += new TextEncoder().encode(raw).length;
      if(content.id!==id || typeof content.content!=="string" || typeof content.published_source_digest!=="string" || !/^[0-9a-f]{64}$/.test(content.published_source_digest))
        throw new Error("Archive requires exact proven published page source");
      const publishedMetadata=selectedPublishedMetadata(content.published_metadata,content.title);
      pages.push({documentId:id,publishedMetadata,content:content.content,verifyCurrent:async()=>{if(await get(path,8*1024*1024)!==raw)throw new Error("Archive original published page changed");}});
      for(const versionId of new Set([...portableAttachmentReferences(content.content,context.origin.serverBase).values(),...coverAttachmentReferences(publishedMetadata.cover_image,context.origin.serverBase).values()])) {
        if(selections.has(versionId))continue;
        if(selections.size>=1024)throw new Error("Archive selected attachment count exceeds bound");
        verify();const serverPage = {space_id:spaceId,document_id:id};
        const selected = await fetchExactAttachmentVersion(serverPage,versionId);verify();
        selections.set(versionId,{selected,refresh:async()=>{verify();const current=await fetchExactAttachmentVersion(serverPage,versionId);verify();return current;}});
      }
    }
    await verifyListing();
    delegated = true;
    const prepared = await prepareBrowserPortableArchive(pages,[...selections.values()],destination);
    try {await verifyListing();}catch(error){await prepared.discard();throw error;}
    return {byteLength:prepared.byteLength,discard:prepared.discard,publish:async()=>{
      try {await verifyListing();}catch(error){await prepared.discard();throw error;}
      await prepared.publish();
    }};
  } catch(error) {if(!delegated)await destination.abort(error).catch(()=>undefined);throw error;}
}
