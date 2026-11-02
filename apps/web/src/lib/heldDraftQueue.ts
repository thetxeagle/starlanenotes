import { api, captureOfflineServerContext, getApiBase } from './api';
import { listOfflineServerMutations, listOfflineServerAttachments, updateOfflineServerMutation, acknowledgeOfflineServerMutation, retainUnknownOfflineServerMutation, type QueuedServerMutation, type QueuedServerAttachment } from './offlineServerCache';
import { sameOfflineReplayOrigin, verifyOfflineReplayContext, type OfflineReplayContext } from './offlineReplayOrigin';
export interface HeldDraftQueueItem extends Omit<QueuedServerMutation,'replayState'> { replayState?: 'held'|'unknown'; hold?: {schemaVersion:1; reason:string; status?:number; message:string} }
export interface QueuedDraftSource {schema_version:1;space_id:string;document_id:string;draft_id:string;draft_source_digest:string;draft:{title:string;content:string;icon:string|null;cover_image:string|null};locked:boolean;locked_by:string|null;page_edit_allowed:true;page_updated_at:string;page_published:boolean;selected_draft:Record<string,unknown>}
export interface HeldDraftQueueReview {schemaVersion:1;original:HeldDraftQueueItem;source:QueuedDraftSource;origin:OfflineReplayContext['origin'];inventory:{mutations:HeldDraftQueueItem[];attachments:QueuedServerAttachment[]}}
export interface HeldDraftQueueTransport {
 capture():OfflineReplayContext; verify(context:OfflineReplayContext):void; online():boolean;
 list(base:string):HeldDraftQueueItem[];attachments(base:string):QueuedServerAttachment[];
 update(next:HeldDraftQueueItem,expected:HeldDraftQueueItem):void;ack(id:string,expected:HeldDraftQueueItem):void;retain(original:HeldDraftQueueItem,message:string):void;
 fetch:typeof fetch;runExclusive<T>(name:string,run:()=>Promise<T>):Promise<T>;
}
const clone=<T>(v:T):T=>JSON.parse(JSON.stringify(v));
const equal=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
const digest=(v:unknown):v is string=>typeof v==='string'&&/^[a-f0-9]{64}$/.test(v);
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
export function queuedDraftPath(item:HeldDraftQueueItem):[string,string,string]|null {
 if(item.method!=='PUT'||!object(item.body)||!Object.prototype.hasOwnProperty.call(item.body,'source_digest'))return null;
 const m=/^\/spaces\/([^/?#]+)\/documents\/([^/?#]+)\/drafts\/([^/?#]+)$/.exec(item.path);if(!m)return null;
 return [m[1],m[2],m[3]];
}
function source(value:unknown,path:[string,string,string],actor:string):QueuedDraftSource {
 if(!object(value)||value.schema_version!==1||value.space_id!==path[0]||value.document_id!==path[1]||value.draft_id!==path[2]||!digest(value.draft_source_digest)||!object(value.draft))throw Error('Current draft write source is unproven; keep the original edit.');
 const d=value.draft;if(typeof d.title!=='string'||typeof d.content!=='string'||![d.icon,d.cover_image].every(v=>v===null||typeof v==='string'))throw Error('Current draft source fields are unproven.');
 const selected=value.selected_draft;
 if(typeof value.locked!=='boolean'||!(value.locked_by===null||typeof value.locked_by==='string')||value.page_edit_allowed!==true||typeof value.page_published!=='boolean'||typeof value.page_updated_at!=='string'||!value.page_updated_at||value.locked&&value.locked_by!==actor||!object(selected))throw Error('Current page Edit and lock authority are unproven.');
 if(selected.id!==path[2]||selected.space_id!==path[0]||selected.document_id!==path[1]||selected.source_digest!==value.draft_source_digest||selected.document_title!==d.title||!['owner','editor'].includes(String(selected.access_role))||typeof selected.owner_id!=='string'||!selected.owner_id||typeof selected.owner_name!=='string'||typeof selected.can_publish!=='boolean'||typeof selected.created_at!=='string'||!selected.created_at||typeof selected.updated_at!=='string'||!selected.updated_at||typeof selected.base_updated_at!=='string'||!selected.base_updated_at||selected.access_role==='owner'&&selected.owner_id!==actor||selected.can_publish&&selected.access_role!=='owner')throw Error('Current selected draft Edit authority is unproven.');
 for(const field of ['title','content','icon','cover_image'])if((field==='icon'||field==='cover_image'?selected[field]??null:selected[field])!==d[field])throw Error('Current selected draft source fields disagree.');
 return clone(value) as unknown as QueuedDraftSource;
}
function confirms(item:HeldDraftQueueItem,value:unknown,reviewed:QueuedDraftSource,actor:string):boolean {
 const p=queuedDraftPath(item),b=item.body;if(!p||!object(b)||!object(value)||value.space_id!==p[0]||value.document_id!==p[1]||value.id!==p[2]||!digest(value.source_digest)||!['owner','editor'].includes(String(value.access_role)))return false;
 if(typeof value.owner_id!=='string'||value.owner_id!==reviewed.selected_draft.owner_id||typeof value.owner_name!=='string'||typeof value.can_publish!=='boolean'||value.can_publish&&value.access_role!=='owner'||value.access_role==='owner'&&value.owner_id!==actor||!['created_at','updated_at','base_updated_at'].every(k=>typeof value[k]==='string'&&!!value[k])||typeof value.title!=='string'||typeof value.content!=='string')return false;
 for(const k of ['title','content','icon','cover_image'] as const){const patch=b[k];if(patch!==undefined&&patch!==null&&typeof patch!=='string')return false;const raw=patch===undefined||patch===null?reviewed.draft[k]:patch;const expected=k==='title'&&typeof raw==='string'?raw.trim():(k==='icon'||k==='cover_image')&&typeof raw==='string'?!raw.trim()?null:raw:raw;if((value[k]??null)!==expected)return false;}return true;
}
export function createHeldDraftQueueService(t:HeldDraftQueueTransport){
 const key=(base:string)=>`starlane:offline-replay:${base}`;
 const inventory=(base:string)=>clone({mutations:t.list(base),attachments:t.attachments(base)});
 const check=(original:HeldDraftQueueItem,context:OfflineReplayContext)=>{
  t.verify(context);if(!t.online())throw Error('Reconnect to review the original queued edit.');
  if(context.origin.storageMode!=='server'||!object(original.body)||!digest(original.body.source_digest)||!queuedDraftPath(original)||!sameOfflineReplayOrigin(original.origin,context.origin)||original.serverBase!==context.origin.serverBase||!['held','unknown'].includes(original.replayState??''))throw Error('Original queued draft identity or held state is unavailable.');
  const inv=inventory(original.serverBase),current=inv.mutations.find(x=>x.id===original.id);if(!equal(current,original))throw Error('Original queue item changed; review it again.');
  if(inv.mutations[0]?.id!==original.id)throw Error('Review the earlier queued operation first; this save cannot leapfrog it.');
  if(inv.mutations.some(x=>x.id!==original.id&&x.replayState!==undefined&&x.replayState!=='held')||inv.attachments.some(x=>x.replayState!==undefined))throw Error('Another uncertain operation must be reviewed first; later held items remain retained.');
  if([...inv.mutations,...inv.attachments].some(x=>!sameOfflineReplayOrigin(x.origin,context.origin)))throw Error('Foreign or legacy queue ownership requires review.');
  const p=queuedDraftPath(original)!;
  if(inv.attachments.some(x=>x.spaceId===p[0]&&x.documentId===p[1]||object(original.body)&&Object.values(original.body).some(value=>typeof value==='string'&&value.includes(x.dataUrl))))throw Error('Review the original attachment dependency before sending this save.');
  return inv;
 };
 const load=async(item:HeldDraftQueueItem,context:OfflineReplayContext)=>{
  const p=queuedDraftPath(item)!;const response=await t.fetch(context.origin.serverBase+item.path+'/save-source',{cache:'no-store',headers:{Authorization:`Bearer ${context.bearer}`}});t.verify(context);
  if(response.status!==200)throw Error('Current page and draft Edit access could not be verified.');const value=await response.json();t.verify(context);return source(value,p,context.origin.accountId);
 };
 return {
  inspect(space:string,page:string){const c=t.capture();t.verify(c);return clone(t.list(c.origin.serverBase).filter(x=>{const p=queuedDraftPath(x);return p?.[0]===space&&p[1]===page&&sameOfflineReplayOrigin(x.origin,c.origin)&&(x.replayState!==undefined||x.attempts>0||!!x.lastError);}));},
  async prepare(id:string){const c=t.capture();t.verify(c);return t.runExclusive(key(c.origin.serverBase),async()=>{const selected=t.list(c.origin.serverBase).find(x=>x.id===id);if(!selected)throw Error('Original queued save is unavailable.');const original=clone(selected);const inv=check(original,c);const s=await load(original,c);check(original,c);if(!equal(inventory(original.serverBase),inv))throw Error('Queue dependencies changed during review.');return {schemaVersion:1,original,source:s,origin:clone(c.origin),inventory:inv} as HeldDraftQueueReview;});},
  async resume(input:HeldDraftQueueReview){const review=clone(input),c=t.capture();t.verify(c);if(review.schemaVersion!==1||!sameOfflineReplayOrigin(review.origin,c.origin))throw Error('This review belongs to its original account and server.');return t.runExclusive(key(c.origin.serverBase),async()=>{
   const inv=check(review.original,c);if(!equal(inv,review.inventory))throw Error('Queue changed after review.');const s=await load(review.original,c);check(review.original,c);if(!equal(inventory(c.origin.serverBase),inv)||!equal(s,review.source))throw Error('Current source or queue changed after review.');
   if(!object(review.original.body)||s.draft_source_digest!==review.original.body.source_digest)throw Error('Current draft differs from the original source. Keep the original request and use a separately reviewed recovery copy.');
   const dispatched:HeldDraftQueueItem={...review.original,replayState:'unknown',hold:{schemaVersion:1,reason:'outcome_unproven',message:'Original save dispatched; confirmation is pending.'}};t.update(dispatched,review.original);let response:Response|undefined;
   try {
    t.verify(c);response=await t.fetch(c.origin.serverBase+dispatched.path,{method:'PUT',headers:{Authorization:`Bearer ${c.bearer}`,'Content-Type':'application/json'},body:JSON.stringify(dispatched.body)});
    t.verify(c);if(response.status!==200)throw Error(`Server returned ${response.status}; keep the original request for review.`);
    const result:unknown=await response.json();t.verify(c);if(!confirms(dispatched,result,s,c.origin.accountId))throw Error('Original save acknowledgement is unproven.');
    if(!equal(t.list(c.origin.serverBase).find(x=>x.id===dispatched.id),dispatched))throw Error('Queue changed during original dispatch.');t.ack(dispatched.id,dispatched);return clone(result);
   }catch(error){const message=error instanceof Error?error.message:'Original outcome is unknown.';const knownStatus=response?.status&&response.status!==200?response.status:undefined;const held:HeldDraftQueueItem={...dispatched,replayState:knownStatus?'held':'unknown',attempts:dispatched.attempts+1,lastError:message,hold:{schemaVersion:1,reason:'outcome_unproven',...(knownStatus?{status:knownStatus}:{}),message}};
    try{t.update(held,dispatched);}catch{t.retain(dispatched,'Original dispatched bytes are uncertain; concurrent queued work is distinct.');}throw Error(message);
   }
  });}
 };
}
export function createLiveHeldDraftQueueService(){return createHeldDraftQueueService({capture:captureOfflineServerContext,verify:c=>verifyOfflineReplayContext(c,getApiBase(),api.getToken()),online:()=>typeof navigator==='undefined'||navigator.onLine!==false,list:listOfflineServerMutations,attachments:listOfflineServerAttachments,update:(n,e)=>updateOfflineServerMutation(n as QueuedServerMutation,e as QueuedServerMutation),ack:(id,e)=>acknowledgeOfflineServerMutation(id,e as QueuedServerMutation),retain:(o,m)=>retainUnknownOfflineServerMutation(o as QueuedServerMutation,m),fetch:(...args)=>fetch(...args),runExclusive:async(name,run)=>{if(typeof navigator==='undefined'||!navigator.locks)throw Error('Deliberate queue resume requires Web Locks; keep the original edit.');return navigator.locks.request(name,run);}});}

/** Shared stateless facade; durable ownership remains in the source-bound queue journal. */
export const heldDraftQueueService = createLiveHeldDraftQueueService();
