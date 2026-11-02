import {afterEach,describe,expect,it,vi} from "vitest";
import {type BrowserSelectedAttachmentVersion} from "./browserAttachmentExport";
const binding=vi.hoisted(()=>({valid:true}));
vi.mock("./api",()=>({api:{getToken:()=>"original"},getApiBase:()=>"https://owned.invalid/api/v1",captureOfflineServerContext:()=>({bearer:"original",refresh:null,origin:{version:1,serverBase:"https://owned.invalid/api/v1",accountId:"owner",sessionId:"session",cacheScope:"session-test",storageMode:"server",provider:"none"}})}));
vi.mock("./offlineReplayOrigin",()=>({sameOfflineReplayOrigin:(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b),verifyOfflineReplayContext:()=>{if(!binding.valid)throw new Error("Original session changed");}}));
const origin={version:1 as const,serverBase:"https://owned.invalid/api/v1",accountId:"owner",sessionId:"session",cacheScope:"session-test",storageMode:"server" as const,provider:"none" as const};
const selected=(length=3):BrowserSelectedAttachmentVersion=>({origin,serverPage:{space_id:"third-space",document_id:"moved-page"},rawVersion:JSON.stringify({version_id:"11111111-1111-4111-8111-111111111111",logical_attachment_id:"22222222-2222-4222-8222-222222222222",byte_length:length,binary_sha256:"a".repeat(64)})});
function response(bytes=new Uint8Array([0,1,255])){return new Response(bytes,{headers:{"content-length":String(bytes.length)}});}
afterEach(()=>{binding.valid=true;vi.unstubAllGlobals();});
import {prepareServerSpacePortableArchive} from "./serverPortableArchive";
import {unzipSync} from "fflate";
vi.mock("./nativeAttachmentProduct",()=>({fetchExactAttachmentVersion:async()=>selected()}));
const pin="11111111-1111-4111-8111-111111111111";
function destination(){const chunks:Uint8Array[]=[];return {chunks,write:vi.fn(async(bytes:Uint8Array)=>{chunks.push(bytes.slice());}),abort:vi.fn(async()=>{chunks.length=0;}),close:vi.fn(async()=>{})};}
function all(chunks:Uint8Array[]){const bytes=new Uint8Array(chunks.reduce((n,c)=>n+c.length,0));let at=0;for(const c of chunks){bytes.set(c,at);at+=c.length;}return bytes;}
function fixture(){let listing=[{id:"moved-page"}],source={id:"moved-page",title:"Original",published_metadata:{title:"Original",icon:null,cover_image:null},content:`![pin](starlane-attachment://${pin})`,published_source_digest:"a".repeat(64)};const paths:string[]=[];vi.stubGlobal("fetch",vi.fn(async(path:string)=>{paths.push(path);return path.endsWith('/file')?response():new Response(JSON.stringify(path.includes('?include_nested=true')?listing:source));}));return{paths,changeListing:()=>{listing=[{id:"other"}];},changeSource:()=>{source={...source,content:"new"};}};}
describe("original-session server portable archive source",()=>{
 it("enumerates nested pages and owns exact published source and pins",async()=>{const f=fixture(),w=destination(),p=await prepareServerSpacePortableArchive("third-space",w);expect(f.paths.some(p=>p.endsWith('?include_nested=true'))).toBe(true);const files=unzipSync(all(w.chunks));expect(new TextDecoder().decode(files["sources/moved-page.md"])).toBe(`![pin](starlane-attachment://${pin})`);expect(w.close).not.toHaveBeenCalled();await p.publish();expect(w.close).toHaveBeenCalledOnce();});
 it("aborts a changed complete space listing before publication",async()=>{const f=fixture(),w=destination(),p=await prepareServerSpacePortableArchive("third-space",w);f.changeListing();await expect(p.publish()).rejects.toThrow("listing changed");expect(w.abort).toHaveBeenCalledOnce();expect(w.close).not.toHaveBeenCalled();});
 it("aborts changed original published source before publication",async()=>{const f=fixture(),w=destination(),p=await prepareServerSpacePortableArchive("third-space",w);f.changeSource();await expect(p.publish()).rejects.toThrow("published page changed");expect(w.abort).toHaveBeenCalledOnce();});
 it("requires proven published source and refuses partial drafts",async()=>{const w=destination();vi.stubGlobal("fetch",vi.fn(async(path:string)=>new Response(JSON.stringify(path.includes('?include_nested=true')?[{id:"page"}]:{id:"page",content:"draft"}))));await expect(prepareServerSpacePortableArchive("space",w)).rejects.toThrow("proven published");expect(w.write).not.toHaveBeenCalled();expect(w.abort).toHaveBeenCalledOnce();});
 it("requires selected presentation instead of borrowing the listing cover",async()=>{
  const w=destination();vi.stubGlobal("fetch",vi.fn(async(path:string)=>new Response(JSON.stringify(path.includes('?include_nested=true')?[{id:"page",cover_image:`starlane-attachment://${pin}`}]:{id:"page",title:"Original",content:"body",published_source_digest:"a".repeat(64)}))));
  await expect(prepareServerSpacePortableArchive("space",w)).rejects.toThrow("selected published metadata required");expect(w.write).not.toHaveBeenCalled();expect(w.abort).toHaveBeenCalledOnce();
 });
 it("captures cover-only selection and aborts changed published cover before close",async()=>{
  let cover=`starlane-attachment://${pin}`;const w=destination();vi.stubGlobal("fetch",vi.fn(async(path:string)=>path.endsWith('/file')?response():new Response(JSON.stringify(path.includes('?include_nested=true')?[{id:"page",cover_image:"ignored-listing"}]:{id:"page",title:"Original",content:"body",published_source_digest:"a".repeat(64),published_metadata:{title:"Original",icon:null,cover_image:cover}}))));
  const p=await prepareServerSpacePortableArchive("space",w);const files=unzipSync(all(w.chunks));expect(files[`assets/${pin}.bin`]).toEqual(new Uint8Array([0,1,255]));cover="https://images.invalid/new.png";await expect(p.publish()).rejects.toThrow("published page changed");expect(w.close).not.toHaveBeenCalled();expect(w.abort).toHaveBeenCalledOnce();
 });

});
