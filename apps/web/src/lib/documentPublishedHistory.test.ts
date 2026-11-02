import { afterEach,beforeEach,describe,expect,it,vi } from "vitest";
import {createHash,webcrypto} from "node:crypto";
import { ApiError,api } from "./api";
import {createPublicationHistoryReader,type PublicationHistoryTransport,type PublicationSnapshotSummary,publicationHistoryContextVersion,subscribePublicationHistoryContext} from "./documentPublishedHistory";
import type {OfflineReplayContext} from "./offlineReplayOrigin";
import {snapshotDetail,snapshotSummary} from "./documentPublishedHistory.fixtures";
const context:OfflineReplayContext={origin:{version:1,serverBase:"https://original.invalid/api/v1",accountId:"owner",sessionId:"sid",cacheScope:"session-cache",storageMode:"server",provider:"none"},bearer:"original-secret",refresh:null};
const envelope=(value:unknown)=>({schema_version:1,space_id:"space",document_id:"page",snapshot:value});
const list=(snapshots:unknown[])=>({schema_version:1,space_id:"space",document_id:"page",snapshots});
function fixture(){let current=true;const transport:PublicationHistoryTransport={capture:()=>structuredClone(context),verify:()=>{if(!current)throw Error("Original context changed");},fetch:vi.fn(),online:()=>true};const fetch=vi.mocked(transport.fetch);return{reader:createPublicationHistoryReader(transport),transport,fetch,drift:()=>{current=false;}};}
function rawResponse(bytes:Uint8Array,summary:PublicationSnapshotSummary,headers:Record<string,string>={}){return new Response(new Uint8Array(bytes),{headers:{"content-type":"application/octet-stream","content-length":String(bytes.length),"x-starlane-snapshot-id":summary.id,"x-starlane-snapshot-sha256":summary.content.raw_sha256,"cache-control":"no-store",...headers}});}
const withBytes=(bytes:Uint8Array):PublicationSnapshotSummary=>({...structuredClone(snapshotSummary),content:{...snapshotSummary.content,raw_byte_length:bytes.length,raw_sha256:createHash("sha256").update(bytes).digest("hex")}});
afterEach(()=>vi.unstubAllGlobals());
beforeEach(()=>{vi.restoreAllMocks();vi.stubGlobal("crypto",webcrypto);});
describe("current-context publication snapshot reader",()=>{
 it("uses current guarded uncached GETs and retains historical captured space without mistaking it for current route",async()=>{
  const f=fixture();const generic=vi.spyOn(api,"fetch");f.fetch.mockResolvedValue(new Response(JSON.stringify(list([snapshotSummary]))));
  expect(await f.reader.list("space","page")).toEqual([snapshotSummary]);
  expect(f.fetch).toHaveBeenCalledWith("https://original.invalid/api/v1/spaces/space/documents/page/publication-snapshots",{method:"GET",headers:{Authorization:"Bearer original-secret"},cache:"no-store"});expect(generic).not.toHaveBeenCalled();
 });
 it("does not fetch offline, locally, or with an invalid requested route",async()=>{
  const f=fixture();f.transport.online=()=>false;await expect(f.reader.list("space","page")).rejects.toThrow("Reconnect");expect(f.fetch).not.toHaveBeenCalled();
  f.transport.online=()=>true;f.transport.capture=()=>({...context,origin:{...context.origin,storageMode:"local"}});await expect(f.reader.list("space","page")).rejects.toThrow("local vaults");expect(f.fetch).not.toHaveBeenCalled();await expect(f.reader.get("", "page","id")).rejects.toThrow("unproven");
 });
 it.each(["space_id","document_id"])("rejects a foreign %s envelope before admitting content",async(field)=>{const f=fixture();f.fetch.mockResolvedValue(new Response(JSON.stringify({...envelope(snapshotDetail),[field]:"foreign"})));await expect(f.reader.get("space","page",snapshotDetail.id)).rejects.toThrow("unproven");});
 it("rejects foreign or duplicate rows and unsupported schemas",async()=>{for(const value of [list([{...snapshotSummary,document_id:"foreign"}]),list([snapshotSummary,snapshotSummary]),{...list([]),schema_version:2}]){const f=fixture();f.fetch.mockResolvedValue(new Response(JSON.stringify(value)));await expect(f.reader.list("space","page")).rejects.toThrow("unproven");}});
 it("rejects context drift after HTTP and during decoded body await",async()=>{
  const f=fixture();f.fetch.mockImplementation(async()=>{f.drift();return new Response(JSON.stringify(list([])));});await expect(f.reader.list("space","page")).rejects.toThrow("Original context changed");
  const g=fixture();g.fetch.mockResolvedValue({status:200,json:async()=>{g.drift();return list([snapshotSummary]);}} as Response);await expect(g.reader.list("space","page")).rejects.toThrow("Original context changed");
 });
 it("preserves exact raw portable content and sanitized metadata while discarding unexpected internal fields",async()=>{
  const f=fixture();f.fetch.mockResolvedValue(new Response(JSON.stringify(envelope({...snapshotDetail,storage_path:"must-not-admit",comparisons:{...snapshotDetail.comparisons,properties_schema:[{...snapshotDetail.comparisons.properties_schema[0],space_id:"internal"}]}}))));
  const actual=await f.reader.get("space","page",snapshotDetail.id,snapshotSummary);expect(actual.content.portable_markdown).toBe(snapshotDetail.content.portable_markdown);expect(actual.comparisons).toEqual(snapshotDetail.comparisons);expect(actual).not.toHaveProperty("storage_path");expect(actual.comparisons.properties_schema[0]).not.toHaveProperty("space_id");
 });
 it("rejects malformed or duplicated metadata rather than hiding observations behind duplicate UI keys",async()=>{
  for(const comparisons of [{...snapshotDetail.comparisons,attachments:[{...snapshotDetail.comparisons.attachments[0],size_bytes:-1}]},{...snapshotDetail.comparisons,property_values:[...snapshotDetail.comparisons.property_values,snapshotDetail.comparisons.property_values[0]]},{...snapshotDetail.comparisons,properties_schema:[{...snapshotDetail.comparisons.properties_schema[0],options_raw:{secret:true}}]}]){const f=fixture();f.fetch.mockResolvedValue(new Response(JSON.stringify(envelope({...snapshotDetail,comparisons}))));await expect(f.reader.get("space","page",snapshotDetail.id)).rejects.toThrow("unproven");}
 });
 it("requires exact selected snapshot summary and safe current-route raw URL",async()=>{
  for(const value of [{...snapshotDetail,snapshot_digest:"c".repeat(64)},{...snapshotDetail,raw_download_url:"https://foreign.invalid/bytes"},{...snapshotDetail,raw_download_url:"/api/v1/spaces/old-space/documents/page/publication-snapshots/op%3Apublication/raw"}]){const f=fixture();f.fetch.mockResolvedValue(new Response(JSON.stringify(envelope(value))));await expect(f.reader.get("space","page",snapshotDetail.id,snapshotSummary)).rejects.toThrow("unproven");}
 });
 it("does not reconstruct raw-only portable content or accept presentation/policy mismatches",async()=>{
  for(const value of [{...snapshotDetail,content:{...snapshotDetail.content,extraction:"raw_only"}},{...snapshotDetail,asset_policy:"retained_bytes"},{...snapshotDetail,binding:"legacy_content_v1"}]){const f=fixture();f.fetch.mockResolvedValue(new Response(JSON.stringify(envelope(value))));await expect(f.reader.get("space","page",snapshotDetail.id)).rejects.toThrow("unproven");}
 });
 it("preserves current permission rejection instead of substituting cached history",async()=>{const f=fixture();f.fetch.mockResolvedValue(new Response('{"message":"Edit required"}',{status:403}));await expect(f.reader.list("space","page")).rejects.toMatchObject({status:403,message:"Edit required"});});
 it("rejects success without the required 200 response",async()=>{const f=fixture();f.fetch.mockResolvedValue(new Response(null,{status:204}));await expect(f.reader.list("space","page")).rejects.toBeInstanceOf(ApiError);});
});
describe("exact raw publication source read",()=>{
 it("validates ID/hash/length and preserves UTF-8 BOM, CRLF, whitespace and unknown header",async()=>{
  const text='\uFEFF---\r\nunknown: keep\r\n---\r\n\r\n  raw body  ';const bytes=new TextEncoder().encode(text);const expected=withBytes(bytes);const f=fixture();f.fetch.mockResolvedValue(rawResponse(bytes,expected));const result=await f.reader.raw("space","page",expected);expect(result.text).toBe(text);expect(result.bytes).toEqual(bytes);
 });
 it("preserves invalid UTF-8 bytes without inventing replacement text",async()=>{const bytes=new Uint8Array([255,254,0,1]);const expected=withBytes(bytes);const f=fixture();f.fetch.mockResolvedValue(rawResponse(bytes,expected));const result=await f.reader.raw("space","page",expected);expect(result.text).toBeNull();expect(result.bytes).toEqual(bytes);});
 it.each<Record<string,string>>([{"content-length":"999"},{"x-starlane-snapshot-id":"foreign"},{"x-starlane-snapshot-sha256":"c".repeat(64)},{"content-type":"text/html"},{"cache-control":"public"}])("rejects an unbound raw response %j",async(headers)=>{const bytes=new TextEncoder().encode("original");const expected=withBytes(bytes);const f=fixture();f.fetch.mockResolvedValue(rawResponse(bytes,expected,headers));await expect(f.reader.raw("space","page",expected)).rejects.toThrow("unproven");});
 it("rejects same-length byte drift despite matching copied headers",async()=>{const expected=withBytes(new TextEncoder().encode("original"));const f=fixture();f.fetch.mockResolvedValue(rawResponse(new TextEncoder().encode("modified"),expected));await expect(f.reader.raw("space","page",expected)).rejects.toThrow("unproven");});
 it("freezes selected proof across await and rejects current context drift",async()=>{
  const bytes=new TextEncoder().encode("original");const expected=withBytes(bytes);const f=fixture();f.fetch.mockImplementation(async()=>{expected.content.raw_sha256="c".repeat(64);return rawResponse(bytes,withBytes(bytes));});expect((await f.reader.raw("space","page",expected)).text).toBe("original");
  const g=fixture();const response=rawResponse(bytes,withBytes(bytes));response.arrayBuffer=async()=>{g.drift();return bytes.buffer;};g.fetch.mockResolvedValue(response);await expect(g.reader.raw("space","page",withBytes(bytes))).rejects.toThrow("Original context changed");
 });
});

describe("reactive history context clearing",()=>{
 function storageFixture(){
  const values=new Map<string,string>([["access_token","original"],["refresh_token","refresh"],["starlane_server_cache_v1:session-scope","session"]]);
  const storage={getItem:(key:string)=>values.get(key)??null};const windowTarget=new EventTarget();const documentTarget=new EventTarget();
  vi.stubGlobal("localStorage",storage);vi.stubGlobal("window",Object.assign(windowTarget,{localStorage:storage}));vi.stubGlobal("document",documentTarget);vi.spyOn(api,"getToken").mockReturnValue("original");return{values,windowTarget,documentTarget};
 }
 it("changes an opaque generation for token/cache/provider/launch changes and even switch-back",()=>{
  const f=storageFixture();let old=publicationHistoryContextVersion();expect(publicationHistoryContextVersion()).toBe(old);
  for(const [key,value] of [["access_token","other"],["refresh_token","other-refresh"],["starlane_server_cache_v1:session-scope","other-session"],["starlane_sync_provider","google-drive"],["starlane_storage_mode","local"]]){f.values.set(key,value);const next=publicationHistoryContextVersion();expect(next).toBeGreaterThan(old);old=next;}
  f.values.set("access_token","original");expect(publicationHistoryContextVersion()).toBeGreaterThan(old);expect(typeof publicationHistoryContextVersion()).toBe("number");
 });
 it("checks same-tab storage changes before the next paint and stops all owned listeners",()=>{
  const f=storageFixture();let frame:FrameRequestCallback=()=>{};const request=vi.fn((callback:FrameRequestCallback)=>{frame=callback;return 7;});const cancel=vi.fn();vi.stubGlobal("requestAnimationFrame",request);vi.stubGlobal("cancelAnimationFrame",cancel);const changed=vi.fn();const stop=subscribePublicationHistoryContext(changed);
  f.values.set("starlane_sync_provider","google-drive");frame(0);expect(changed).toHaveBeenCalledTimes(1);
  f.values.set("access_token","other");f.windowTarget.dispatchEvent(new Event("storage"));expect(changed).toHaveBeenCalledTimes(2);
  stop();f.values.set("access_token","third");f.windowTarget.dispatchEvent(new Event("focus"));f.documentTarget.dispatchEvent(new Event("visibilitychange"));frame(0);expect(changed).toHaveBeenCalledTimes(2);expect(cancel).toHaveBeenCalledWith(7);
 });
});
