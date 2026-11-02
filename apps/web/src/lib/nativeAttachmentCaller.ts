import { api, captureOfflineServerContext, getApiBase } from "./api";
import { isOfflineReplayOrigin, sameOfflineReplayOrigin, verifyOfflineReplayContext, type OfflineReplayOrigin } from "./offlineReplayOrigin";

interface Destination { root_path: string; root: {device:number;inode:number}; database:{device:number;inode:number} }
interface Request {operation_id:string;destination:Destination;origin:OfflineReplayOrigin;server_version_raw:string;association:{space_id:string;document_id:string}}
interface LegacyPending {format:1;request:Request;downloadPath:string;expectedLength:number}
interface RetainedPending {format:2;request:Request;downloadPath:string;expectedLength:number;serverSelection:{space_id:string;document_id:string;logical_attachment_id:string;version_id:string}}
type Pending=LegacyPending|RetainedPending;
const RETAINED_KEY="starlane_native_attachment_download_v2:";
export interface CurrentServerAttachmentPage {space_id:string;document_id:string}
const KEY="starlane_native_attachment_download_v1:";
const CHUNK=65536;
const encoder=new TextEncoder();
function exactVersion(raw: string): { downloadPath: string; expectedLength: number } {
  const version = JSON.parse(raw) as Record<string, unknown>;
  if (!version || typeof version.version_id !== "string" || !version.version_id ||
    typeof version.original_storage_path !== "string" || !version.original_storage_path.startsWith("spaces/") ||
    version.original_storage_path.split("/").some(part => !part || part === "." || part === "..") ||
    !Number.isSafeInteger(version.byte_length) || (version.byte_length as number) < 0 || (version.byte_length as number) > 104857600) {
    throw new Error("Exact server version is required");
  }
  return { downloadPath: "/files/" + version.original_storage_path.split("/").map(encodeURIComponent).join("/"), expectedLength: version.byte_length as number };
}
function selectedVersion(raw:string) {
 const v=JSON.parse(raw) as Record<string,unknown>;
 const id=(x:unknown):x is string=>typeof x==='string'&&/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(x);
 if(!v||!id(v.version_id)||!id(v.logical_attachment_id)||!Number.isSafeInteger(v.version_number)||(v.version_number as number)<1||!Number.isSafeInteger(v.byte_length)||(v.byte_length as number)<0||(v.byte_length as number)>104857600||typeof v.binary_sha256!=='string'||!/^[a-f0-9]{64}$/.test(v.binary_sha256))throw new Error('Exact immutable history version required');
 for(const field of ['original_space_id','original_document_id','uploader_id','original_name','mime_type','original_created_at','original_storage_path','original_filename','upload_operation_id','upload_attempt_id','provenance'])if(typeof v[field]!=='string'||!v[field])throw new Error('Complete immutable history record required');
 for(const field of ['raw_receipt_sha256','intent_sha256','install_proof_sha256'])if(typeof v[field]!=='string'||!/^[a-f0-9]{64}$/.test(v[field] as string))throw new Error('Original history proof binding required');
 return {version_id:v.version_id,logical_attachment_id:v.logical_attachment_id,expectedLength:v.byte_length as number};
}
function retainedPath(page:CurrentServerAttachmentPage,version:{logical_attachment_id:string;version_id:string}) {
 if(!page||typeof page.space_id!=='string'||!page.space_id||typeof page.document_id!=='string'||!page.document_id)throw new Error('Selected current server page required');
 return `/spaces/${encodeURIComponent(page.space_id)}/documents/${encodeURIComponent(page.document_id)}/attachment-families/${version.logical_attachment_id}/versions/${version.version_id}/file`;
}
function originalPending(operationId:string):{raw:string;pending:Pending} {
 const legacy=localStorage.getItem(KEY+operationId),retained=localStorage.getItem(RETAINED_KEY+operationId);
 if(legacy&&retained)throw new Error('Ambiguous original pending download; retain for review');
 const raw=legacy??retained;
 if(!raw||encoder.encode(raw).length>64*1024)throw new Error('Original download request missing or oversized');
 const pending=validatePending(JSON.parse(raw),operationId);
 if((legacy&&pending.format!==1)||(retained&&pending.format!==2))throw new Error('Original pending protocol differs; retain for review');
 return {raw,pending};
}
function pendingUnchanged(p:Pending,operationId:string,raw:string){return localStorage.getItem((p.format===1?KEY:RETAINED_KEY)+operationId)===raw;}
function validatePending(value: unknown, operationId: string): Pending {
  const p = value as Pending;
  if (!p || (p.format !== 1 && p.format !== 2) || !p.request || p.request.operation_id !== operationId ||
    !isOfflineReplayOrigin(p.request.origin) || typeof p.request.server_version_raw !== "string" ||
    !p.request.association || typeof p.request.association.space_id !== "string" || !p.request.association.space_id ||
    typeof p.request.association.document_id !== "string" || !p.request.association.document_id ||
    !p.request.destination || typeof p.request.destination.root_path !== "string" ||
    !p.request.destination.root || !p.request.destination.database ||
    ![p.request.destination.root.device, p.request.destination.root.inode, p.request.destination.database.device, p.request.destination.database.inode].every(Number.isSafeInteger) ||
    encoder.encode(JSON.stringify(p.request)).length > 60 * 1024) throw new Error("Original download request invalid; retain for review");
  if(p.format===2){
   const version=selectedVersion(p.request.server_version_raw);
   if(!p.serverSelection||p.serverSelection.logical_attachment_id!==version.logical_attachment_id||p.serverSelection.version_id!==version.version_id||p.downloadPath!==retainedPath(p.serverSelection,version)||p.expectedLength!==version.expectedLength)throw new Error('Original selected history route binding differs; retain for review');
   return p;
  }
  const derived = exactVersion(p.request.server_version_raw);
  if (derived.downloadPath !== p.downloadPath || derived.expectedLength !== p.expectedLength) throw new Error("Original download byte binding differs; retain for review");
  return p;
}

async function invoke<T>(cmd:string,args:Record<string,unknown>):Promise<T> {
  const internals=(window as unknown as {__TAURI_INTERNALS__?:{invoke:<T>(cmd:string,args:Record<string,unknown>)=>Promise<T>}}).__TAURI_INTERNALS__;
  if (!internals?.invoke) throw new Error("Owned attachment delivery requires the updated Linux native shell");
  return internals.invoke<T>(cmd,args);
}
/** The shell explicitly advertises support; Tauri presence alone is insufficient. */
export async function supportsNativeAttachmentDownloads(): Promise<boolean> {
  try {
    const capability = await invoke<unknown>("get_native_attachment_capabilities", {});
    return !!capability && typeof capability === "object" &&
      (capability as Record<string, unknown>).format === 1 &&
      (capability as Record<string, unknown>).bounded_version_download === true;
  } catch {
    return false;
  }
}
/** Persist metadata only, before fetching any private bytes. No credential is saved. */
export async function prepareNativeAttachmentCaller(localSpaceId:string,localDocumentId:string,rawVersion:string,selectedOrigin:OfflineReplayOrigin):Promise<string> {
  const context=captureOfflineServerContext();
  // The selected version belongs to its original read context. A fresh current
  // session must not silently adopt metadata selected on another server/account.
  if (!sameOfflineReplayOrigin(selectedOrigin, context.origin)) throw new Error("Selected server version context changed; retain the original selection");
  const verify=()=>verifyOfflineReplayContext(context,getApiBase(),api.getToken());
  verify();
  const version=exactVersion(rawVersion);
  const supported = await supportsNativeAttachmentDownloads();
  verify();
  if (!supported) throw new Error("This native shell does not support bounded attachment downloads");
  const destination=await invoke<Destination>("capture_native_attachment_destination",{provider:context.origin.provider});
  verify();
  if (![destination.root.device,destination.root.inode,destination.database.device,destination.database.inode].every(Number.isSafeInteger)) throw new Error("Native destination identity cannot be represented exactly by this caller");
  const request:Request={operation_id:crypto.randomUUID(),destination,origin:context.origin,server_version_raw:rawVersion,association:{space_id:localSpaceId,document_id:localDocumentId}};
  if(encoder.encode(JSON.stringify(request)).length>60*1024) throw new Error("Native request envelope is too large");
  const pending:Pending={format:1,request,expectedLength:version.expectedLength,downloadPath:version.downloadPath};
  const raw=JSON.stringify(pending);
  localStorage.setItem(KEY+request.operation_id,raw);
  if(localStorage.getItem(KEY+request.operation_id)!==raw) throw new Error("Original native download identity could not be retained");
  verify();
  return request.operation_id;
}
/** Exact original request survives caller restart. Server validates the bearer. */
export async function runNativeAttachmentCaller(operationId:string):Promise<unknown> {
  const {raw,pending:p}=originalPending(operationId);
  const context=captureOfflineServerContext();
  if(p.request.operation_id!==operationId || !sameOfflineReplayOrigin(p.request.origin,context.origin)) throw new Error("Original download context differs; retain for review");
  const verify=()=>{verifyOfflineReplayContext(context,getApiBase(),api.getToken());if(!pendingUnchanged(p,operationId,raw))throw new Error("Original caller request changed");};
  const args={request:p.request,current:context.origin};
  verify();
  const supported = await supportsNativeAttachmentDownloads();
  verify();
  if (!supported) throw new Error("This native shell does not support bounded attachment downloads; original work retained");
  const prior=await invoke<unknown|null>("prepare_native_attachment_download",args);
  verify();
  if(prior!==null) return prior;
  const response=await fetch(context.origin.serverBase+p.downloadPath,{headers:{Authorization:`Bearer ${context.bearer}`},credentials:"omit",redirect:"error"});
  verify();
  if(!response.ok || !response.body) throw new Error("Private attachment delivery refused; original work retained");
  const reader=response.body.getReader();
  const buffer=new Uint8Array(CHUNK);
  let used=0,index=0,total=0;
  const emit=async()=>{verify();await invoke<void>("append_native_attachment_download",{...args,index,bytes:Array.from(buffer.subarray(0,used))});verify();index++;used=0;};
  try {
    for(;;){const next=await reader.read();verify();if(next.done)break;const bytes=next.value;total+=bytes.length;if(total>p.expectedLength)throw new Error("Attachment length differs");
      for(let offset=0;offset<bytes.length;){const count=Math.min(CHUNK-used,bytes.length-offset);buffer.set(bytes.subarray(offset,offset+count),used);used+=count;offset+=count;if(used===CHUNK)await emit();}}
    if(total!==p.expectedLength)throw new Error("Attachment truncated");
    if(used)await emit();
    verify();const result=await invoke<unknown>("finish_native_attachment_download",args);verify();return result;
  } finally {await reader.cancel().catch(()=>undefined);reader.releaseLock();}
}

/** Fresh selection only: no migration or retargeting of legacy /files pending. */
export async function prepareRetainedNativeAttachmentCaller(localSpaceId:string,localDocumentId:string,serverPage:CurrentServerAttachmentPage,rawVersion:string,selectedOrigin:OfflineReplayOrigin):Promise<string>{
 const context=captureOfflineServerContext();
 if(!sameOfflineReplayOrigin(selectedOrigin,context.origin))throw new Error('Selected server version context changed; retain original selection');
 const verify=()=>verifyOfflineReplayContext(context,getApiBase(),api.getToken());verify();
 const version=selectedVersion(rawVersion);const downloadPath=retainedPath(serverPage,version);
 const selection={space_id:serverPage.space_id,document_id:serverPage.document_id,logical_attachment_id:version.logical_attachment_id,version_id:version.version_id};
 const supported=await supportsNativeAttachmentDownloads();verify();if(!supported)throw new Error('This native shell does not support bounded attachment downloads');
 const destination=await invoke<Destination>('capture_native_attachment_destination',{provider:context.origin.provider});verify();
 const request:Request={operation_id:crypto.randomUUID(),destination,origin:context.origin,server_version_raw:rawVersion,association:{space_id:localSpaceId,document_id:localDocumentId}};
 const pending:RetainedPending={format:2,request,serverSelection:selection,downloadPath,expectedLength:version.expectedLength};
 validatePending(pending,request.operation_id);const raw=JSON.stringify(pending);if(encoder.encode(raw).length>64*1024)throw new Error('Original selected history request oversized');
 localStorage.setItem(RETAINED_KEY+request.operation_id,raw);if(!pendingUnchanged(pending,request.operation_id,raw))throw new Error('Original selected history could not be retained');verify();return request.operation_id;
}
/** Existing persisted operation is the sole delivery binding, including its original protocol. */
export function captureNativeAttachmentDelivery(operationId:string){
 const {raw,pending}=originalPending(operationId),context=captureOfflineServerContext();
 const verify=()=>{verifyOfflineReplayContext(context,getApiBase(),api.getToken());if(!pendingUnchanged(pending,operationId,raw)||!sameOfflineReplayOrigin(pending.request.origin,context.origin))throw new Error('Original native delivery context changed; request retained');};
 verify();return {request:pending.request,current:context.origin,verify};
}
/** Preview/export handlers are additive capabilities; old shells must refuse before dispatch. */
export async function supportsNativeAttachmentDelivery():Promise<boolean>{
 try{const c=await invoke<Record<string,unknown>>('get_native_attachment_capabilities',{});return !!c&&c.format===1&&c.bounded_version_preview===true&&c.receipt_bound_export===true;}catch{return false;}
}
