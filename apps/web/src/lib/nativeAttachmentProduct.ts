import {api,captureOfflineServerContext,getApiBase,isTauri} from './api';
import {sameOfflineReplayOrigin,verifyOfflineReplayContext,type OfflineReplayOrigin} from './offlineReplayOrigin';
import {prepareRetainedNativeAttachmentCaller,runNativeAttachmentCaller,type CurrentServerAttachmentPage} from './nativeAttachmentCaller';
import {exportNativeAttachment,previewNativeAttachment,imageNativeAttachment} from './nativeAttachmentDelivery';
export interface SelectedAttachmentVersion {rawVersion:string;origin:OfflineReplayOrigin;serverPage:CurrentServerAttachmentPage}
export interface AttachmentHistoryHead {version_id:string;revision:number}
export interface NativeAttachmentHistory {records:SelectedAttachmentVersion[];total:number;nextAfter:number|null;currentHead?:AttachmentHistoryHead|null}
/** v2 totals count authorized versions, never the actual family revision. */
export function attachmentHistoryHead(envelope:unknown):AttachmentHistoryHead|null {
 const p=envelope as {schema_version?:unknown;current_head?:unknown;total?:unknown;versions?:unknown[]}|null;
 if(!p||!Number.isSafeInteger(p.total)||Number(p.total)<0||!Array.isArray(p.versions))throw new Error('Attachment history head envelope invalid');
 let head:unknown;
 if(p.schema_version===undefined){const matches=p.versions.filter(v=>v&&typeof v==='object'&&(v as {version_number?:unknown}).version_number===p.total);if(matches.length>1)throw new Error('Legacy history head ambiguous');head=matches.length?{version_id:(matches[0] as {version_id:unknown}).version_id,revision:p.total}:null;}
 else if(p.schema_version===2){if(p.current_head===undefined)throw new Error('Explicit authorized history head required');head=p.current_head;}
 else throw new Error('Unsupported attachment history schema');
 if(head===null)return null;
 const h=head as Partial<AttachmentHistoryHead>;
 if(!h||typeof h.version_id!=='string'||! /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(h.version_id)||!Number.isSafeInteger(h.revision)||Number(h.revision)<1)throw new Error('Authorized attachment head invalid');
 for(const v of p.versions){if(v&&typeof v==='object'&&(v as {version_id?:unknown}).version_id===h.version_id&&(v as {version_number?:unknown}).version_number!==h.revision)throw new Error('Authorized attachment head revision differs');}
 return{version_id:h.version_id,revision:h.revision!};
}
const META_LIMIT=1024*1024,PREVIEW_LIMIT=65536;
const encoder=new TextEncoder();
function verifyContext(context:ReturnType<typeof captureOfflineServerContext>){verifyOfflineReplayContext(context,getApiBase(),api.getToken());}
/** Preserve exact record JSON from the response; never reconstruct an original envelope. */
export function historyRecordSpans(raw:string):string[]{
 const parsed=JSON.parse(raw) as {versions?:unknown[]};if(!parsed||!Array.isArray(parsed.versions)||parsed.versions.length>100)throw new Error('Bounded version history required');
 // Scan root properties, respecting quoted/escaped strings and nested values.
 let depth=0,string=false,escape=false,keyStart=-1,versionsStart=-1;
 for(let i=0;i<raw.length;i++){const c=raw[i];if(string){if(escape)escape=false;else if(c==='\\')escape=true;else if(c==='"'){string=false;if(depth===1&&keyStart>=0){const key=JSON.parse(raw.slice(keyStart,i+1));let j=i+1;while(/\s/.test(raw[j]??'' )&&j<raw.length)j++;if(raw[j]===':'&&key==='versions'){if(versionsStart>=0)throw new Error('Duplicate history versions field');j++;while(j<raw.length&&/\s/.test(raw[j]))j++;if(raw[j]!=='[')throw new Error('History versions array required');versionsStart=j;}keyStart=-1;}}continue;}if(c==='"'){string=true;if(depth===1)keyStart=i;}else if(c==='{'||c==='[')depth++;else if(c==='}'||c===']')depth--;}
 if(versionsStart<0)throw new Error('Original history records unavailable');
 const records:string[]=[];let start=-1;depth=0;string=false;escape=false;
 for(let i=versionsStart+1;i<raw.length;i++){const c=raw[i];if(string){if(escape)escape=false;else if(c==='\\')escape=true;else if(c==='"')string=false;continue;}if(c==='"')string=true;else if(c==='{'){if(depth===0)start=i;depth++;}else if(c==='}'){depth--;if(depth===0){records.push(raw.slice(start,i+1));start=-1;}}else if(c==='[')depth++;else if(c===']'){if(depth===0)break;depth--;}}
 if(records.length!==parsed.versions.length||records.some((r,i)=>JSON.stringify(JSON.parse(r))!==JSON.stringify(parsed.versions![i])))throw new Error('Original history span differs');return records;
}
export async function fetchNativeAttachmentHistory(page:CurrentServerAttachmentPage,logicalId:string,after=0):Promise<NativeAttachmentHistory>{
 const context=captureOfflineServerContext();verifyContext(context);
 if(!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(logicalId)||!Number.isSafeInteger(after)||after<0)throw new Error('Explicit attachment family/cursor required');
 const path=`/spaces/${encodeURIComponent(page.space_id)}/documents/${encodeURIComponent(page.document_id)}/attachment-families/${logicalId}/versions?after=${after}&limit=100`;
 const response=await fetch(context.origin.serverBase+path,{headers:{Authorization:`Bearer ${context.bearer}`},credentials:'omit',redirect:'error'});verifyContext(context);
 if(!response.ok||!response.body)throw new Error('Current attachment history unavailable');const reader=response.body.getReader();const decoder=new TextDecoder('utf-8',{fatal:true});let raw='',length=0;
 try{for(;;){const next=await reader.read();verifyContext(context);if(next.done)break;length+=next.value.length;if(length>META_LIMIT)throw new Error('Attachment history response exceeds bounded page');raw+=decoder.decode(next.value,{stream:true});}raw+=decoder.decode();}finally{await reader.cancel().catch(()=>undefined);reader.releaseLock();}
 verifyContext(context);const parsed=JSON.parse(raw) as {total:number;next_after:number|null};
 if(!Number.isSafeInteger(parsed.total)||parsed.total<0||(parsed.next_after!==null&&(!Number.isSafeInteger(parsed.next_after)||parsed.next_after<=after)))throw new Error('History continuation invalid');
 const records=historyRecordSpans(raw).map(rawVersion=>{const v=JSON.parse(rawVersion);if(v.logical_attachment_id!==logicalId)throw new Error('History family binding differs');return{rawVersion,origin:context.origin,serverPage:{space_id:page.space_id,document_id:page.document_id}};});verifyContext(context);return{records,total:parsed.total,nextAfter:parsed.next_after,currentHead:attachmentHistoryHead(parsed)};
}
/** Selected exact history record and its current page are mandatory, independent of filename. */
export async function ownSelectedAttachment(localPage:CurrentServerAttachmentPage,selection:SelectedAttachmentVersion):Promise<string>{
 if(!isTauri())throw new Error('Native attachment shell required');const context=captureOfflineServerContext();
 if(!sameOfflineReplayOrigin(context.origin,selection.origin))throw new Error('Original selected history context changed');verifyContext(context);
 const v=JSON.parse(selection.rawVersion) as {version_id:string;logical_attachment_id:string};
 const key='starlane_native_attachment_selection_v2:'+JSON.stringify([context.origin,localPage,selection.serverPage,v.logical_attachment_id,v.version_id]);
 const held=localStorage.getItem(key);let op:string;
 if(held){const prior=JSON.parse(held) as {operationId:string;rawVersion:string};if(prior.rawVersion!==selection.rawVersion||typeof prior.operationId!=='string')throw new Error('Retained selected version metadata changed; retain for review');op=prior.operationId;}
 else{op=await prepareRetainedNativeAttachmentCaller(localPage.space_id,localPage.document_id,selection.serverPage,selection.rawVersion,selection.origin);verifyContext(context);const raw=JSON.stringify({operationId:op,rawVersion:selection.rawVersion});localStorage.setItem(key,raw);if(localStorage.getItem(key)!==raw)throw new Error('Original selected version index could not be retained');}
 await runNativeAttachmentCaller(op);verifyContext(context);return op;
}
export async function saveSelectedAttachment(localPage:CurrentServerAttachmentPage,selection:SelectedAttachmentVersion){return exportNativeAttachment(await ownSelectedAttachment(localPage,selection));}
/** Intentional text preview, not a complete text-file download. Full Save stays available. */
export async function previewSelectedAttachmentText(localPage:CurrentServerAttachmentPage,selection:SelectedAttachmentVersion):Promise<{text:string;truncated:boolean}>{
 const op=await ownSelectedAttachment(localPage,selection);const length=JSON.parse(selection.rawVersion).byte_length as number;const bytes=await previewNativeAttachment(op,0,Math.min(length,PREVIEW_LIMIT));
 return{text:new TextDecoder().decode(bytes),truncated:length>PREVIEW_LIMIT};
}
export function selectedVersionId(selection:SelectedAttachmentVersion):string{return JSON.parse(selection.rawVersion).version_id;}
export function encodedSelectionBytes(selection:SelectedAttachmentVersion):number{return encoder.encode(selection.rawVersion).length;}

export async function imageSelectedAttachment(localPage:CurrentServerAttachmentPage,selection:SelectedAttachmentVersion):Promise<string>{return imageNativeAttachment(await ownSelectedAttachment(localPage,selection));}

/** Exact immutable-ID lookup, needed for published retired pins with no family attribute. */
export function exactObjectFieldSpan(raw:string,key:string):string {
 const parsed=JSON.parse(raw) as Record<string,unknown>;let string=false,escape=false,depth=0,start=-1,found:string|null=null;
 for(let i=0;i<raw.length;i++){const c=raw[i];if(string){if(escape)escape=false;else if(c==='\\')escape=true;else if(c==='"'){string=false;if(depth===1&&start>=0){const name=JSON.parse(raw.slice(start,i+1));let j=i+1;while(j<raw.length&&/\s/.test(raw[j]))j++;if(raw[j]===':'&&name===key){if(found!==null)throw new Error('Duplicate original version field');j++;while(j<raw.length&&/\s/.test(raw[j]))j++;if(raw[j]!=='{')throw new Error('Original version object required');let end=j+1,level=1,quoted=false,escaped=false;for(;end<raw.length;end++){const x=raw[end];if(quoted){if(escaped)escaped=false;else if(x==='\\')escaped=true;else if(x==='"')quoted=false;}else if(x==='"')quoted=true;else if(x==='{'||x==='[')level++;else if(x==='}'||x===']'){level--;if(level===0)break;}}found=raw.slice(j,end+1);}start=-1;}}continue;}if(c==='"'){string=true;if(depth===1)start=i;}else if(c==='{'||c==='[')depth++;else if(c==='}'||c===']')depth--;}
 if(found===null||JSON.stringify(JSON.parse(found))!==JSON.stringify(parsed[key]))throw new Error('Original version field unavailable');return found;
}
export async function fetchExactAttachmentVersion(page:CurrentServerAttachmentPage,versionId:string):Promise<SelectedAttachmentVersion>{
 if(!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(versionId))throw new Error('Explicit immutable attachment ID required');
 const context=captureOfflineServerContext();verifyContext(context);const response=await fetch(context.origin.serverBase+`/spaces/${encodeURIComponent(page.space_id)}/documents/${encodeURIComponent(page.document_id)}/attachment-versions/${versionId}`,{headers:{Authorization:`Bearer ${context.bearer}`},credentials:'omit',redirect:'error'});verifyContext(context);
 if(!response.ok||!response.body)throw new Error('Current original version unavailable');const reader=response.body.getReader(),decoder=new TextDecoder('utf-8',{fatal:true});let raw='',length=0;try{for(;;){const item=await reader.read();verifyContext(context);if(item.done)break;length+=item.value.length;if(length>65536)throw new Error('Original version response oversized');raw+=decoder.decode(item.value,{stream:true});}raw+=decoder.decode();}finally{await reader.cancel().catch(()=>undefined);reader.releaseLock();}
 const envelope=JSON.parse(raw);const rawVersion=exactObjectFieldSpan(raw,'version'),version=JSON.parse(rawVersion);
 if(envelope.current_space_id!==page.space_id||envelope.current_document_id!==page.document_id||version.version_id!==versionId||version.logical_attachment_id!==envelope.logical_attachment_id)throw new Error('Selected current version association differs');verifyContext(context);return{rawVersion,origin:context.origin,serverPage:{space_id:page.space_id,document_id:page.document_id}};
}
