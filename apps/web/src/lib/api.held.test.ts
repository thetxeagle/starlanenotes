import { setCurrentLaunchMode } from "./launchMode";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api, replayOfflineServerMutations, captureOfflineServerContext } from "./api";
import {
  listOfflineServerMutations,
  queueOfflineServerMutation as rawQueueMutation,
  updateOfflineServerMutation,
} from "./offlineServerCache";

function replayToken(sub = "actor-a", sid = "session-a", stamp = 1) {
  return `e30.${btoa(JSON.stringify({ sub, sid, token_type: "access", exp: 9999999999, stamp }))}.fixture`;
}
function queueOfflineServerMutation(...args: Parameters<typeof rawQueueMutation>) {
  if (!api.getToken()) api.setToken(replayToken());
  const origin = captureOfflineServerContext().origin;
  return rawQueueMutation(args[0], args[1], args[2], args[3], origin.cacheScope, { ...origin, serverBase: args[0].replace(/\/+$/, "") });
}

function stubLocalStorage() {
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    get length() {
      return storage.size;
    },
    key: vi.fn((index: number) => [...storage.keys()][index] ?? null),
    getItem: vi.fn((key: string) => storage.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      storage.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      storage.delete(key);
    }),
  });
  vi.stubGlobal("window", { localStorage: globalThis.localStorage });
}

describe("durable source-bound draft automatic replay holds",()=>{
 beforeEach(()=>{vi.unstubAllGlobals();stubLocalStorage();api.logout();setCurrentLaunchMode("server");vi.stubGlobal("navigator",{onLine:true,locks:{request:async(_name:string,run:()=>Promise<unknown>)=>run()}});});
 for(const status of [403,409])it(`holds ${status} across later unlocked automatic replay events`,async()=>{
  const original=queueOfflineServerMutation("/api/v1","/spaces/s/documents/p/drafts/d","PUT",{title:"Original",content:"EXACT BYTES",source_digest:"a".repeat(64)});
  const request=vi.fn(async()=>new Response(JSON.stringify({message:"Outcome may be unknown"}),{status}));vi.stubGlobal("fetch",request);
  await replayOfflineServerMutations();const held=listOfflineServerMutations()[0];expect(held).toMatchObject({...original,replayState:"held",attempts:1,hold:{reason:"outcome_unproven"}});
  request.mockImplementation(async()=>new Response(JSON.stringify({id:"d",document_id:"p",space_id:"s",title:"Original",content:"EXACT BYTES",source_digest:"b".repeat(64)}),{status:200}));
  for(let i=0;i<4;i++)await replayOfflineServerMutations();expect(request).toHaveBeenCalledTimes(1);expect(listOfflineServerMutations()).toEqual([held]);
 });
 it("upgrades legacy attempted draft without rewriting its original or sending",async()=>{const original=queueOfflineServerMutation("/api/v1","/spaces/s/documents/p/drafts/d","PUT",{content:"RETAIN",source_digest:"a".repeat(64)});updateOfflineServerMutation({...original,attempts:2,lastError:"Server returned409"},original);const request=vi.fn();vi.stubGlobal("fetch",request);await replayOfflineServerMutations();expect(request).not.toHaveBeenCalled();expect(listOfflineServerMutations()[0]).toMatchObject({...original,attempts:2,replayState:"held",body:original.body});});
 it("writes original unknown barrier before transport sees its first request",async()=>{queueOfflineServerMutation("/api/v1","/spaces/s/documents/p/drafts/d","PUT",{content:"RETAIN",source_digest:"a".repeat(64)});const request=vi.fn(async()=>{expect(listOfflineServerMutations()[0]).toMatchObject({replayState:"unknown",body:{content:"RETAIN"}});throw Error("lostACK");});vi.stubGlobal("fetch",request);await replayOfflineServerMutations();await replayOfflineServerMutations();expect(request).toHaveBeenCalledTimes(1);expect(listOfflineServerMutations()[0].replayState).toBe("unknown");});
 it("does not dispatch if unknown barrier cannot be persisted",async()=>{queueOfflineServerMutation("/api/v1","/spaces/s/documents/p/drafts/d","PUT",{content:"RETAIN",source_digest:"a".repeat(64)});const request=vi.fn();vi.stubGlobal("fetch",request);(localStorage.setItem as ReturnType<typeof vi.fn>).mockImplementation(()=>{throw Error("quota");});await expect(replayOfflineServerMutations()).rejects.toThrow("quota");expect(request).not.toHaveBeenCalled();expect(listOfflineServerMutations()[0].body).toMatchObject({content:"RETAIN"});});
 it("retains an unfamiliar replay state as blocked evidence rather than hiding or sending it",async()=>{const original=queueOfflineServerMutation("/api/v1","/spaces/s/documents/p/drafts/d","PUT",{content:"RETAIN",source_digest:"a".repeat(64)});updateOfflineServerMutation({...original,replayState:"future-held" as unknown as typeof original.replayState},original);const request=vi.fn();vi.stubGlobal("fetch",request);await replayOfflineServerMutations();expect(request).not.toHaveBeenCalled();expect(listOfflineServerMutations()[0].id).toBe(original.id);});

it.each(['title','owner_id','access_role','can_publish','updated_at','document_title'])('partial first-attempt ACK missing %s retains the original queue as unknown',async field=>{
 const original=queueOfflineServerMutation('/api/v1','/spaces/s/documents/p/drafts/d','PUT',{content:'EXACT ORIGINAL',source_digest:'a'.repeat(64)});
 const row:Record<string,unknown>={id:'d',space_id:'s',document_id:'p',title:'Unsent retained title',document_title:'Published',content:'EXACT ORIGINAL',owner_id:'actor-a',owner_name:'Actor',access_role:'owner',can_publish:true,created_at:'created',updated_at:'updated',base_updated_at:'base',source_digest:'b'.repeat(64)};
 delete row[field];const request=vi.fn(async()=>new Response(JSON.stringify(row),{status:200}));vi.stubGlobal('fetch',request);
 await expect(replayOfflineServerMutations()).resolves.toEqual({applied:0,pending:1,blocked:1});
 expect(listOfflineServerMutations()).toEqual([expect.objectContaining({id:original.id,body:original.body,replayState:'unknown'})]);
 await replayOfflineServerMutations();expect(request).toHaveBeenCalledTimes(1);
});

});
