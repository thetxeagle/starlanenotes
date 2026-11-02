import {afterEach,describe,expect,it,vi} from "vitest";
import {type BrowserSelectedAttachmentVersion} from "./browserAttachmentExport";
const binding=vi.hoisted(()=>({valid:true}));
vi.mock("./api",()=>({api:{getToken:()=>"original"},getApiBase:()=>"https://owned.invalid/api/v1",captureOfflineServerContext:()=>({bearer:"original",refresh:null,origin:{version:1,serverBase:"https://owned.invalid/api/v1",accountId:"owner",sessionId:"session",cacheScope:"session-test",storageMode:"server",provider:"none"}})}));
vi.mock("./offlineReplayOrigin",()=>({sameOfflineReplayOrigin:(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b),verifyOfflineReplayContext:()=>{if(!binding.valid)throw new Error("Original session changed");}}));
const origin={version:1 as const,serverBase:"https://owned.invalid/api/v1",accountId:"owner",sessionId:"session",cacheScope:"session-test",storageMode:"server" as const,provider:"none" as const};
const selected=(length=3):BrowserSelectedAttachmentVersion=>({origin,serverPage:{space_id:"third-space",document_id:"moved-page"},rawVersion:JSON.stringify({version_id:"11111111-1111-4111-8111-111111111111",logical_attachment_id:"22222222-2222-4222-8222-222222222222",byte_length:length,binary_sha256:"a".repeat(64)})});
function response(bytes=new Uint8Array([0,1,255])){return new Response(bytes,{headers:{"content-length":String(bytes.length)}});}
afterEach(()=>{binding.valid=true;vi.unstubAllGlobals();});
import {prepareBrowserPortableArchive} from "./browserPortableArchive";
import {unzipSync} from "fflate";
function destination(){const chunks:Uint8Array[]=[];return {chunks,write:vi.fn(async(bytes:Uint8Array)=>{chunks.push(bytes.slice());}),abort:vi.fn(async()=>{chunks.length=0;}),close:vi.fn(async()=>{})};}
function all(chunks:Uint8Array[]){const b=new Uint8Array(chunks.reduce((n,c)=>n+c.length,0));let o=0;for(const c of chunks){b.set(c,o);o+=c.length;}return b;}
const pin="11111111-1111-4111-8111-111111111111";
describe("exact portable Markdown archive",()=>{
 it("owns original bytes, exact references and raw metadata without closing destination",async()=>{const s=selected(),w=destination(),content=`![old](starlane-attachment://${pin})`;vi.stubGlobal("fetch",vi.fn(async()=>response()));const p=await prepareBrowserPortableArchive([{documentId:"page",content,verifyCurrent:async()=>{}}],[{selected:s,refresh:async()=>s}],w);const archive=unzipSync(all(w.chunks));expect(archive[`assets/${pin}.bin`]).toEqual(new Uint8Array([0,1,255]));expect(new TextDecoder().decode(archive["pages/page.md"])).toBe(`![old](../assets/${pin}.bin)`);expect(new TextDecoder().decode(archive["sources/page.md"])).toBe(content);expect(new TextDecoder().decode(archive[`metadata/${pin}.json`])).toBe(s.rawVersion);expect(w.close).not.toHaveBeenCalled();await p.publish();expect(w.close).toHaveBeenCalledOnce();});
 it("refuses an omitted pin before writing",async()=>{const w=destination();await expect(prepareBrowserPortableArchive([{documentId:"page",content:`![old](starlane-attachment://${pin})`,verifyCurrent:async()=>{}}],[],w)).rejects.toThrow("unavailable");expect(w.write).not.toHaveBeenCalled();expect(w.abort).toHaveBeenCalledOnce();});
 it("aborts changed original page before publication",async()=>{const s=selected(),w=destination();let current=true;vi.stubGlobal("fetch",vi.fn(async()=>response()));const p=await prepareBrowserPortableArchive([{documentId:"page",content:`![old](starlane-attachment://${pin})`,verifyCurrent:async()=>{if(!current)throw Error("page replaced");}}],[{selected:s,refresh:async()=>s}],w);current=false;await expect(p.publish()).rejects.toThrow("page replaced");expect(w.abort).toHaveBeenCalledOnce();expect(w.close).not.toHaveBeenCalled();});
 it("exports a cover-only immutable asset with original and rewritten presentation sidecars",async()=>{
  const s=selected(),w=destination(),pinRef=`starlane-attachment://${pin}`;vi.stubGlobal("fetch",vi.fn(async()=>response()));
  const p=await prepareBrowserPortableArchive([{documentId:"page",content:"Body without images",publishedMetadata:{title:"Cover only",icon:"star",cover_image:pinRef},verifyCurrent:async()=>{}}],[{selected:s,refresh:async()=>s}],w);
  const files=unzipSync(all(w.chunks));expect(files[`assets/${pin}.bin`]).toEqual(new Uint8Array([0,1,255]));
  expect(JSON.parse(new TextDecoder().decode(files["pages/page.metadata.json"])).cover_image).toBe(`../assets/${pin}.bin`);
  expect(JSON.parse(new TextDecoder().decode(files["sources/page.metadata.json"])).cover_image).toBe(pinRef);await p.publish();expect(w.close).toHaveBeenCalledOnce();
 });
 it("refuses a cover-only asset absent from exact selections before writes",async()=>{
  const w=destination();await expect(prepareBrowserPortableArchive([{documentId:"page",content:"body",publishedMetadata:{title:"Cover",icon:null,cover_image:`starlane-attachment://${pin}`},verifyCurrent:async()=>{}}],[],w)).rejects.toThrow("unavailable");expect(w.write).not.toHaveBeenCalled();expect(w.abort).toHaveBeenCalledOnce();
 });

});
