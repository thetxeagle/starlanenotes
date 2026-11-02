import { captureNativeArchiveLocalSession } from "./nativeArchiveCurrentContext";
import { assertLocalVaultSnapshot, type LocalVaultSnapshot, type VaultAsset } from "./localVault";

const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
const LIMIT = 30 * 1024 * 1024;
const CHUNK = 65536;
const equal=(a:unknown,b:unknown)=>JSON.stringify(a,(_key,value:unknown)=>value&&typeof value==="object"&&!Array.isArray(value)?Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b))):value)===JSON.stringify(b,(_key,value:unknown)=>value&&typeof value==="object"&&!Array.isArray(value)?Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b))):value);
const clone = <T>(value:T):T => JSON.parse(JSON.stringify(value)) as T;
interface OriginalRequest {
 operation_id:string; name:string; mime_type:string; byte_length:number;
 space_id:string; document_id:string; draft_id:string;
 original_binding:Awaited<ReturnType<ReturnType<typeof captureNativeArchiveLocalSession>["describe"]>>;
}
interface Retained {
 format:1; operationId:string; request:OriginalRequest;
 state:"prepared"|"transfer_unknown"|"canonical_committed"|"adopted"|"discard_unknown"|"discarded";
 received:number; blob:Blob|null; acknowledgement:unknown;
}
async function invoke(command:string,args:Record<string,unknown>):Promise<unknown>{
 const native=(window as unknown as {__TAURI_INTERNALS__?:{invoke(c:string,a:Record<string,unknown>):Promise<unknown>}}).__TAURI_INTERNALS__;
 if(!native?.invoke)throw new Error("Updated native local cover creator unavailable");
 try{return await native.invoke(command,args);}catch(error){throw error instanceof Error?error:new Error(typeof error==="string"?error:"Native local cover operation unavailable");}
}
// File bodies are immutable while pending. WebKit can invalidate a persisted
// Blob when a retrieved Blob is written back into its own row after restart.
// v1 originals therefore remain untouched; progress is an additive overlay.
const STORES=["originals","progress","files"];
async function database():Promise<IDBDatabase>{
 return new Promise((resolve,reject)=>{
  if(typeof indexedDB==="undefined"){reject(new Error("Durable original cover journal unavailable"));return;}
  let rejected=false;
  const request=indexedDB.open("starlane-native-local-cover-originals",2);
  request.onupgradeneeded=()=>{for(const name of STORES)if(!request.result.objectStoreNames.contains(name))request.result.createObjectStore(name,{keyPath:"operationId"});};
  request.onsuccess=()=>{const db=request.result;db.onversionchange=()=>db.close();if(rejected){db.close();return;}resolve(db);};
  request.onerror=request.onblocked=()=>{rejected=true;reject(new Error("Durable original cover journal unavailable"));};
 });
}
function journalRow(tx:IDBTransaction,base:Retained,accept:(row:Retained)=>void,refuse:(error:Error)=>void){
 const progress=tx.objectStore("progress").get(base.operationId),file=tx.objectStore("files").get(base.operationId);let remaining=2;
 const ready=()=>{if(--remaining)return;try{
  const overlay=progress.result as Retained|undefined,original=file.result as {operationId:string;request:OriginalRequest;blob:Blob}|undefined;
  if(overlay&&(!equal(overlay.request,base.request)||overlay.operationId!==base.operationId||overlay.blob!==null))throw new Error("Original cover progress differs");
  if(original&&(original.operationId!==base.operationId||!equal(original.request,base.request)||!(original.blob instanceof Blob)||original.blob.size!==base.request.byte_length||original.blob.type!==base.request.mime_type))throw new Error("Original cover File record differs");
  const row={...(overlay??base)};
  row.blob=row.state==="prepared"||row.state==="transfer_unknown"||row.state==="discard_unknown"?(original?.blob??base.blob):null;
  accept(row);
 }catch(error){refuse(error instanceof Error?error:new Error("Original cover journal unavailable"));}};
 progress.onsuccess=ready;file.onsuccess=ready;
}
async function store(row:Retained,initial=false):Promise<void>{
 const db=await database();try{await new Promise<void>((resolve,reject)=>{
  const tx=db.transaction(STORES,"readwrite"),table=tx.objectStore("originals"),request=table.get(row.operationId);
  request.onsuccess=()=>{const old=request.result as Retained|undefined;
   if(initial?!!old:!old||!equal(old.request,row.request)){tx.abort();return;}
   const metadata={...row,blob:null};
   if(initial){
    if(!(row.blob instanceof Blob)||row.blob.size!==row.request.byte_length||row.blob.type!==row.request.mime_type){tx.abort();return;}
    table.add(metadata);tx.objectStore("files").add({operationId:row.operationId,request:row.request,blob:row.blob});
   }else{
    tx.objectStore("progress").put(metadata);
    if(row.state==="canonical_committed"||row.state==="adopted"||row.state==="discarded"){
     // Called only after a checked native winner or explicit checked discard. Legacy bodies can now be
     // released atomically with that acknowledgement, never while resuming.
     tx.objectStore("files").delete(row.operationId);
     if(old?.blob instanceof Blob)table.put(metadata);
    }
   }
  };
  tx.oncomplete=()=>resolve();tx.onabort=tx.onerror=()=>reject(new Error("Original local cover journal could not be durably retained"));
 });}finally{db.close();}
}
export function checkedNativeLocalCoverAcknowledgement(value:unknown,request:OriginalRequest):{asset:VaultAsset;snapshot:LocalVaultSnapshot}{
 if(new TextEncoder().encode(JSON.stringify(value)).length>16*1024*1024)throw new Error("Native cover acknowledgement exceeds bound");
 const r=value as {format?:unknown;operation_id?:unknown;canonical_committed?:unknown;asset?:VaultAsset;canonical_snapshot?:unknown;receipt_json?:unknown};
 assertLocalVaultSnapshot(r.canonical_snapshot);
 const a=r.asset,p=a?.native_local_version_provenance;
 if(Object.keys(r).length!==6||r.format!==1||r.operation_id!==request.operation_id||r.canonical_committed!==true||!a||a.id!==`native-cover-${request.operation_id}`||a.url!==`starlane-local-attachment://${request.operation_id}`||a.name!==request.name||a.size!==request.byte_length||a.mime_type!==request.mime_type||a.space_id!==request.space_id||a.document_id!==request.document_id||a.source!=="tauri"||a.vault_path!==`${request.original_binding.root}/assets/local-covers/native-cover-${request.operation_id}.bin`||typeof a.created_at!=="string"||!a.created_at||a.server_version_provenance!==undefined||!p||Object.keys(p).length!==8||p.format!==1||p.version_id!==request.operation_id||p.producer_operation_id!==request.operation_id||p.byte_length!==request.byte_length||p.original_filename!==request.name||p.mime_type!==request.mime_type||p.uploader!=="local-user"||typeof p.binary_sha256!=="string"||!/^[a-f0-9]{64}$/.test(p.binary_sha256)||typeof r.receipt_json!=="string"||new TextEncoder().encode(r.receipt_json).length>65536||!r.canonical_snapshot.assets.some(asset=>equal(asset,a)))throw new Error("Native original cover acknowledgement differs");
 const receipt=JSON.parse(r.receipt_json) as Record<string,unknown>;
 if(receipt.operation_id!==request.operation_id||!equal(receipt.request,request)||receipt.canonical_committed!==true||!equal(receipt.asset,a))throw new Error("Native original cover receipt differs");
 return{asset:clone(a),snapshot:r.canonical_snapshot};
}
/** One deliberate selection makes one original operation. Failure retains it;
 * this function never silently substitutes a new operation or a legacy data URL.
 * Caller admits only this genuine IPC snapshot, then uses normal draft save. */
export async function createOriginalNativeLocalCover(file:File,selected:{space_id:string;document_id:string;draft_id:string},verifyCurrent:()=>void,admit:(snapshot:LocalVaultSnapshot,verify:()=>void)=>Promise<void>){
 if(!(file instanceof Blob)||!file.name.trim()||file.name.length>255||file.size<1||file.size>LIMIT||!["image/png","image/jpeg","image/gif","image/webp"].includes(file.type)||Object.values(selected).some(v=>!v||v.length>256||/[\\/\0]/.test(v)))throw new Error("Select a supported local cover no larger than30MiB");
 const session=captureNativeArchiveLocalSession();const verify=()=>{session.verify();verifyCurrent();};verify();const binding=await session.describe();verify();
 const request:OriginalRequest={operation_id:crypto.randomUUID(),name:file.name,mime_type:file.type,byte_length:file.size,...clone(selected),original_binding:clone(binding)};
 if(!UUID.test(request.operation_id))throw new Error("Original cover operation unavailable");
 const retained:Retained={format:1,operationId:request.operation_id,request,state:"prepared",received:0,blob:file,acknowledgement:null};
 await store(retained,true);verify();retained.state="transfer_unknown";await store(retained);verify();
 let response=await invoke("prepare_original_native_local_cover",{provider:session.context.provider,context:session.context,current:session.context,request});verify();
 let token="";
 const ack=(value:unknown,index?:number,received?:number)=>{
  const r=value as {format?:unknown;operation_id?:unknown;producer_token?:unknown;index?:unknown;received?:unknown};
  if(r?.format!==1||r.operation_id!==request.operation_id||typeof r.producer_token!=="string"||!/^[a-f0-9]{64}$/.test(r.producer_token)||(index!==undefined&&(r.index!==index||r.received!==received)))throw new Error("Original native cover chunk acknowledgement differs");token=r.producer_token;
 };
 ack(response);
 for(let index=0,offset=0;offset<file.size;index++){
  verify();const bytes=new Uint8Array(await file.slice(offset,Math.min(offset+CHUNK,file.size)).arrayBuffer());verify();
  response=await invoke("append_original_native_local_cover",{producerToken:token,current:session.context,index,bytes:Array.from(bytes)});verify();offset+=bytes.length;ack(response,index,offset);retained.received=offset;await store(retained);verify();
 }
 const result=await invoke("finish_original_native_local_cover",{producerToken:token,current:session.context});verify();
 const checked=checkedNativeLocalCoverAcknowledgement(result,request);retained.state="canonical_committed";retained.acknowledgement=result;retained.blob=null;await store(retained);verify();
 await admit(checked.snapshot,verify);verify();return{operationId:request.operation_id,asset:checked.asset};
}

async function retainedOriginal(operationId:string):Promise<Retained>{
 if(!UUID.test(operationId))throw new Error("Exact original local cover operation required");
 const db=await database();try{return await new Promise<Retained>((resolve,reject)=>{
  const tx=db.transaction(STORES,"readonly"),request=tx.objectStore("originals").get(operationId);let row:Retained;let failure:Error|null=null;
  request.onsuccess=()=>{const base=request.result as Retained|undefined;if(base)journalRow(tx,base,value=>{row=value;},error=>{failure=error;tx.abort();});};
  tx.oncomplete=()=>{if(!row||row.format!==1||row.operationId!==operationId||row.request.operation_id!==operationId||!Number.isSafeInteger(row.request.byte_length)||row.request.byte_length<1||row.request.byte_length>LIMIT||!["prepared","transfer_unknown","canonical_committed","adopted","discard_unknown","discarded"].includes(row.state)||!Number.isSafeInteger(row.received)||row.received<0||row.received>row.request.byte_length){reject(new Error("Original local cover journal invalid or unavailable"));return;}resolve(row);};
  tx.onabort=tx.onerror=()=>reject(failure??new Error("Original local cover journal unavailable"));
 });}finally{db.close();}
}
/** Known original winner only: no file upload, new UUID or receipt-only grant. */
export async function reconcileOriginalNativeLocalCover(operationId:string,selected:{space_id:string;document_id:string;draft_id:string},verifyCurrent:()=>void,admit:(snapshot:LocalVaultSnapshot,verify:()=>void)=>Promise<void>){
 const session=captureNativeArchiveLocalSession(),verify=()=>{session.verify();verifyCurrent();};verify();const row=await retainedOriginal(operationId);verify();
 if(!equal(row.request.original_binding.context,session.context)||!equal({space_id:row.request.space_id,document_id:row.request.document_id,draft_id:row.request.draft_id},selected))throw new Error("Original local cover page/draft/session differs");
 const binding=await session.describe();verify();if(binding.root!==row.request.original_binding.root)throw new Error("Original local cover belongs to another vault");
 const result=await invoke("inspect_original_native_local_cover",{provider:session.context.provider,context:session.context,current:session.context,originalBinding:binding,operationId});verify();
 const checked=checkedNativeLocalCoverAcknowledgement(result,row.request);row.state="canonical_committed";row.acknowledgement=result;row.blob=null;await store(row);verify();await admit(checked.snapshot,verify);verify();return{operationId,asset:checked.asset};
}
export async function markOriginalNativeLocalCoverAdopted(operationId:string,pin:string,verifyCurrent:()=>void){
 const session=captureNativeArchiveLocalSession(),verify=()=>{session.verify();verifyCurrent();};verify();const row=await retainedOriginal(operationId);verify();
 if(!equal(row.request.original_binding.context,session.context)||pin!==`starlane-local-attachment://${operationId}`||row.state!=="canonical_committed")throw new Error("Original acknowledged local cover differs");
 checkedNativeLocalCoverAcknowledgement(row.acknowledgement,row.request);row.state="adopted";await store(row);verify();
}
export async function previewOriginalNativeLocalCover(pin:string,spaceId:string,documentId:string){
 const id=pin.slice("starlane-local-attachment://".length);if(pin!==`starlane-local-attachment://${id}`||!UUID.test(id))throw new Error("Exact local cover version required");
 const session=captureNativeArchiveLocalSession();session.verify();const binding=await session.describe();session.verify();
 const response=await invoke("open_original_native_local_cover_image",{provider:session.context.provider,context:session.context,current:session.context,originalBinding:binding,operationId:id,spaceId,documentId});session.verify();
 const r=response as {format?:unknown;url?:unknown;complete_image?:unknown};if(r.format!==1||r.complete_image!==true||typeof r.url!=="string"||!/^starlane-local-cover-preview:\/\/image\/[a-f0-9]{64}$/.test(r.url))throw new Error("Original local cover image acknowledgement differs");return r.url;
}

/** Read-only retained UI status. It conveys no native creator or file authority. */
export async function inspectRetainedOriginalNativeLocalCover(operationId:string,selected:{space_id:string;document_id:string;draft_id:string}){
 const session=captureNativeArchiveLocalSession();session.verify();const row=await retainedOriginal(operationId);session.verify();
 if(!equal(row.request.original_binding.context,session.context)||!equal({space_id:row.request.space_id,document_id:row.request.document_id,draft_id:row.request.draft_id},selected))throw new Error("Original local cover page/draft/session differs");
 return{operationId,state:row.state,received:row.received,byteLength:row.request.byte_length,originalFileRetained:row.blob instanceof Blob};
}

export interface RetainedNativeLocalCoverReview {
 operationId:string;name:string;state:Retained["state"];received:number;byteLength:number;originalFileRetained:boolean;discardedBytes?:number;
}
type SelectedCoverDraft={space_id:string;document_id:string;draft_id:string};
/** Bounded original-session discovery. These labels grant no native custody. */
export async function retainedOriginalNativeLocalCovers(selected:SelectedCoverDraft,currentPin:string|null|undefined,verifyCurrent:()=>void):Promise<RetainedNativeLocalCoverReview[]>{
 const session=captureNativeArchiveLocalSession(),verify=()=>{session.verify();verifyCurrent();};verify();
 const binding=await session.describe();verify();const db=await database();
 try{return await new Promise<RetainedNativeLocalCoverReview[]>((resolve,reject)=>{
  const tx=db.transaction(STORES,"readonly"),request=tx.objectStore("originals").openCursor();let scanned=0;const rows:RetainedNativeLocalCoverReview[]=[];let failure:Error|null=null;
  const refuse=(message:string)=>{failure=new Error(message);tx.abort();};
  request.onsuccess=()=>{
   try{verify();const cursor=request.result;if(!cursor)return;
    if(++scanned>1024){refuse("Original cover journal exceeds review bound; retain the original vault");return;}
    const base=cursor.value as Retained;
    if(!base||base.format!==1||typeof base.operationId!=="string"||!UUID.test(base.operationId)||!base.request||base.request.operation_id!==base.operationId){refuse("Original cover journal contains unavailable evidence");return;}
    journalRow(tx,base,row=>{try{
    if(!row||row.format!==1||typeof row.operationId!=="string"||!UUID.test(row.operationId)||!row.request||row.request.operation_id!==row.operationId){refuse("Original cover journal contains unavailable evidence");return;}
    if(equal(row.request.original_binding?.context,session.context)&&row.request.original_binding.root===binding.root&&equal({space_id:row.request.space_id,document_id:row.request.document_id,draft_id:row.request.draft_id},selected)){
     if(!["prepared","transfer_unknown","canonical_committed","adopted","discard_unknown","discarded"].includes(row.state)||typeof row.request.name!=="string"||!row.request.name.trim()||row.request.name.length>255||!Number.isSafeInteger(row.request.byte_length)||row.request.byte_length<1||row.request.byte_length>LIMIT||!Number.isSafeInteger(row.received)||row.received<0||row.received>row.request.byte_length||new TextEncoder().encode(JSON.stringify(row.request)).length>65536){refuse("Original selected cover evidence is malformed");return;}
     if(row.state==="canonical_committed"||row.state==="adopted")checkedNativeLocalCoverAcknowledgement(row.acknowledgement,row.request);
     // Adoption alone does not prove that a later draft save retained the pin.
     const discarded=row.state==="discarded"?discardStructure(row.acknowledgement,row.request):null;
     if(!(row.state==="adopted"&&currentPin===`starlane-local-attachment://${row.operationId}`)){
      if(rows.length>=64){refuse("More than64 original cover operations require review; retain the journal");return;}
      rows.push({operationId:row.operationId,name:row.request.name,state:row.state,received:row.received,byteLength:row.request.byte_length,originalFileRetained:row.blob instanceof Blob,...(discarded?{discardedBytes:discarded.discardedBytes}:{})});
     }
    }
    cursor.continue();
    }catch(error){failure=error instanceof Error?error:new Error("Original cover review unavailable");tx.abort();}},error=>{failure=error;tx.abort();});
   }catch(error){failure=error instanceof Error?error:new Error("Original cover review unavailable");tx.abort();}
  };
  tx.oncomplete=()=>{try{verify();resolve(rows);}catch(error){reject(error);}};
  tx.onabort=tx.onerror=()=>reject(failure??new Error("Original cover review unavailable; retain the journal"));
 });}finally{db.close();}
}
/** Deliberate continuation of the retained original File and operation only.
 * Native SQL/chunk/custody proof supplies progress. Unknown/corrupt disagreement
 * refuses without truncating files, replacing the original, or choosing latest. */
export async function resumeOriginalNativeLocalCover(operationId:string,selected:SelectedCoverDraft,verifyCurrent:()=>void,admit:(snapshot:LocalVaultSnapshot,verify:()=>void)=>Promise<void>){
 const session=captureNativeArchiveLocalSession(),verify=()=>{session.verify();verifyCurrent();};verify();const row=await retainedOriginal(operationId);verify();
 if(!equal(row.request.original_binding.context,session.context)||!equal({space_id:row.request.space_id,document_id:row.request.document_id,draft_id:row.request.draft_id},selected)||!["prepared","transfer_unknown"].includes(row.state)||!(row.blob instanceof Blob)||row.blob.size!==row.request.byte_length||row.blob.type!==row.request.mime_type)throw new Error("Retained original cover File/session is unavailable; check the original winner");
 const binding=await session.describe();verify();if(!equal(binding,row.request.original_binding))throw new Error("Original local cover source changed; retain it for review");
 row.state="transfer_unknown";await store(row);verify();
 let response=await invoke("resume_original_native_local_cover",{provider:session.context.provider,context:session.context,current:session.context,request:row.request});verify();
 const progress=checkedNativeLocalCoverResumeProgress(response,operationId,row.request.byte_length,row.received);
 let offset=progress.offset,index=progress.index,token=progress.token;
 row.received=offset;await store(row);verify();
 while(offset<row.request.byte_length){
  const bytes=new Uint8Array(await row.blob.slice(offset,Math.min(offset+CHUNK,row.request.byte_length)).arrayBuffer());verify();
  response=await invoke("append_original_native_local_cover",{producerToken:token,current:session.context,index,bytes:Array.from(bytes)});verify();const ack=response as {format?:unknown;operation_id?:unknown;producer_token?:unknown;received?:unknown;index?:unknown};
  if(ack?.format!==1||ack.operation_id!==operationId||typeof ack.producer_token!=="string"||!/^[a-f0-9]{64}$/.test(ack.producer_token)||ack.index!==index||ack.received!==offset+bytes.length)throw new Error("Native original cover continuation acknowledgement differs");
  token=ack.producer_token;offset+=bytes.length;index++;row.received=offset;await store(row);verify();
 }
 const result=await invoke("finish_original_native_local_cover",{producerToken:token,current:session.context});verify();const checked=checkedNativeLocalCoverAcknowledgement(result,row.request);
 row.state="canonical_committed";row.acknowledgement=result;row.blob=null;await store(row);verify();await admit(checked.snapshot,verify);verify();return{operationId,asset:checked.asset};
}

/** Actual destination creator and exact selected published/owned-draft cover. */
export async function previewImportedNativeLocalCover(selected:{space_id:string;document_id:string;asset_id:string;draft_id:string|null;cover_reference:string}){
 const session=captureNativeArchiveLocalSession();session.verify();const binding=await session.describe();session.verify();
 const response=await invoke("open_current_imported_native_cover_image",{provider:session.context.provider,context:session.context,current:session.context,selection:selected,originalBinding:binding});session.verify();
 const r=response as {format?:unknown;url?:unknown;complete_image?:unknown};if(r.format!==1||r.complete_image!==true||typeof r.url!=="string"||!/^starlane-imported-cover-preview:\/\/image\/[a-f0-9]{64}$/.test(r.url))throw new Error("Exact imported cover image acknowledgement differs");return r.url;
}

/** Transport progress only; actual native custody still owns all file reads/writes. */
export function checkedNativeLocalCoverResumeProgress(value:unknown,operationId:string,byteLength:number,previouslyAcknowledged:number){
 const r=value as {format?:unknown;operation_id?:unknown;producer_token?:unknown;received?:unknown;next_index?:unknown};
 if(!r||Object.keys(r).length!==5||r.format!==1||r.operation_id!==operationId||typeof r.producer_token!=="string"||!/^[a-f0-9]{64}$/.test(r.producer_token)||!Number.isSafeInteger(r.received)||!Number.isSafeInteger(r.next_index)||!Number.isSafeInteger(byteLength)||byteLength<1||byteLength>LIMIT||!Number.isSafeInteger(previouslyAcknowledged)||previouslyAcknowledged<0||previouslyAcknowledged>byteLength)throw new Error("Native original cover progress differs");
 const offset=r.received as number,index=r.next_index as number;
 if(offset<previouslyAcknowledged||offset<0||offset>byteLength||index<0||index>512||index!==Math.ceil(offset/CHUNK))throw new Error("Native original cover progress disagrees; retain evidence");
 return{offset,index,token:r.producer_token};
}

function discardStructure(value:unknown,request:OriginalRequest):{requestJson:string;requestSha:string;discardedBytes:number}{
 if(!value||typeof value!=="object"||new TextEncoder().encode(JSON.stringify(value)).length>128*1024)throw new Error("Original discard acknowledgement exceeds bound");
 const r=value as Record<string,unknown>;if(Object.keys(r).length!==6||r.format!==1||r.state!=="discarded_original_payload"||r.operation_id!==request.operation_id||typeof r.request_sha256!=="string"||!/^[a-f0-9]{64}$/.test(r.request_sha256)||typeof r.request_json!=="string"||typeof r.receipt_json!=="string"||new TextEncoder().encode(r.request_json).length>65536||new TextEncoder().encode(r.receipt_json).length>65536||!equal(JSON.parse(r.request_json),request))throw new Error("Exact original discard acknowledgement differs");
 const receipt=JSON.parse(r.receipt_json) as Record<string,unknown>;if(!receipt||Object.keys(receipt).length!==5||receipt.format!==1||receipt.state!==r.state||receipt.operation_id!==request.operation_id||typeof receipt.native_completion_json!=="string")throw new Error("Original native discard receipt differs");
 const raw=receipt.native_completion_json,body=JSON.parse(raw) as Record<string,unknown>,after=body?.payload_after as Record<string,unknown>,stamp=receipt.native_completion_stamp as Record<string,unknown>;
 const keys=["device","inode","uid","mode","links","length","ctime","ctime_nsec","mtime","mtime_nsec"];
 const numeric=(v:Record<string,unknown>)=>v&&Object.keys(v).length===10&&keys.every(key=>typeof v[key]==="number"&&Number.isFinite(v[key])&&Number.isInteger(v[key])&&Number(v[key])>=0);
 if(!body||Object.keys(body).length!==7||body.format!==1||body.state!==r.state||body.operation_id!==request.operation_id||body.request_sha256!==r.request_sha256||typeof body.plan_sha256!=="string"||!/^[a-f0-9]{64}$/.test(body.plan_sha256)||!equal(body.binding,request.original_binding)||!numeric(after)||after.links!==0||!Number.isSafeInteger(after.length)||Number(after.length)>request.byte_length||!numeric(stamp)||stamp.links!==1||stamp.length!==new TextEncoder().encode(raw).length)throw new Error("Original native discard completion differs");
 // Transport stamps are historical observations, never renderer custody grants.
 return{requestJson:r.request_json,requestSha:r.request_sha256,discardedBytes:Number(after.length)};
}
export async function checkedNativeLocalCoverDiscardAcknowledgement(value:unknown,request:OriginalRequest):Promise<{discardedBytes:number}>{
 const checked=discardStructure(value,request);const bytes=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(checked.requestJson));const actual=Array.from(new Uint8Array(bytes),byte=>byte.toString(16).padStart(2,"0")).join("");if(actual!==checked.requestSha)throw new Error("Original discard request bytes/hash differ");return{discardedBytes:checked.discardedBytes};
}
/** Explicit same-original discard or result inspection; no File upload/new UUID.
 * Unknown outcomes retain the original File and operation for deliberate review. */
export async function discardOriginalNativeLocalCover(operationId:string,selected:{space_id:string;document_id:string;draft_id:string},verifySelected:()=>void,inspectOnly=false):Promise<{discardedBytes:number}>{
 const session=captureNativeArchiveLocalSession(),verify=()=>{session.verify();verifySelected();};verify();const row=await retainedOriginal(operationId);verify();
 if(!equal(row.request.original_binding.context,session.context)||!equal({space_id:row.request.space_id,document_id:row.request.document_id,draft_id:row.request.draft_id},selected)||!["prepared","transfer_unknown","discard_unknown","discarded"].includes(row.state))throw new Error("Original partial operation/draft/session is unavailable; check its winner");
 const binding=await session.describe();verify();if(!equal(binding,row.request.original_binding))throw new Error("Original discard source changed; retain its evidence for review");
 if(!inspectOnly){row.state="discard_unknown";await store(row);verify();}
 const result=await invoke(inspectOnly?"inspect_original_native_local_cover_discard":"discard_original_native_local_cover",{provider:session.context.provider,context:session.context,current:session.context,request:row.request});verify();const checked=await checkedNativeLocalCoverDiscardAcknowledgement(result,row.request);verify();
 row.state="discarded";row.acknowledgement=clone(result);row.blob=null;await store(row);verify();return checked;
}
