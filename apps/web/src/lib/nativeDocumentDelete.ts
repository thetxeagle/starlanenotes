import type { DocumentRecord } from "./dataClient";
import { captureNativeArchiveLocalSession } from "./nativeArchiveCurrentContext";
import { canonicalDeleteOperation, requirePermanentDeleteTarget } from "./documentDeleteIntent";
import { assertLocalVaultSnapshot, type LocalVaultSnapshot } from "./localVault";
const PREFIX = "starlane_native_document_delete_v1:";
const MAX_RECORDS = 64, MAX_RECORD_BYTES = 65536, MAX_TOTAL_BYTES = 1048576;
const bytes = (v: string) => new TextEncoder().encode(v).length;
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
const same = (a: unknown, b: unknown): boolean => JSON.stringify(a, (_k,v: unknown) => v && typeof v === "object" && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).sort(([x],[y])=>x.localeCompare(y))) : v) === JSON.stringify(b, (_k,v: unknown) => v && typeof v === "object" && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).sort(([x],[y])=>x.localeCompare(y))) : v);
type Session = ReturnType<typeof captureNativeArchiveLocalSession>;
type PhysicalIdentity={device:number;inode:number};
export type Binding = Awaited<ReturnType<Session["describe"]>> & {custody:{root:PhysicalIdentity;database_directory:PhysicalIdentity;database:PhysicalIdentity}};
function checkedCustody(value:unknown):Binding["custody"] {const c=value as Binding["custody"];if(!c||Object.keys(c).length!==3||![c.root,c.database_directory,c.database].every(v=>v&&Object.keys(v).length===2&&Number.isSafeInteger(v.device)&&v.device>=0&&Number.isSafeInteger(v.inode)&&v.inode>0))throw new Error("Original physical deletion selection unavailable; retain evidence");return clone(c);}
async function physicalBinding(session:Session,space:string):Promise<Binding>{session.verify();const invoke=(window as unknown as {__TAURI_INTERNALS__?:{invoke(c:string,a:Record<string,unknown>):Promise<unknown>}}).__TAURI_INTERNALS__?.invoke;if(!invoke)throw new Error("Original native Trash physical source unavailable");const result=await invoke("list_original_native_trash_documents",{provider:session.context.provider,context:clone(session.context),current:clone(session.context),spaceId:space}) as {format?:unknown;space_id?:unknown;binding?:Binding};session.verify();if(bytes(JSON.stringify(result))>16777216||result?.format!==1||result.space_id!==space||!result.binding||Object.keys(result.binding).length!==4||!same(result.binding.context,session.context))throw new Error("Original native Trash physical source differs");checkedCustody(result.binding.custody);return clone(result.binding);}
export interface NativeDeleteAttempt { format: 1; request: { operation_id: string; queued_at: string; space_id: string; document_id: string; original_target: {created_at: string; deleted_at: string}; original_binding: Binding } }
function checked(raw: string): NativeDeleteAttempt {
 if (bytes(raw)>MAX_RECORD_BYTES) throw new Error("Native deletion evidence exceeds bound; retain it for review");
 const row=JSON.parse(raw) as NativeDeleteAttempt, r=row?.request;
 if (!row || Object.keys(row).length!==2 || row.format!==1 || !r || Object.keys(r).length!==6 || !canonicalDeleteOperation(r.operation_id) || ![r.queued_at,r.space_id,r.document_id].every(v=>typeof v==="string" && v.trim().length>0 && bytes(v)<=4096) || /[/?#]/.test(r.space_id+r.document_id) || bytes(JSON.stringify(r))>32768) throw new Error("Native deletion evidence malformed; retain it for review");
 requirePermanentDeleteTarget(r.original_target);
 const b=r.original_binding;
 if(!b || Object.keys(b).length!==4 || typeof b.root!=="string" || !b.root.startsWith("/") || typeof b.revision!=="string" || !b.revision || b.revision.length>256 || !b.context || Object.keys(b.context).length!==7)throw new Error("Native original deletion binding malformed; retain evidence");
 checkedCustody(b.custody);
 return clone(row);
}
export function listPendingNativeDocumentDeletes(): NativeDeleteAttempt[] {
 if(localStorage.length>10000)throw new Error("Native deletion evidence inventory exceeds bound");
 const rows:NativeDeleteAttempt[]=[];let size=0;
 for(let i=0;i<localStorage.length;i++){
  const key=localStorage.key(i);if(!key?.startsWith(PREFIX))continue;
  const raw=localStorage.getItem(key);if(raw===null)throw new Error("Native deletion inventory changed; retain evidence");
  const row=checked(raw);if(key!==PREFIX+row.request.operation_id && !(key.startsWith(PREFIX+row.request.operation_id+":evidence:") && canonicalDeleteOperation(key.slice((PREFIX+row.request.operation_id+":evidence:").length))))throw new Error("Native deletion journal identity differs");
  size+=bytes(raw);rows.push(row);if(rows.length>MAX_RECORDS || size>MAX_TOTAL_BYTES)throw new Error("Native deletion evidence inventory exceeds bound");
 }
 return rows;
}
async function dispatch(session: Session, original: NativeDeleteAttempt, raw: string, admit: (snapshot: LocalVaultSnapshot, verify: ()=>void)=>Promise<void>) {
 session.verify();const r=original.request,key=PREFIX+r.operation_id;
 if(!same(r.original_binding.context,session.context) || localStorage.getItem(key)!==raw)throw new Error("Original native deletion session or journal differs");
 const invoke=(window as unknown as {__TAURI_INTERNALS__?:{invoke(c:string,a:Record<string,unknown>):Promise<unknown>}}).__TAURI_INTERNALS__?.invoke;
 if(!invoke)throw new Error("Bound native deletion command unavailable; retain evidence");
 try {
 const before=await physicalBinding(session,r.space_id);if(before.root!==r.original_binding.root||!same(before.custody,r.original_binding.custody))throw new Error("Original displayed physical Trash source changed; retain exact attempt");
 const response=await invoke("delete_original_native_trash_document",{provider:session.context.provider,context:clone(session.context),current:clone(session.context),request:clone(r)});session.verify();
 if(bytes(JSON.stringify(response))>16777216+MAX_RECORD_BYTES)throw new Error("Native deletion acknowledgment exceeds bound");
 const result=response as {format?:unknown;receipt?:unknown;canonical_committed?:unknown;operation_id?:unknown;snapshot?:unknown;reconciled?:unknown};
 if(result?.format!==1 || result.canonical_committed!==true || result.operation_id!==r.operation_id || typeof result.receipt!=="string" || bytes(result.receipt)>MAX_RECORD_BYTES || typeof result.reconciled!=="boolean")throw new Error("Native deletion acknowledgment differs; retain original attempt");
 const receipt=JSON.parse(result.receipt) as {format?:unknown;request?:unknown;creator?:unknown;canonical_committed?:unknown;operation_id?:unknown;canonical_revision?:unknown};
 if(Object.keys(receipt).length!==6 || receipt.format!==1 || receipt.canonical_committed!==true || receipt.operation_id!==r.operation_id || !same(receipt.request,r) || !same(receipt.creator,r.original_binding.custody) || typeof receipt.canonical_revision!=="string" || !receipt.canonical_revision)throw new Error("Original native deletion raw receipt differs");
 assertLocalVaultSnapshot(result.snapshot);const snapshot=result.snapshot;
 if(!snapshot.canonical_revision || snapshot.documents.some(p=>p.id===r.document_id))throw new Error("Original deleted page is present or canonical revision missing");
 const current=await physicalBinding(session,r.space_id);session.verify();
 if(!same(current.custody,r.original_binding.custody)||current.root!==r.original_binding.root || current.revision!==snapshot.canonical_revision)throw new Error("Native original source changed before cache admission; retain exact attempt");
 const verify=()=>{session.verify();if(localStorage.getItem(key)!==raw)throw new Error("Original deletion journal changed; retain evidence");};
 verify();await admit(clone(snapshot),verify);verify();localStorage.removeItem(key);
 if(localStorage.getItem(key)!==null)throw new Error("Original deletion acknowledged but journal remains; retain it for review");
 } catch(error) {
  if(localStorage.getItem(key)!==raw) {
   const evidenceKey=key+":evidence:"+crypto.randomUUID();
   // A conflicting writer cannot replace the exact dispatched request with its bytes.
   // Preserve the original under a separate evidence key; duplicates refuse replay.
   localStorage.setItem(evidenceKey,raw);
   if(localStorage.getItem(evidenceKey)!==raw)throw new Error("Native deletion outcome unknown and original evidence retention failed");
  }
  throw error;
 }
}
async function locked<T>(work:()=>Promise<T>):Promise<T>{
 if(!navigator.locks?.request)throw new Error("Native deletion needs cross-window serialization; retain original page and evidence");
 return navigator.locks.request("starlane-native-original-document-delete-v1",work);
}
export async function deleteOriginalNativeDocument(space:string,document:string,target:{created_at?:string;deleted_at?:string|null},admit:(snapshot:LocalVaultSnapshot,verify:()=>void)=>Promise<void>, selectedBinding?:Binding){
 const witness=requirePermanentDeleteTarget(target),session=captureNativeArchiveLocalSession();
 return locked(async()=>{
  session.verify();let rows=listPendingNativeDocumentDeletes();
  if(rows.length>1)throw new Error("Conflicting native deletion attempts require original named review");
  let row=rows[0];
  if(row && (row.request.space_id!==space || row.request.document_id!==document || !same(row.request.original_target,witness) || !same(row.request.original_binding.context,session.context)))throw new Error("Prior native deletion belongs to another original page/session; reconcile that attempt first");
  if(!row){if(!selectedBinding)throw new Error("Original displayed canonical Trash selection unavailable; reload Trash");const binding=clone(selectedBinding);const current=await physicalBinding(session,space);session.verify();if(!same(binding,current))throw new Error("Original displayed canonical Trash source changed; reload and review");rows=listPendingNativeDocumentDeletes();if(rows.length)throw new Error("Prior native deletion appeared; reconcile it first");row={format:1,request:{operation_id:crypto.randomUUID(),queued_at:new Date().toISOString(),space_id:space,document_id:document,original_target:witness as {created_at:string;deleted_at:string},original_binding:clone(binding)}};const raw=JSON.stringify(row);checked(raw);const key=PREFIX+row.request.operation_id;if(localStorage.getItem(key)!==null)throw new Error("Native deletion operation collision");localStorage.setItem(key,raw);if(localStorage.getItem(key)!==raw)throw new Error("Original deletion evidence could not be retained");}
  const raw=localStorage.getItem(PREFIX+row.request.operation_id);if(raw===null)throw new Error("Original native deletion evidence absent");await dispatch(session,clone(row),raw,admit);
 });
}
/** Exact retained request only: never current-page lookup or witness rebasing. */
export async function reconcileOriginalNativeDocumentDelete(operation:string,admit:(snapshot:LocalVaultSnapshot,verify:()=>void)=>Promise<void>){
 const session=captureNativeArchiveLocalSession();return locked(async()=>{session.verify();const rows=listPendingNativeDocumentDeletes();if(rows.length!==1 || rows[0].request.operation_id!==operation)throw new Error("Original named native deletion evidence conflicts or is absent");const raw=localStorage.getItem(PREFIX+operation)!;await dispatch(session,rows[0],raw,admit);});
}

const trashBindings=new WeakMap<object,{binding:Binding;space:string;document:string;target:unknown}>();
export function originalNativeTrashBinding(row:object,space?:string,document?:string,target?:unknown):Binding|undefined {const original=trashBindings.get(row);if(!original)return undefined;if(space!==undefined && (original.space!==space || original.document!==document || !same(original.target,target)))throw new Error("Original canonical Trash selection differs");return clone(original.binding);}
/** Authoritative held-descriptor producer before the confirmation can open. */
export async function listOriginalNativeTrashDocuments(space:string):Promise<DocumentRecord[]> {
 const session=captureNativeArchiveLocalSession();session.verify();
 const native=(window as unknown as {__TAURI_INTERNALS__?:{invoke(c:string,a:Record<string,unknown>):Promise<unknown>}}).__TAURI_INTERNALS__;
 if(!native)throw new Error("Original native Trash source unavailable");
 const response=await native.invoke("list_original_native_trash_documents",{provider:session.context.provider,context:clone(session.context),current:clone(session.context),spaceId:space});session.verify();
 if(bytes(JSON.stringify(response))>16777216)throw new Error("Original native Trash source exceeds bound");
 const r=response as {format?:unknown;binding?:Binding;space_id?:unknown;documents?:DocumentRecord[]};
 if(r.format!==1 || r.space_id!==space || !r.binding || Object.keys(r.binding).length!==4 || !same(r.binding.context,session.context) || typeof r.binding.root!=="string" || !r.binding.root.startsWith("/") || typeof r.binding.revision!=="string" || !r.binding.revision || r.binding.revision.length>256 || !Array.isArray(r.documents))throw new Error("Original canonical Trash selection malformed");
 checkedCustody(r.binding.custody);
 const rows=clone(r.documents);const ids=new Set<string>();for(const row of rows){if(typeof row.id!=="string" || !row.id || ids.has(row.id) || row.space_id!==space)throw new Error("Original Trash rows differ");requirePermanentDeleteTarget({created_at:row.created_at,deleted_at:row.deleted_at});ids.add(row.id);trashBindings.set(row,{binding:clone(r.binding),space:row.space_id,document:row.id,target:{created_at:row.created_at,deleted_at:row.deleted_at}});}session.verify();return rows;
}

export async function describeCurrentNativeDeletionSelection(space:string):Promise<Binding>{const session=captureNativeArchiveLocalSession();return physicalBinding(session,space);}
