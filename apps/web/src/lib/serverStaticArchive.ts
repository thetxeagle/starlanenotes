import { selectedPublishedMetadata, coverAttachmentReferences } from "./publishedCover";
import { api, captureOfflineServerContext, getApiBase } from "./api";
import { verifyOfflineReplayContext } from "./offlineReplayOrigin";
import { fetchExactAttachmentVersion } from "./nativeAttachmentProduct";
import { portableAttachmentReferences } from "./portableAttachmentReferences";
import { type PortableArchivePage, type PortableArchiveSelection } from "./browserPortableArchive";
import { prepareBrowserStaticArchive } from "./browserStaticArchive";
import type {LocalVaultSnapshot} from "./localVault";
import type {StaticSiteExportOptions} from "./staticSiteExport";
import type { ArchiveWritable, PreparedArchive } from "./streamingZip";
/** Original-session bounded published-page ownership. Caller selects destination before invoking. */
export async function prepareServerSpaceStaticArchive(spaceId: string, destination: ArchiveWritable, options: StaticSiteExportOptions = {}): Promise<PreparedArchive> {
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
    const now=new Date().toISOString();
    const snapshot:LocalVaultSnapshot={schema_version:1,spaces:[],folders:[],documents:[],assets:[],templates:[],workflow_templates:[],document_workflows:[],document_workflow_events:[],property_schemas:[],property_values:[],tags:[],doc_tags:[],search_index:[],sync_state:{state:"idle",updated_at:now,queued_operations:[]}};
    let metadataBytes=0;
    const captured=new Map<string,string>();
    const metadata=async(path:string):Promise<unknown>=>{
      const raw=await get(path,1024*1024);metadataBytes+=new TextEncoder().encode(raw).length;
      if(metadataBytes>8*1024*1024)throw new Error("Static archive metadata exceeds bounded ownership");
      captured.set(path,raw);return JSON.parse(raw);
    };
    const readSpace=async()=>{const records:unknown=JSON.parse(await get("/spaces",1024*1024));if(!Array.isArray(records))throw new Error("Static archive complete space catalog unavailable");const record=records.find(record=>record?.id===spaceId);if(!record || typeof record.name!=="string" || typeof record.slug!=="string")throw new Error("Static archive original space unavailable");return record;};
    const space=await readSpace(),spaceRaw=JSON.stringify(space);
    snapshot.spaces.push({id:spaceId,name:space.name,slug:space.slug,created_at:now,updated_at:now});
    const ownedArray=async(path:string)=>{const records=await metadata(path);if(!Array.isArray(records) || records.length>1024 || records.some(record=>!record || typeof record.id!=="string"))throw new Error("Static archive bounded exact metadata listing unavailable");return records;};
    snapshot.folders=(await ownedArray(`/spaces/${encodeURIComponent(spaceId)}/folders`)).map(record=>({...record,space_id:spaceId}));
    snapshot.tags=(await ownedArray(`/spaces/${encodeURIComponent(spaceId)}/tags`)).map(record=>({...record,space_id:spaceId}));
    snapshot.property_schemas=(await ownedArray(`/spaces/${encodeURIComponent(spaceId)}/properties`)).map(record=>({...record,space_id:spaceId}));
    const verifyMetadata=async()=>{verify();if(JSON.stringify(await readSpace())!==spaceRaw)throw new Error("Static archive original space changed");for(const [path,raw] of captured)if(await get(path,1024*1024)!==raw)throw new Error("Static archive original page metadata changed");verify();};
    const pages:PortableArchivePage[] = [],selections = new Map<string,PortableArchiveSelection>(),ids = new Set<string>();let sourceBytes = 0;
    for (const record of records) {
      const id = record?.id;
      if(typeof id!=="string" || !id || new TextEncoder().encode(id).length>256 || ids.has(id))throw new Error("Archive exact page identity invalid");
      ids.add(id);
      const path = `${prefix}/${encodeURIComponent(id)}/content`,raw = await get(path,8*1024*1024-sourceBytes),content = JSON.parse(raw);
      sourceBytes += new TextEncoder().encode(raw).length;
      if(content.id!==id || typeof content.content!=="string" || typeof content.published_source_digest!=="string" || !/^[0-9a-f]{64}$/.test(content.published_source_digest))
        throw new Error("Archive requires exact proven published page source");
      if(typeof content.title!=="string")throw new Error("Static archive exact original page title unavailable");
      const selectedMetadata=selectedPublishedMetadata(content.published_metadata,content.title);
      snapshot.documents.push({...record,id,space_id:spaceId,title:content.title,icon:selectedMetadata.icon ?? undefined,cover_image:selectedMetadata.cover_image ?? undefined,content:content.content,created_at:record.created_at ?? now,updated_at:record.updated_at ?? now});
      const properties=await metadata(`${prefix}/${encodeURIComponent(id)}/properties`),tags=await metadata(`${prefix}/${encodeURIComponent(id)}/tags`);
      if(!Array.isArray(properties) || !Array.isArray(tags))throw new Error("Static archive complete original page metadata unavailable");
      snapshot.property_values!.push(...properties.filter(property=>property.value!==null).map(property=>({id:`${id}:${property.property_id}`,document_id:id,property_id:property.property_id,value:property.value})));
      snapshot.doc_tags!.push(...tags.map(tag=>({document_id:id,tag_id:tag.id})));
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
    await verifyListing();await verifyMetadata();
    delegated = true;
    const prepared = await prepareBrowserStaticArchive(snapshot,options,pages,[...selections.values()],destination);
    try {await verifyListing();await verifyMetadata();}catch(error){await prepared.discard();throw error;}
    return {byteLength:prepared.byteLength,discard:prepared.discard,publish:async()=>{
      try {await verifyListing();await verifyMetadata();}catch(error){await prepared.discard();throw error;}
      await prepared.publish();
    }};
  } catch(error) {if(!delegated)await destination.abort(error).catch(()=>undefined);throw error;}
}
