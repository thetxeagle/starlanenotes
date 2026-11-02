import type { DraftRecoveryIntent } from '@/lib/documentDraftRecoveryActions';
import type { DraftSaveSelection,DraftRecord } from '@/lib/dataClient';
import type { HeldDraftQueueReview,HeldDraftQueueItem } from '@/lib/heldDraftQueue';
import type { StorageLike } from '@/lib/documentDraftRecovery';
import { sameOfflineReplayOrigin,isOfflineReplayOrigin } from '@/lib/offlineReplayOrigin';
const PREFIX='starlane_queued_draft_copy_review_v1:';
const immutable=(o:HeldDraftQueueItem)=>({id:o.id,serverBase:o.serverBase,path:o.path,method:o.method,body:o.body,queuedAt:o.queuedAt,queueOrder:o.queueOrder,origin:o.origin});
const clone=<T>(v:T):T=>JSON.parse(JSON.stringify(v));
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
export interface CopyFields {title:string;content:string;icon:string|null;cover_image:string|null}
export interface QueuedCopyComposition {fields:CopyFields;provenance:Record<keyof CopyFields,'original-request'|'current-reviewed-draft'>}
export interface QueuedCopyReviewRecord {schemaVersion:1;original:HeldDraftQueueItem;composition:QueuedCopyComposition;intent:DraftRecoveryIntent}
export function composeQueuedDraftCopy(original:unknown,current:CopyFields):QueuedCopyComposition {
 if(!object(original))throw Error('Original queued changes are unavailable.');
 const fields:CopyFields={title:current.title,content:current.content,icon:current.icon,cover_image:current.cover_image};const provenance:QueuedCopyComposition['provenance']={title:'current-reviewed-draft',content:'current-reviewed-draft',icon:'current-reviewed-draft',cover_image:'current-reviewed-draft'};
 for(const k of ['title','content','icon','cover_image'] as const){const v=original[k];if(v===null||v===undefined)continue;if(typeof v!=='string')throw Error(`Unsupported original ${k}; download original.`);provenance[k]='original-request';if(k==='title')fields.title=v.trim();else if(k==='content')fields.content=v;else fields[k]=v.trim()?v:null;}
 if(!fields.title.trim()||new TextEncoder().encode(fields.title).length>500||new TextEncoder().encode(fields.content).length>5_242_880)throw Error('New copy exceeds existing draft title/content bounds.');
 return {fields,provenance};
}
function key(original:HeldDraftQueueItem,selection:DraftSaveSelection):string {
 if(selection.mode!=='server'||!selection.origin||!sameOfflineReplayOrigin(original.origin,selection.origin)||selection.accountId!==original.origin?.accountId||selection.serverBase!==original.serverBase)throw Error('Original copy review account/session/provider is unavailable.');
 return PREFIX+[original.serverBase,JSON.stringify(original.origin),original.id].map(encodeURIComponent).join(':');
}
function validate(record:unknown,selection:DraftSaveSelection):asserts record is QueuedCopyReviewRecord {
 if(!object(record)||record.schemaVersion!==1||!object(record.original)||!object(record.intent)||!object(record.composition))throw Error('Retained copy review is corrupt; preserve original evidence.');
 const r=record as unknown as QueuedCopyReviewRecord;key(r.original,selection);
 const {intent:i}=r;const o=r.original,p=/^\/spaces\/([^/]+)\/documents\/([^/]+)\/drafts\/([^/]+)$/.exec(o.path);
 if(!o.id||o.method!=='PUT'||!p||!isOfflineReplayOrigin(o.origin)||!object(o.body)||typeof o.body.source_digest!=='string'||!/^[a-f0-9]{64}$/.test(o.body.source_digest)||!['held','unknown'].includes(o.replayState??'')||!object(i.scope)||!object(i.checkpoint)||!object(i.reviewedDraft)||!object(i.selection)||!object(i.patch)||typeof i.published!=='boolean')throw Error('Retained copy identity is unproven.');
 if(i.scope.storageMode!=='server'||i.scope.authIdentity!==selection.accountId||i.scope.serverTarget!==selection.serverBase||i.scope.spaceId!==p[1]||i.scope.documentId!==p[2]||i.scope.draftId!==p[3]||i.checkpoint.snapshotId!==`queued-copy:${o.id}`||i.checkpoint.capturedAt!==o.queuedAt||i.selection.mode!=='server'||i.selection.accountId!==selection.accountId||i.selection.serverBase!==selection.serverBase||i.selection.storageSelection!==selection.storageSelection||!sameOfflineReplayOrigin(i.selection.origin,selection.origin)||i.reviewedDraft.id!==p[3]||i.reviewedDraft.space_id!==p[1]||i.reviewedDraft.document_id!==p[2]||!['owner','editor'].includes(i.reviewedDraft.access_role)||typeof i.reviewedDraft.source_digest!=='string'||!/^[a-f0-9]{64}$/.test(i.reviewedDraft.source_digest))throw Error('Retained copy belongs to a different selection.');
 const d=i.reviewedDraft;const c=composeQueuedDraftCopy(o.body,{title:d.title,content:d.content,icon:d.icon??null,cover_image:d.cover_image??null});
 const composition=r.composition;const fields=['title','content','icon','cover_image'] as const;
 const sameComposition=object(composition.fields)&&object(composition.provenance)&&Object.keys(composition).length===2&&Object.keys(composition.fields).length===4&&Object.keys(composition.provenance).length===4&&fields.every(k=>composition.fields[k]===c.fields[k]&&composition.provenance[k]===c.provenance[k]);
 if(!sameComposition||i.patch.title!==c.fields.title||i.patch.content!==c.fields.content||i.patch.icon!==(c.fields.icon??'')||i.patch.cover_image!==(c.fields.cover_image??'')||i.checkpoint.title!==c.fields.title||i.checkpoint.content!==c.fields.content||i.checkpoint.icon!==c.fields.icon||i.checkpoint.coverImage!==c.fields.cover_image||i.checkpoint.synopsis!==''||i.checkpoint.customCss!==''||i.checkpoint.pageWidth!=='standard'||i.checkpoint.baseDraftUpdatedAt!==d.updated_at)throw Error('Retained raw composition disagrees; no new request is allowed.');
}
/** Pure new-copy proposal. Call only after fresh scoped Edit-source review. */
export function prepareQueuedDraftCopy(review:HeldDraftQueueReview,selection:DraftSaveSelection):QueuedCopyReviewRecord {
 const original=clone(review.original);key(original,selection);
 if(!sameOfflineReplayOrigin(review.origin,selection.origin))throw Error('Review origin changed.');
 const draft=clone(review.source.selected_draft) as unknown as DraftRecord;
 const composition=composeQueuedDraftCopy(original.body,review.source.draft),f=composition.fields;
 const intent:DraftRecoveryIntent={scope:{storageMode:'server',authIdentity:selection.accountId!,serverTarget:selection.serverBase,spaceId:review.source.space_id,documentId:review.source.document_id,draftId:review.source.draft_id},selection:clone(selection),reviewedDraft:draft,published:review.source.page_published,holdReason:'outcome_unknown',checkpoint:{snapshotId:`queued-copy:${original.id}`,capturedAt:original.queuedAt,baseDraftUpdatedAt:draft.updated_at,title:f.title,content:f.content,synopsis:'',pageWidth:'standard',customCss:'',icon:f.icon,coverImage:f.cover_image},patch:{title:f.title,content:f.content,icon:f.icon??'',cover_image:f.cover_image??''}};
 const record:QueuedCopyReviewRecord={schemaVersion:1,original,composition,intent};validate(record,selection);return record;
}
/** Read original exact proposal across reload; never recompose a held operation. */
export function readQueuedDraftCopyReview(storage:StorageLike,original:HeldDraftQueueItem,selection:DraftSaveSelection):QueuedCopyReviewRecord|null {
 const raw=storage.getItem(key(original,selection));if(raw===null)return null;let r:unknown;try{r=JSON.parse(raw);}catch{throw Error('Retained copy review is corrupt; do not replace it.');}validate(r,selection);
 if(JSON.stringify(immutable(r.original))!==JSON.stringify(immutable(original)))throw Error('Original queued request changed; retained copy evidence is distinct.');return clone(r);
}
/** Durable review only. No birth/save/checkpoint/queue mutation or acknowledgement. */
function persistLocked(storage:StorageLike,record:QueuedCopyReviewRecord,selection:DraftSaveSelection):QueuedCopyReviewRecord {
 validate(record,selection);const existing=readQueuedDraftCopyReview(storage,record.original,selection);if(existing)return existing;
 const k=key(record.original,selection),raw=JSON.stringify(record);storage.setItem(k,raw);if(storage.getItem(k)!==raw)throw Error('Copy review was not durably retained; no request may be sent.');return clone(record);
}

export async function persistQueuedDraftCopyReview(storage:StorageLike,record:QueuedCopyReviewRecord,selection:DraftSaveSelection,runExclusive?:<T>(name:string,run:()=>Promise<T>)=>Promise<T>):Promise<QueuedCopyReviewRecord> {
 const name=`starlane:queued-copy-review:${key(record.original,selection)}`;
 if(runExclusive)return runExclusive(name,async()=>persistLocked(storage,record,selection));
 if(typeof navigator==='undefined'||!navigator.locks)throw Error('Retaining copy review requires Web Locks; download original.');
 return navigator.locks.request(name,async()=>persistLocked(storage,record,selection));
}
/** Retained proposals remain discoverable even if their queue request was ACKed. */
export function listQueuedDraftCopyReviews(storage:StorageLike & Pick<Storage,'key'|'length'>,selection:DraftSaveSelection,space:string,document:string):QueuedCopyReviewRecord[] {
 if(selection.mode!=='server'||!selection.origin||!isOfflineReplayOrigin(selection.origin))throw Error('Current original copy scope is unavailable.');
 const prefix=PREFIX+[selection.serverBase,JSON.stringify(selection.origin)].map(encodeURIComponent).join(':')+':';
 const keys:string[]=[];for(let n=0;n<storage.length;n++){const k=storage.key(n);if(k?.startsWith(prefix))keys.push(k);}
 const out:QueuedCopyReviewRecord[]=[];
 for(const k of keys){const raw=storage.getItem(k);if(raw===null)continue;let value:unknown;try{value=JSON.parse(raw);}catch{throw Error('Retained copy inventory is corrupt; preserve evidence.');}validate(value,selection);if(k!==key(value.original,selection))throw Error('Retained copy inventory identity disagrees.');if(value.intent.scope.spaceId===space&&value.intent.scope.documentId===document)out.push(clone(value));}
 return out;
}
