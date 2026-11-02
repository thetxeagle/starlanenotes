import { api, ApiError, captureOfflineServerContext, getApiBase } from "./api";
import { isOfflineReplayOrigin, sameOfflineReplayOrigin, verifyOfflineReplayContext, type OfflineReplayContext, type OfflineReplayOrigin } from "./offlineReplayOrigin";
import type { DraftRecord } from "./dataClient";
import type { PublicationSnapshotDetail, PublicationSnapshotExtraction } from "./documentPublishedHistory";

export type SnapshotRecoveryReason = "raw_only" | "title_bounds" | "content_bounds";
export interface SnapshotRecoveryFields { title: string; content: string; icon: string | null; cover_image: string | null }
export interface SnapshotRecoverySource {
  schema_version: 1; space_id: string; document_id: string; snapshot_id: string;
  snapshot_digest: string; snapshot_source_digest: string; page_source_digest: string;
  page_published: boolean; page_updated_at: string; recoverable: boolean;
  reason: SnapshotRecoveryReason | null; extraction: PublicationSnapshotExtraction;
  draft: SnapshotRecoveryFields | null;
}
export interface SnapshotRecoveryRequest { schema_version: 1; operation_id: string; snapshot_source_digest: string; page_source_digest: string }
export interface SnapshotRecoveryIntent {
  schema_version: 1; operation_id: string; space_id: string; document_id: string; snapshot_id: string;
  origin: OfflineReplayOrigin; request: SnapshotRecoveryRequest; source: SnapshotRecoverySource;
}
export interface SnapshotRecoveryReceipt extends SnapshotRecoveryRequest {
  space_id: string; document_id: string; snapshot_id: string; draft: DraftRecord; disposition: "applied" | "historical";
}
export interface SnapshotRecoveryRecord {
  schema_version: 1; intent: SnapshotRecoveryIntent; state: "prepared" | "dispatched" | "unknown" | "confirmed" | "cancelled" | "rejected";
  receipt?: SnapshotRecoveryReceipt; error?: string;
}
export class SnapshotRecoveryHeldError extends Error {
  constructor(public operationId: string, message: string) { super(message); this.name = "SnapshotRecoveryHeldError"; }
}
export class SnapshotRecoveryUnavailableError extends Error {
  constructor(public reason: SnapshotRecoveryReason) { super(reason === "raw_only" ? "This legacy source cannot be safely extracted into a draft. Keep the preserved Markdown download." : reason === "title_bounds" ? "The captured title is outside supported draft bounds. Keep the preserved source." : "The captured body exceeds supported draft bounds. Keep the preserved source."); this.name = "SnapshotRecoveryUnavailableError"; }
}
interface StorageLike { getItem(key: string): string | null; setItem(key: string, value: string): void; length: number; key(index: number): string | null }
export interface SnapshotRecoveryTransport {
  storage: StorageLike; capture(): OfflineReplayContext; verify(context: OfflineReplayContext): void;
  fetch: typeof fetch; runExclusive<T>(name: string, run: () => Promise<T>): Promise<T>;
  online(): boolean; operationId(): string;
}
const PREFIX = "starlane_snapshot_recovery_v1:";
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const canonical = (value: unknown): unknown => Array.isArray(value) ? value.map(canonical) : value && typeof value === "object" ? Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical((value as Record<string, unknown>)[key])])) : value;
const equal = (a: unknown, b: unknown) => JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));
const object = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const text = (v: unknown): v is string => typeof v === "string" && !!v.trim();
const digest = (v: unknown): v is string => typeof v === "string" && /^[a-f0-9]{64}$/.test(v);
const uuid = (v: unknown): v is string => typeof v === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-[1-5][a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/.test(v);
const nullable = (v: unknown): v is string | null => v === null || typeof v === "string";
const fields = (v: unknown): v is SnapshotRecoveryFields => object(v) && typeof v.title === "string" && typeof v.content === "string" && nullable(v.icon) && nullable(v.cover_image);
const supported = (v: SnapshotRecoveryFields) => !!v.title.trim() && new TextEncoder().encode(v.title).length <= 500 && new TextEncoder().encode(v.content).length <= 5_242_880;
const route = (space: string, page: string, snapshot: string) => `/spaces/${encodeURIComponent(space)}/documents/${encodeURIComponent(page)}/publication-snapshots/${encodeURIComponent(snapshot)}`;
const lock = (origin: OfflineReplayOrigin, space: string, page: string) => `starlane:snapshot-recovery:${JSON.stringify([origin.serverBase, origin.accountId, space, page])}`;
const unproven = () => new Error("Snapshot recovery evidence is unproven; retain the original request and source for review.");
function sourceFor(value: unknown, space: string, page: string, snapshot: string): SnapshotRecoverySource {
  if (!object(value) || value.schema_version !== 1 || value.space_id !== space || value.document_id !== page || value.snapshot_id !== snapshot || !digest(value.snapshot_digest) || !digest(value.snapshot_source_digest) || !digest(value.page_source_digest) || typeof value.page_published !== "boolean" || !text(value.page_updated_at) || typeof value.recoverable !== "boolean" || !["exact_published_draft", "proven_generated_header", "raw_only"].includes(String(value.extraction))) throw unproven();
  if (value.recoverable ? value.reason !== null || !fields(value.draft) || !supported(value.draft) || value.extraction === "raw_only" : value.draft !== null || !["raw_only", "title_bounds", "content_bounds"].includes(String(value.reason)) || (value.extraction === "raw_only") !== (value.reason === "raw_only")) throw unproven();
  return {schema_version:1,space_id:space,document_id:page,snapshot_id:snapshot,snapshot_digest:value.snapshot_digest,snapshot_source_digest:value.snapshot_source_digest,page_source_digest:value.page_source_digest,page_published:value.page_published,page_updated_at:value.page_updated_at,recoverable:value.recoverable,reason:value.reason as SnapshotRecoveryReason|null,extraction:value.extraction as PublicationSnapshotExtraction,draft:value.draft === null ? null : {title:(value.draft as SnapshotRecoveryFields).title,content:(value.draft as SnapshotRecoveryFields).content,icon:(value.draft as SnapshotRecoveryFields).icon,cover_image:(value.draft as SnapshotRecoveryFields).cover_image}};
}
function validateIntent(intent: SnapshotRecoveryIntent): void {
  if (!intent || intent.schema_version !== 1 || !uuid(intent.operation_id) || !text(intent.space_id) || !text(intent.document_id) || !text(intent.snapshot_id) || !isOfflineReplayOrigin(intent.origin) || intent.origin.storageMode !== "server" || !intent.request || Object.keys(intent.request).length !== 4 || intent.request.schema_version !== 1 || intent.request.operation_id !== intent.operation_id) throw unproven();
  const source = sourceFor(intent.source,intent.space_id,intent.document_id,intent.snapshot_id);
  if (!source.recoverable || intent.request.snapshot_source_digest !== source.snapshot_source_digest || intent.request.page_source_digest !== source.page_source_digest) throw unproven();
}
function receiptFor(value: unknown, intent: SnapshotRecoveryIntent, status?: number): SnapshotRecoveryReceipt {
  if (!object(value) || value.schema_version !== 1 || value.operation_id !== intent.operation_id || value.space_id !== intent.space_id || value.document_id !== intent.document_id || value.snapshot_id !== intent.snapshot_id || value.snapshot_source_digest !== intent.request.snapshot_source_digest || value.page_source_digest !== intent.request.page_source_digest || !["applied", "historical"].includes(String(value.disposition)) || status !== undefined && (status === 201 ? value.disposition !== "applied" : status === 200 ? value.disposition !== "historical" : true) || !object(value.draft)) throw unproven();
  const d = value.draft, expected = intent.source.draft!;
  if (!uuid(d.id) || d.document_id !== intent.document_id || d.space_id !== intent.space_id || d.owner_id !== intent.origin.accountId || d.access_role !== "owner" || d.can_publish !== true || typeof d.owner_name !== "string" || typeof d.document_title !== "string" || !text(d.created_at) || !text(d.updated_at) || !digest(d.source_digest) || d.base_updated_at !== intent.source.page_updated_at || !equal({title:d.title,content:d.content,icon:d.icon??null,cover_image:d.cover_image??null},expected)) throw unproven();
  const draft:DraftRecord = {id:d.id,document_id:intent.document_id,space_id:intent.space_id,owner_id:intent.origin.accountId,owner_name:d.owner_name,document_title:d.document_title,title:expected.title,content:expected.content,access_role:"owner",can_publish:true,created_at:d.created_at,updated_at:d.updated_at,base_updated_at:intent.source.page_updated_at,source_digest:d.source_digest,...(expected.icon === null ? {} : {icon:expected.icon}),...(expected.cover_image === null ? {} : {cover_image:expected.cover_image}),...(typeof d.birth_operation_id === "string" ? {birth_operation_id:d.birth_operation_id} : {})};
  return {...intent.request,space_id:intent.space_id,document_id:intent.document_id,snapshot_id:intent.snapshot_id,draft,disposition:value.disposition as SnapshotRecoveryReceipt["disposition"]};
}

/** No save, editor selection, publication, offline queue, or automatic replacement operation. */
export function createPublicationSnapshotRecoveryService(transport: SnapshotRecoveryTransport) {
  const records = ():SnapshotRecoveryRecord[] => {
    const keys:string[]=[];for(let i=0;i<transport.storage.length;i++){const key=transport.storage.key(i);if(key?.startsWith(PREFIX))keys.push(key);}
    return keys.map(key=>{let r:SnapshotRecoveryRecord;try{r=JSON.parse(transport.storage.getItem(key)!);validateIntent(r.intent);}catch{throw new Error("Stored snapshot recovery is corrupt; retain it for review.");}
      if(r.schema_version!==1||key!==PREFIX+r.intent.operation_id||!["prepared","dispatched","unknown","confirmed","cancelled","rejected"].includes(r.state)||r.error!==undefined&&typeof r.error!=="string"||r.state==="confirmed"&&!r.receipt||r.receipt&&r.state!=="confirmed")throw unproven();if(r.receipt)receiptFor(r.receipt,r.intent);return clone(r);});
  };
  const read=(id:string)=>{const record=records().find(r=>r.intent.operation_id===id);if(!record)throw new SnapshotRecoveryHeldError(id,"Original recovery request is unavailable; keep its evidence. No new copy was dispatched.");return record;};
  const write=(next:SnapshotRecoveryRecord,previous:SnapshotRecoveryRecord|null)=>{const key=PREFIX+next.intent.operation_id;if(transport.storage.getItem(key)!==(previous?JSON.stringify(previous):null))throw new Error("Recovery journal changed; retain original evidence.");const raw=JSON.stringify(next);transport.storage.setItem(key,raw);if(transport.storage.getItem(key)!==raw)throw new Error("Recovery evidence could not be retained on this device.");return clone(next);};
  const current=(origin:OfflineReplayOrigin, reauthenticated=false)=>{
    const context=transport.capture();
    const sameActorTarget=context.origin.serverBase===origin.serverBase&&context.origin.accountId===origin.accountId&&context.origin.storageMode===origin.storageMode&&context.origin.provider===origin.provider;
    if(reauthenticated?!sameActorTarget:!sameOfflineReplayOrigin(context.origin,origin))throw new Error("Recovery belongs to its original account, server and storage provider; retain its request.");
    transport.verify(context);return context;
  };
  const request=async(context:OfflineReplayContext,path:string,body?:SnapshotRecoveryRequest)=>{
    transport.verify(context);const response=await transport.fetch(context.origin.serverBase+path,{method:body?"POST":"GET",cache:"no-store",headers:{Authorization:`Bearer ${context.bearer}`,...(body?{"Content-Type":"application/json"}:{})},...(body?{body:JSON.stringify(body)}:{})});
    // GET admission follows current context. A POST result belongs to its original request:
    // decode and durably confirm it before checking whether the visible UI has moved away.
    if(!body)transport.verify(context);const raw=await response.text();if(!body)transport.verify(context);
    if(!response.ok){let error:{message?:unknown;error?:unknown}={};try{error=JSON.parse(raw);}catch{/* HTTP failure remains held. */}throw new ApiError(typeof error.message==="string"?error.message:`Recovery returned ${response.status}`,response.status,typeof error.error==="string"?error.error:undefined);}
    if(body?![200,201].includes(response.status):response.status!==200)throw unproven();return{value:JSON.parse(raw) as unknown,status:response.status};
  };
  const dispatch=(input:SnapshotRecoveryIntent,retry:boolean)=>{const intent=clone(input);validateIntent(intent);return transport.runExclusive(lock(intent.origin,intent.space_id,intent.document_id),async()=>{
    const context=current(intent.origin,true);let record=read(intent.operation_id);if(!equal(record.intent,intent))throw unproven();
    if(record.state==="cancelled"||record.state==="rejected"||!retry&&record.state!=="prepared")throw new SnapshotRecoveryHeldError(intent.operation_id,"Only an undispatched reviewed request can create a copy. Retry pending outcomes using the exact original operation.");
    if(!transport.online())throw new SnapshotRecoveryHeldError(intent.operation_id,"Recovery was not dispatched. Reconnect and retry the original request explicitly.");
    const firstDispatch = record.state === "prepared" && !retry;
    record=write({...record,state:record.receipt?"confirmed":"dispatched"},record);
    try{const {value,status}=await request(context,route(intent.space_id,intent.document_id,intent.snapshot_id)+"/recover",intent.request);const receipt=receiptFor(value,intent,status);if(record.receipt&&!equal(record.receipt,{...receipt,disposition:record.receipt.disposition}))throw unproven();
      record=write({...record,state:"confirmed",receipt,error:undefined},record);transport.verify(context);return clone(receipt);
    }catch(error){const message=error instanceof Error?error.message:"Recovery outcome unavailable";const rejected = firstDispatch && error instanceof ApiError && error.status === 409 && message.startsWith("recovery_rejected: ");try{write({...record,state:record.receipt?"confirmed":rejected?"rejected":"unknown",error:message},record);}catch{throw new SnapshotRecoveryHeldError(intent.operation_id,"Recovery evidence could not be updated. Keep the original operation; never create a replacement copy.");}
      throw new SnapshotRecoveryHeldError(intent.operation_id,rejected?"The reviewed source changed before any copy was created. Original evidence is retained; explicitly review the current source again.":record.receipt?"The confirmed recovery target is retained. Return to the original context and review its receipt; current editing state was not changed.":`${message}; outcome unknown. Retry only the retained original recovery request; no new copy or publication is authorized.`);
    }
  });};
  return {
    async prepare(space:string,page:string,selectedInput:PublicationSnapshotDetail,originInput:OfflineReplayOrigin):Promise<SnapshotRecoveryIntent>{const selected=clone(selectedInput),origin=clone(originInput);if(!text(space)||!text(page)||selected.document_id!==page||!text(selected.id)||!digest(selected.snapshot_digest)||!selected.content||typeof selected.content.title!=="string"||!nullable(selected.content.icon)||!nullable(selected.content.cover_reference))throw unproven();
      return transport.runExclusive(lock(origin,space,page),async()=>{const context=current(origin);const pending=records().find(r=>r.intent.origin.serverBase===origin.serverBase&&r.intent.origin.accountId===origin.accountId&&r.intent.space_id===space&&r.intent.document_id===page&&["prepared","dispatched","unknown"].includes(r.state));if(pending)throw new SnapshotRecoveryHeldError(pending.intent.operation_id,"An original recovery request already exists. Reopen its review or retry that request explicitly before preparing another copy.");if(!transport.online())throw new Error("Reconnect to review current recovery authority and source.");
        const {value}=await request(context,route(space,page,selected.id)+"/recovery-source");const source=sourceFor(value,space,page,selected.id);if(source.snapshot_digest!==selected.snapshot_digest||source.extraction!==selected.content.extraction||source.recoverable&&!equal(source.draft,{title:selected.content.title,content:selected.content.portable_markdown,icon:selected.content.icon,cover_image:selected.content.cover_reference}))throw new Error("Selected snapshot changed before recovery review. No copy was prepared.");if(!source.recoverable)throw new SnapshotRecoveryUnavailableError(source.reason!);
        const operation_id=transport.operationId();const intent:SnapshotRecoveryIntent={schema_version:1,operation_id,space_id:space,document_id:page,snapshot_id:selected.id,origin,source,request:{schema_version:1,operation_id,snapshot_source_digest:source.snapshot_source_digest,page_source_digest:source.page_source_digest}};validateIntent(intent);transport.verify(context);write({schema_version:1,intent,state:"prepared"},null);return clone(intent);
      });
    },
    recover:(intent:SnapshotRecoveryIntent)=>dispatch(intent,false),
    retry:(id:string)=>dispatch(read(id).intent,true),
    inspect(space:string,page:string){const context=transport.capture();transport.verify(context);return records().filter(r=>r.intent.space_id===space&&r.intent.document_id===page&&r.intent.origin.serverBase===context.origin.serverBase&&r.intent.origin.accountId===context.origin.accountId&&r.intent.origin.provider===context.origin.provider&&r.intent.origin.storageMode===context.origin.storageMode);},
    async cancelPreparation(id:string){const original=read(id);await transport.runExclusive(lock(original.intent.origin,original.intent.space_id,original.intent.document_id),async()=>{current(original.intent.origin,true);const record=read(id);if(!equal(record.intent,original.intent)||record.state!=="prepared")throw new SnapshotRecoveryHeldError(id,"Only an undispatched prepared recovery can be cancelled. Keep unknown or confirmed outcome evidence.");write({...record,state:"cancelled"},record);});},
  };
}
function live(){return createPublicationSnapshotRecoveryService({storage:localStorage,capture:captureOfflineServerContext,verify:context=>verifyOfflineReplayContext(context,getApiBase(),api.getToken()),fetch:(...args)=>fetch(...args),online:()=>typeof navigator==="undefined"||navigator.onLine!==false,operationId:()=>crypto.randomUUID(),runExclusive:async(name,run)=>{if(typeof navigator==="undefined"||!navigator.locks)throw new Error("Snapshot recovery requires Web Locks. Keep original source and requests; use a supported HTTPS browser.");return navigator.locks.request(name,run);}});}
export const preparePublicationSnapshotRecovery=(space:string,page:string,selected:PublicationSnapshotDetail,origin:OfflineReplayOrigin)=>live().prepare(space,page,selected,origin);
export const recoverPreparedPublicationSnapshot=(intent:SnapshotRecoveryIntent)=>live().recover(intent);
export const retryPublicationSnapshotRecovery=(id:string)=>live().retry(id);
export const inspectPublicationSnapshotRecoveries=(space:string,page:string)=>live().inspect(space,page);
export const cancelPreparedPublicationSnapshotRecovery=(id:string)=>live().cancelPreparation(id);

/** Presentation-only availability; the server's current source/authority check remains mandatory. */
export function snapshotRecoveryUnavailableReason(snapshot: PublicationSnapshotDetail): SnapshotRecoveryReason | null {
  if (snapshot.content.extraction === "raw_only" || snapshot.content.portable_markdown === null) return "raw_only";
  if (!snapshot.content.title.trim() || new TextEncoder().encode(snapshot.content.title).length > 500) return "title_bounds";
  if (new TextEncoder().encode(snapshot.content.portable_markdown).length > 5_242_880) return "content_bounds";
  return null;
}
