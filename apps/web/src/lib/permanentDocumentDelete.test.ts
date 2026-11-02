import { readCleanupAttempts } from "./permanentDocumentCleanup";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { api, captureOfflineServerContext, replayOfflineServerMutations } from "./api";
import { permanentlyDeleteOriginalServerDocument } from "./permanentDocumentDelete";
import { listOfflineServerMutations, queueOfflineServerMutation } from "./offlineServerCache";
import { setCurrentLaunchMode } from "./launchMode";
import { setSyncProvider } from "./storageProviders";
const witness = {created_at: "2026-10-11T10:00:00Z", deleted_at: "2026-10-11T11:00:00Z"};
const token = (sid = "a") => `e30.${btoa(JSON.stringify({sub: "actor",sid,token_type:"access"}))}.fixture`;
describe("original permanent deletion", () => {
 beforeEach(() => {
  const store = new Map<string,string>();
  const storage = {getItem:(k:string)=>store.get(k)??null,setItem:(k:string,v:string)=>{store.set(k,v)},removeItem:(k:string)=>{store.delete(k)},get length(){return store.size},key:(i:number)=>[...store.keys()][i]??null};
  vi.stubGlobal("localStorage",storage); vi.stubGlobal("window",{localStorage:storage,addEventListener:vi.fn(),removeEventListener:vi.fn()});
  vi.stubGlobal("navigator",{locks:{request:(_n:string,run:()=>unknown)=>run()}});
  setCurrentLaunchMode("server");setSyncProvider("none");api.setToken(token());
 });
 afterEach(()=>{vi.restoreAllMocks();vi.unstubAllGlobals()});
 it("retains UUID and witness through lost ACK and reload-like retry; exact ACK removes only original",async()=>{
  const calls: RequestInit[]=[];
  vi.stubGlobal("fetch",vi.fn(async(_url:string,init:RequestInit)=>{calls.push(init);throw new Error("lost ACK")}));
  await expect(permanentlyDeleteOriginalServerDocument("s","p",witness)).rejects.toThrow("lost ACK");
  const retained=listOfflineServerMutations()[0];expect(retained.id).not.toBe(retained.deletionOperationId);expect(retained.replayState).toBe("unknown");
  vi.stubGlobal("fetch",vi.fn(async(_url:string,init:RequestInit)=>{calls.push(init);return new Response(null,{status:204,headers:{"x-starlane-delete-operation":retained.deletionOperationId!}})}));
  await permanentlyDeleteOriginalServerDocument("s","p",witness);
  expect(calls[0].body).toBe(calls[1].body);expect((calls[0].headers as Record<string,string>)["x-starlane-delete-operation"]).toBe((calls[1].headers as Record<string,string>)["x-starlane-delete-operation"]);expect(listOfflineServerMutations()).toEqual([]);
 });
 it("metadata ACK durably hands original UUID and witness to cleanup without a second request",async()=>{
  const request=vi.fn(async(_url:string,init:RequestInit)=>new Response(null,{status:204,headers:{"x-starlane-delete-operation":(init.headers as Record<string,string>)["x-starlane-delete-operation"]}}));vi.stubGlobal("fetch",request);
  await permanentlyDeleteOriginalServerDocument("s","p",witness);
  expect(listOfflineServerMutations()).toEqual([]);const retained=readCleanupAttempts();expect(retained).toHaveLength(1);expect(retained[0].original_target).toEqual(witness);expect(retained[0].operation).toBe((request.mock.calls[0][1].headers as Record<string,string>)["x-starlane-delete-operation"]);expect(request).toHaveBeenCalledTimes(1);
 });
 it("failed cleanup journal write retains metadata UUID after successful server deletion",async()=>{
  const storage=localStorage;const originalSet=storage.setItem.bind(storage);vi.spyOn(storage,"setItem").mockImplementation((key,value)=>{if(key==="starlane_permanent_cleanup_v1")throw new Error("quota exhausted");originalSet(key,value);});
  vi.stubGlobal("fetch",vi.fn(async(_url:string,init:RequestInit)=>new Response(null,{status:204,headers:{"x-starlane-delete-operation":(init.headers as Record<string,string>)["x-starlane-delete-operation"]}})));
  await expect(permanentlyDeleteOriginalServerDocument("s","p",witness)).rejects.toThrow("quota exhausted");expect(listOfflineServerMutations()).toHaveLength(1);expect(listOfflineServerMutations()[0].replayState).toBe("unknown");expect(readCleanupAttempts()).toEqual([]);
 });
 it("offline replay preserves cleanup receipt before acknowledging original metadata intent",async()=>{
  const context=captureOfflineServerContext();const row=queueOfflineServerMutation(context.origin.serverBase,"/spaces/s/documents/p/trash","DELETE",{original_target:witness},context.origin.cacheScope,context.origin,undefined,crypto.randomUUID());
  vi.stubGlobal("fetch",vi.fn(async()=>new Response(null,{status:204,headers:{"x-starlane-delete-operation":row.deletionOperationId!}})));
  await replayOfflineServerMutations();expect(listOfflineServerMutations()).toEqual([]);expect(readCleanupAttempts()[0]).toEqual({version:1,operation:row.deletionOperationId,space:"s",document:"p",original_target:witness,origin:row.origin});
 });
 it("offline replay journal failure preserves exact operation for explicit review",async()=>{
  const context=captureOfflineServerContext();const row=queueOfflineServerMutation(context.origin.serverBase,"/spaces/s/documents/p/trash","DELETE",{original_target:witness},context.origin.cacheScope,context.origin,undefined,crypto.randomUUID());
  localStorage.setItem("starlane_permanent_cleanup_v1","broken original evidence");vi.stubGlobal("fetch",vi.fn(async()=>new Response(null,{status:204,headers:{"x-starlane-delete-operation":row.deletionOperationId!}})));
  await replayOfflineServerMutations();expect(listOfflineServerMutations()).toHaveLength(1);expect(listOfflineServerMutations()[0].deletionOperationId).toBe(row.deletionOperationId);expect(listOfflineServerMutations()[0].body).toEqual(row.body);expect(localStorage.getItem("starlane_permanent_cleanup_v1")).toBe("broken original evidence");
 });
 it("never rebinds retained operation to recreated page or new Trash cycle",async()=>{
  const request=vi.fn(async()=>{throw new Error("lost")});vi.stubGlobal("fetch",request);
  await expect(permanentlyDeleteOriginalServerDocument("s","p",witness)).rejects.toThrow();
  const before=JSON.stringify(listOfflineServerMutations());
  await expect(permanentlyDeleteOriginalServerDocument("s","p",{...witness,created_at:"replacement"})).rejects.toThrow("original page");
  await expect(permanentlyDeleteOriginalServerDocument("s","p",{...witness,deleted_at:"new cycle"})).rejects.toThrow("original page");
  expect(request).toHaveBeenCalledTimes(1);expect(JSON.stringify(listOfflineServerMutations())).toBe(before);
 });
 for(const status of [200,202,204]) it(`missing echo/status ${status} never clears retained UUID`,async()=>{
  vi.stubGlobal("fetch",vi.fn(async()=>new Response(null,{status})));
  await expect(permanentlyDeleteOriginalServerDocument("s","p",witness)).rejects.toThrow("unproven");expect(listOfflineServerMutations()).toHaveLength(1);
 });
 it("session changes after actual dispatch retain original without redispatch",async()=>{
  vi.stubGlobal("fetch",vi.fn(async(_url:string,init:RequestInit)=>{const operation=(init.headers as Record<string,string>)["x-starlane-delete-operation"];api.setToken(token("other"));return new Response(null,{status:204,headers:{"x-starlane-delete-operation":operation}})}));
  await expect(permanentlyDeleteOriginalServerDocument("s","p",witness)).rejects.toThrow("context changed");
  await expect(permanentlyDeleteOriginalServerDocument("s","p",witness)).rejects.toThrow("original page and session");expect(listOfflineServerMutations()).toHaveLength(1);
 });
 it("concurrent journal mutation retains both original dispatched witness and foreign bytes",async()=>{
  vi.stubGlobal("fetch",vi.fn(async(_url:string,init:RequestInit)=>{
   const row=listOfflineServerMutations()[0];localStorage.setItem(`starlane_server_cache_v1:mutation_queue:item:${encodeURIComponent(row.id)}`,JSON.stringify({...row,body:{original_target:{...witness,deleted_at:"foreign"}}}));
   return new Response(null,{status:204,headers:{"x-starlane-delete-operation":(init.headers as Record<string,string>)["x-starlane-delete-operation"]}});
  }));
  await expect(permanentlyDeleteOriginalServerDocument("s","p",witness)).rejects.toThrow("changed");const rows=listOfflineServerMutations();expect(rows).toHaveLength(2);expect(rows[0].deletionOperationId).toBe(rows[1].deletionOperationId);expect(rows.some(row=>JSON.stringify(row.body)===JSON.stringify({original_target:witness}))).toBe(true);
 });
 it("legacy destructive queue refuses before fetch and blocks next operation",async()=>{
  const c=captureOfflineServerContext();queueOfflineServerMutation(c.origin.serverBase,"/spaces/s/documents/p/trash","DELETE",undefined,c.origin.cacheScope,c.origin);
  queueOfflineServerMutation(c.origin.serverBase,"/spaces/s/documents/new","PUT",{},c.origin.cacheScope,c.origin);
  const request=vi.fn();vi.stubGlobal("fetch",request);await replayOfflineServerMutations();expect(request).not.toHaveBeenCalled();expect(listOfflineServerMutations()).toHaveLength(2);
 });
 it("legacy generic permanent deletion cannot bypass retained selection",async()=>{
  const request=vi.fn();vi.stubGlobal("fetch",request);await expect(api.fetch("/spaces/s/documents/p/trash",{method:"DELETE"})).rejects.toThrow("retained original");expect(request).not.toHaveBeenCalled();
 });
 it("requires exact nonempty original Trash identity before journaling",async()=>{
  await expect(permanentlyDeleteOriginalServerDocument("s","p",{created_at:"x",deleted_at:null})).rejects.toThrow("Reload Trash");expect(listOfflineServerMutations()).toEqual([]);
 });
});

describe("browser canonical local deletion",()=>{
 beforeEach(async()=>{
  const store=new Map<string,string>();const storage={getItem:(k:string)=>store.get(k)??null,setItem:(k:string,v:string)=>{store.set(k,v)},removeItem:(k:string)=>{store.delete(k)},get length(){return store.size},key:(i:number)=>[...store.keys()][i]??null};
  vi.stubGlobal("localStorage",storage);vi.stubGlobal("window",{localStorage:storage,addEventListener:vi.fn(),removeEventListener:vi.fn()});vi.stubGlobal("navigator",{locks:{request:(_n:string,run:()=>unknown)=>run()}});setCurrentLaunchMode("local");setSyncProvider("none");
  const {__resetLocalDataForTests}=await import("./dataClient");await __resetLocalDataForTests();
 });
 afterEach(()=>{vi.restoreAllMocks();vi.unstubAllGlobals()});
 it("commits exact original witness and UUID alongside canonical removal; reload retains queue",async()=>{
  const {dataClient}=await import("./dataClient");
  const page=await dataClient.createDocument("local-space",{title:"Original",content:"body"});await dataClient.deleteDocument("local-space",page.id);
  const row=(await dataClient.listTrashedDocuments("local-space"))[0];expect(row.created_at).toBeTruthy();
  await dataClient.permanentlyDeleteDocument("local-space",row.id,{created_at:row.created_at,deleted_at:row.deleted_at});
  const before=await dataClient.exportLocalVaultSnapshot();expect(before.documents.some(p=>p.id===row.id)).toBe(false);
  const op=before.sync_state!.queued_operations.find(o=>o.entity_id===row.id&&o.action==="delete")!;
  expect(op.id).toMatch(/^[0-9a-f-]{36}$/);expect(op.payload).toEqual({id:row.id,space_id:"local-space",original_target:{created_at:row.created_at,deleted_at:row.deleted_at}});
  const {createLocalVaultAdapter}=await import("./localVault");const after=await createLocalVaultAdapter("none").loadSnapshot();expect(after.sync_state!.queued_operations.find(o=>o.id===op.id)).toEqual(op);
 });
 it("native unbound adapter refuses before any mutation or IPC",async()=>{
  const {dataClient}=await import("./dataClient");const request=vi.fn();(window as unknown as {__TAURI_INTERNALS__:unknown}).__TAURI_INTERNALS__={invoke:request};
  await expect(dataClient.permanentlyDeleteDocument("local-space","p",witness)).rejects.toThrow("Update the desktop shell");expect(request).not.toHaveBeenCalled();
 });
 it("changed original trash witness refuses without canonical or queue changes",async()=>{
  const {dataClient}=await import("./dataClient");const page=await dataClient.createDocument("local-space",{title:"Original",content:"body"});await dataClient.deleteDocument("local-space",page.id);const row=(await dataClient.listTrashedDocuments("local-space"))[0];const before=await dataClient.exportLocalVaultSnapshot();
  await expect(dataClient.permanentlyDeleteDocument("local-space",row.id,{created_at:row.created_at,deleted_at:"other-cycle"})).rejects.toThrow("Original Trash page changed");const after=await dataClient.exportLocalVaultSnapshot();expect(after.documents).toEqual(before.documents);expect(after.sync_state!.queued_operations).toEqual(before.sync_state!.queued_operations);
 });
});
