import {canonicalDeleteOperation, originalDeleteTarget} from "./documentDeleteIntent";
import {isOfflineReplayOrigin, type OfflineReplayOrigin} from "./offlineReplayOrigin";
import type {QueuedServerMutation} from "./offlineServerCache";

export type CleanupState = "pending" | "retained_review" | "physically_completed";
export interface CleanupResult { operation_id:string; metadata_deleted:true; outcome:CleanupState; reason:string|null; assets:{asset_id:string;outcome:CleanupState;reason:string|null}[] }
export interface CleanupAttempt { version:1; operation:string; space:string; document:string; original_target:{created_at:string;deleted_at:string}; origin:OfflineReplayOrigin }
const key="starlane_permanent_cleanup_v1";
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==="object"&&!Array.isArray(value);
const state=(value:unknown):value is CleanupState=>value==="pending"||value==="retained_review"||value==="physically_completed";
const reason=(value:unknown):value is string|null=>value===null||typeof value==="string"&&value.length>0&&value.length<=256;
/** A metadata acknowledgment cannot stand in for physical completion. */
export function parseCleanupResult(value:unknown,operation:string):CleanupResult {
 if(!object(value)||Object.keys(value).length!==5||value.operation_id!==operation||!canonicalDeleteOperation(operation)||value.metadata_deleted!==true||!state(value.outcome)||!reason(value.reason)||!Array.isArray(value.assets)||value.assets.length>32)throw new Error("Original cleanup outcome is unproven; retain the original receipt");
 const ids=new Set<string>();
 for(const asset of value.assets){if(!object(asset)||Object.keys(asset).length!==3||typeof asset.asset_id!=="string"||!asset.asset_id.trim()||asset.asset_id.length>4096||ids.has(asset.asset_id)||!state(asset.outcome)||!reason(asset.reason)||asset.outcome==="physically_completed"&&asset.reason!==null||asset.outcome!=="physically_completed"&&asset.reason===null)throw new Error("Original cleanup asset evidence is malformed; retain the original receipt");ids.add(asset.asset_id);}
 const expected=value.assets.length===0||value.assets.some(a=>a.outcome==="retained_review")?"retained_review":value.assets.some(a=>a.outcome==="pending")?"pending":"physically_completed";
 if(value.outcome!==expected||value.assets.length===0&&value.reason!=="no_supported_owned_original_assets"||value.assets.length>0&&value.reason!==null)throw new Error("Original cleanup completion is inconsistent; retain the original receipt");
 return structuredClone(value) as unknown as CleanupResult;
}
function validAttempt(value:unknown):value is CleanupAttempt {
 if(!object(value)||Object.keys(value).length!==6||value.version!==1||!canonicalDeleteOperation(value.operation)||typeof value.space!=="string"||typeof value.document!=="string"||!value.space||!value.document||value.space.length>4096||value.document.length>4096||/[/?#]/.test(value.space+value.document)||!isOfflineReplayOrigin(value.origin))return false;
 const target=originalDeleteTarget({original_target:value.original_target});return !!target?.deleted_at;
}
export function cleanupAttemptFromAcknowledgedMutation(row:QueuedServerMutation):CleanupAttempt {
 const match=/^\/spaces\/([^/?#]+)\/documents\/([^/?#]+)\/trash$/.exec(row.path),target=originalDeleteTarget(row.body);
 const result={version:1,operation:row.deletionOperationId,space:match?.[1],document:match?.[2],original_target:target,origin:row.origin};
 if(row.method!=="DELETE"||!validAttempt(result))throw new Error("Original acknowledged deletion evidence unavailable; no new cleanup identity may be created");
 return structuredClone(result);
}
export function readCleanupAttempts():CleanupAttempt[] {
 const raw=localStorage.getItem(key);if(raw===null)return[];
 if(raw.length>1048576)throw new Error("Cleanup review exceeds bound; retain every original receipt");
 let rows:unknown;try{rows=JSON.parse(raw);}catch{throw new Error("Cleanup journal is unreadable; retain it for review");}
 if(!Array.isArray(rows)||rows.length>64||rows.some(row=>!validAttempt(row))||new Set(rows.map(row=>row.operation)).size!==rows.length)throw new Error("Cleanup journal conflicts; retain every original receipt");
 return structuredClone(rows);
}
/** Shared journal lock is nested inside the existing original-page operation lock. */
export async function retainAcknowledgedCleanup(attempt:CleanupAttempt,verify:()=>void):Promise<void> {
 if(!validAttempt(attempt)||!navigator.locks?.request)throw new Error("Original cleanup requires serialized, complete receipt evidence");
 const original=structuredClone(attempt);
 await navigator.locks.request("starlane-permanent-cleanup-journal",()=>{verify();const rows=readCleanupAttempts();const prior=rows.find(row=>row.operation===original.operation);if(prior&&JSON.stringify(prior)!==JSON.stringify(original))throw new Error("Original cleanup identity changed; retain conflicting evidence");if(!prior)rows.push(original);if(rows.length>64)throw new Error("Cleanup journal is full; retain the original deletion queue");const raw=JSON.stringify(rows);if(raw.length>1048576)throw new Error("Cleanup journal exceeds bound; retain the original deletion queue");verify();localStorage.setItem(key,raw);if(localStorage.getItem(key)!==raw)throw new Error("Cleanup journal changed; retain original deletion evidence");verify();});
}

export function cleanupAttemptRequest(attempt:CleanupAttempt):{path:string;body:{original_target:CleanupAttempt["original_target"]}} {
 if(!validAttempt(attempt))throw new Error("Original cleanup receipt is incomplete; retain it for review");
 return {path:`/spaces/${attempt.space}/documents/${attempt.document}/permanent-deletions/${attempt.operation}/cleanup`,body:{original_target:structuredClone(attempt.original_target)}};
}

/** Remove only the selected durable receipt after exact physical completion. */
export async function acknowledgeCompletedCleanup(attempt:CleanupAttempt,result:CleanupResult,verify:()=>void):Promise<void> {
 const original=structuredClone(attempt);if(!validAttempt(original)||parseCleanupResult(result,original.operation).outcome!=="physically_completed"||!navigator.locks?.request)throw new Error("Physical completion is unproven; retain the original cleanup receipt");
 await navigator.locks.request("starlane-permanent-cleanup-journal",()=>{
  verify();const rows=readCleanupAttempts();const selected=rows.find(row=>row.operation===original.operation);
  if(JSON.stringify(selected)!==JSON.stringify(original))throw new Error("Original cleanup receipt changed; retain current evidence");
  const raw=JSON.stringify(rows.filter(row=>row.operation!==original.operation));verify();localStorage.setItem(key,raw);if(localStorage.getItem(key)!==raw)throw new Error("Cleanup journal changed during completion; preserve current evidence");verify();
 });
}
