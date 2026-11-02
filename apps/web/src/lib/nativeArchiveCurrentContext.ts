import { api, getApiBase, isTauri } from "./api";
import { getCurrentLaunchMode } from "./launchMode";
import { getStorageSelection } from "./storageProviders";
import { LOCAL_MODE_ACCOUNT_ID } from "./localModeIdentity";
import { prepareNativeArchiveOutcomeReview, type NativeArchiveReviewBinding } from "./nativeArchiveOutcomeReview";
const KEY = "starlane_native_archive_local_session_v1";
const uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const equal = (a: unknown, b: unknown) => JSON.stringify(a, (_key, value: unknown) => value && typeof value === "object" && !Array.isArray(value) ? Object.fromEntries(Object.entries(value).sort(([x], [y]) => x.localeCompare(y))) : value) === JSON.stringify(b, (_key, value: unknown) => value && typeof value === "object" && !Array.isArray(value) ? Object.fromEntries(Object.entries(value).sort(([x], [y]) => x.localeCompare(y))) : value);
export interface ImportedAttachmentSelection { space_id: string; document_id: string; asset_id: string }
async function invoke(command: string, args: Record<string, unknown>): Promise<unknown> {
 const native = (window as unknown as {__TAURI_INTERNALS__?: {invoke(c: string, a: Record<string, unknown>): Promise<unknown>}}).__TAURI_INTERNALS__;
 if (!native?.invoke) throw new Error("Updated native canonical archive consumer unavailable");
 return native.invoke(command,args);
}
function checkedBinding(value: unknown, context: NativeArchiveReviewBinding["context"]): NativeArchiveReviewBinding {
 const b=value as NativeArchiveReviewBinding;
 if (!b || Object.keys(b).length!==3 || !equal(b.context,context) || typeof b.root!=="string" || !b.root.startsWith("/") || b.root==="/" || b.root.endsWith("/") || b.root.split("/").slice(1).some(p=>!p||p==="."||p==="..") || typeof b.revision!=="string" || !b.revision || b.revision.length>256) throw new Error("Original native canonical archive binding malformed");
 return copy(b);
}
/** Real local app actor and retained local epoch. This is not server authentication. */
export function captureNativeArchiveLocalSession() {
 const storage=getStorageSelection();
 if (!isTauri() || getCurrentLaunchMode()!=="local" || storage.mode!=="local") throw new Error("Current native local vault required");
 let raw=localStorage.getItem(KEY);
 if (raw===null) {
  if (!crypto.randomUUID) throw new Error("Native archive local session identity unavailable");
  raw=JSON.stringify({version:1,sessionId:crypto.randomUUID()});localStorage.setItem(KEY,raw);
  if(localStorage.getItem(KEY)!==raw)throw new Error("Native archive local session could not be retained");
 }
 let epoch:{version:number;sessionId:string};
 try{epoch=JSON.parse(raw);}catch{throw new Error("Retained native archive session malformed; retain archive evidence");}
 if (!epoch || Object.keys(epoch).length!==2 || epoch.version!==1 || !uuid.test(epoch.sessionId)) throw new Error("Retained native archive session malformed; retain archive evidence");
 const selection=JSON.stringify(storage),base=getApiBase(),token=api.getToken(),refresh=api.getRefreshToken(),diskToken=localStorage.getItem("access_token"),diskRefresh=localStorage.getItem("refresh_token"),scope=localStorage.getItem("starlane_server_cache_v1:session-scope");
 const context:NativeArchiveReviewBinding["context"]={version:1,serverBase:base,accountId:LOCAL_MODE_ACCOUNT_ID,sessionId:epoch.sessionId,cacheScope:`local-archive-${epoch.sessionId}`,storageMode:"local",provider:storage.provider};
 const verify=()=>{
  if(!isTauri() || getCurrentLaunchMode()!=="local" || JSON.stringify(getStorageSelection())!==selection || getApiBase()!==base || api.getToken()!==token || api.getRefreshToken()!==refresh || localStorage.getItem("access_token")!==diskToken || localStorage.getItem("refresh_token")!==diskRefresh || localStorage.getItem("starlane_server_cache_v1:session-scope")!==scope || localStorage.getItem(KEY)!==raw)throw new Error("Original native archive local session changed; retain evidence");
 };
 verify();
 return {context:copy(context),verify,async describe():Promise<NativeArchiveReviewBinding>{
  verify();const response=await invoke("describe_current_native_vault_archive",{provider:context.provider,context:copy(context),current:copy(context)});verify();
  if(new TextEncoder().encode(JSON.stringify(response)).length>65536)throw new Error("Native canonical descriptor exceeds bound");
  const r=response as {format?:unknown;binding?:unknown;initialized?:unknown;authority?:unknown;export_authority?:unknown;retry_authority?:unknown;serve_authority?:unknown};
  if(r?.format!==1 || r.initialized!==true || r.authority!=="local_archive_provenance" || r.export_authority!==false || r.retry_authority!==false || r.serve_authority!==false)throw new Error("Native canonical descriptor unavailable");
  return checkedBinding(r.binding,context);
 },async preview(binding:NativeArchiveReviewBinding,selected:ImportedAttachmentSelection,offset:number,length:number):Promise<{bytes:Uint8Array;byteLength:number;rawVersion:string}>{
  verify();const original=checkedBinding(binding,context),target=copy(selected);
  if(Object.keys(target).length!==3 || ![target.space_id,target.document_id,target.asset_id].every(v=>typeof v==="string"&&v.length>0&&v.length<=256) || !Number.isSafeInteger(offset)||offset<0||!Number.isSafeInteger(length)||length<0||length>65536)throw new Error("Exact bounded imported attachment preview required");
  const response=await invoke("read_imported_native_attachment_preview",{provider:context.provider,context:copy(context),current:copy(context),selection:target,originalBinding:copy(original),offset,length});verify();
  if(new TextEncoder().encode(JSON.stringify(response)).length>1024*1024)throw new Error("Imported preview response exceeds bound");
  const r=response as {format?:unknown;binding?:unknown;selection?:unknown;offset?:unknown;bytes?:unknown;byte_length?:unknown;raw_version_record?:unknown};
  if(r?.format!==1 || !equal(checkedBinding(r.binding,context),original) || !equal(r.selection,target) || r.offset!==offset || !Array.isArray(r.bytes) || r.bytes.length!==length || !r.bytes.every(b=>Number.isInteger(b)&&b>=0&&b<=255) || !Number.isSafeInteger(r.byte_length) || (r.byte_length as number)<offset+length || (r.byte_length as number)>104857600 || typeof r.raw_version_record!=="string" || new TextEncoder().encode(r.raw_version_record).length>65536)throw new Error("Exact imported preview acknowledgment differs");
  const version=JSON.parse(r.raw_version_record);if(!version || typeof version!=="object" || version.byte_length!==r.byte_length)throw new Error("Original imported raw version metadata differs");
  verify();return{bytes:Uint8Array.from(r.bytes),byteLength:r.byte_length as number,rawVersion:r.raw_version_record};
 }};
}

/** Current physical root/session must match; a retained review revision stays historical. */
export async function openCurrentNativeArchiveOutcomeReview(parentPath:string,retainedBinding?:NativeArchiveReviewBinding){
 const session=captureNativeArchiveLocalSession(),current=await session.describe();session.verify();
 const original=retainedBinding?checkedBinding(copy(retainedBinding),session.context):current;
 if(original.root!==current.root)throw new Error("Retained archive review belongs to another current vault");
 const review=prepareNativeArchiveOutcomeReview(original,parentPath);
 return{binding:copy(original),async pending(){session.verify();const rows=await review.pending();session.verify();return rows;},async acknowledgeNamedDestination(operation:string,destination:string){session.verify();await review.acknowledgeNamedDestination(operation,destination);try{session.verify();}catch{throw new Error("Native archive review acknowledgment unknown after local session change; retain evidence");}}};
}
export async function previewCurrentImportedNativeAttachmentText(selected:ImportedAttachmentSelection,byteLength:number){
 if(!Number.isSafeInteger(byteLength)||byteLength<0||byteLength>104857600)throw new Error("Original imported length required");
 const session=captureNativeArchiveLocalSession(),binding=await session.describe();session.verify();
 const result=await session.preview(binding,copy(selected),0,Math.min(byteLength,65536));session.verify();
 if(result.byteLength!==byteLength)throw new Error("Original imported length changed");
 return{text:new TextDecoder().decode(result.bytes),truncated:byteLength>result.bytes.length,rawVersion:result.rawVersion};
}
