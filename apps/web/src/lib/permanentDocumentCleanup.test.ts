import {beforeEach,describe,expect,it,vi} from "vitest";
import {cleanupAttemptFromAcknowledgedMutation,parseCleanupResult,readCleanupAttempts,retainAcknowledgedCleanup,type CleanupAttempt} from "./permanentDocumentCleanup";
import type {QueuedServerMutation} from "./offlineServerCache";
const operation="11111111-2222-3333-4444-555555555555";
const origin={version:1 as const,serverBase:"https://notes.example/api/v1",accountId:"owner",sessionId:"session",cacheScope:"session-partition",storageMode:"server" as const,provider:"none" as const};
const attempt:CleanupAttempt={version:1,operation,space:"space",document:"original",original_target:{created_at:"2026-10-11T00:00:00Z",deleted_at:"2026-10-11T00:01:00Z"},origin};
const complete=()=>({operation_id:operation,metadata_deleted:true,outcome:"physically_completed",reason:null,assets:[{asset_id:"version-one",outcome:"physically_completed",reason:null}]});
let store:Map<string,string>;let lockTail:Promise<unknown>;
beforeEach(()=>{store=new Map();lockTail=Promise.resolve();vi.stubGlobal("localStorage",{getItem:(k:string)=>store.get(k)??null,setItem:(k:string,v:string)=>store.set(k,v)});vi.stubGlobal("navigator",{locks:{request:(_name:string,fn:()=>unknown)=>{const next=lockTail.then(fn);lockTail=next.catch(()=>{});return next;}}});});
describe("physical completion evidence",()=>{
 it("accepts exact completion, bounded pending, and named retained review separately",()=>{expect(parseCleanupResult(complete(),operation).outcome).toBe("physically_completed");expect(parseCleanupResult({...complete(),outcome:"pending",assets:[{asset_id:"version-one",outcome:"pending",reason:"bounded_retry_with_original_operation"}]},operation).outcome).toBe("pending");expect(parseCleanupResult({...complete(),outcome:"retained_review",reason:"no_supported_owned_original_assets",assets:[]},operation).outcome).toBe("retained_review");});
 it.each([
  {...complete(),operation_id:"66666666-2222-3333-4444-555555555555"},
  {...complete(),metadata_deleted:false},
  {...complete(),outcome:"complete"},
  {...complete(),assets:[]},
  {...complete(),assets:[{asset_id:"version-one",outcome:"pending",reason:"bounded_retry_with_original_operation"}]},
  {...complete(),assets:[{asset_id:"version-one",outcome:"physically_completed",reason:"unknown"}]},
  {...complete(),assets:[...complete().assets,...complete().assets]},
  {...complete(),assets:Array.from({length:33},(_,i)=>({asset_id:String(i),outcome:"physically_completed",reason:null}))},
  {...complete(),credentials:"unexpected"},
 ])("refuses mismatched or contradictory acknowledgments",value=>{expect(()=>parseCleanupResult(value,operation)).toThrow("Original cleanup");});
});
describe("original acknowledged deletion journal",()=>{
 it("derives only the original UUID and raw Trash witness",()=>{const row={method:"DELETE",path:"/spaces/space/documents/original/trash",deletionOperationId:operation,body:{original_target:attempt.original_target},origin}as QueuedServerMutation;expect(cleanupAttemptFromAcknowledgedMutation(row)).toEqual(attempt);expect(()=>cleanupAttemptFromAcknowledgedMutation({...row,deletionOperationId:undefined})).toThrow();});
 it("serializes two different page receipts without dropping either",async()=>{await Promise.all([retainAcknowledgedCleanup(attempt,()=>{}),retainAcknowledgedCleanup({...attempt,operation:"66666666-2222-3333-4444-555555555555",document:"second"},()=>{})]);expect(readCleanupAttempts()).toHaveLength(2);});
 it("retains exact retry and refuses same-UUID target rebasing",async()=>{await retainAcknowledgedCleanup(attempt,()=>{});await retainAcknowledgedCleanup(attempt,()=>{});const before=new Map(store);await expect(retainAcknowledgedCleanup({...attempt,original_target:{...attempt.original_target,created_at:"different incarnation"}},()=>{})).rejects.toThrow("identity changed");expect(store).toEqual(before);});
 it("does not erase malformed or legacy evidence",async()=>{store.set("starlane_permanent_cleanup_v1","original unparseable receipt");const before=new Map(store);await expect(retainAcknowledgedCleanup(attempt,()=>{})).rejects.toThrow("unreadable");expect(store).toEqual(before);});
 it("cannot retain a receipt after its original session changes during lock wait",async()=>{let release!:()=>void;lockTail=new Promise(resolve=>{release=()=>resolve(undefined);});let current=true;const pending=retainAcknowledgedCleanup(attempt,()=>{if(!current)throw new Error("session changed");});current=false;release();await expect(pending).rejects.toThrow("session changed");expect(readCleanupAttempts()).toEqual([]);});
 it("fails closed without cross-window serialization",async()=>{vi.stubGlobal("navigator",{});await expect(retainAcknowledgedCleanup(attempt,()=>{})).rejects.toThrow("serialized");expect(readCleanupAttempts()).toEqual([]);});
});
