import {afterEach,describe,expect,it,vi} from "vitest";
import {type BrowserSelectedAttachmentVersion} from "./browserAttachmentExport";
const binding=vi.hoisted(()=>({valid:true}));
vi.mock("./api",()=>({api:{getToken:()=>"original"},getApiBase:()=>"https://owned.invalid/api/v1",captureOfflineServerContext:()=>({bearer:"original",refresh:null,origin:{version:1,serverBase:"https://owned.invalid/api/v1",accountId:"owner",sessionId:"session",cacheScope:"session-test",storageMode:"server",provider:"none"}})}));
vi.mock("./offlineReplayOrigin",()=>({sameOfflineReplayOrigin:(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b),verifyOfflineReplayContext:()=>{if(!binding.valid)throw new Error("Original session changed");}}));
const origin={version:1 as const,serverBase:"https://owned.invalid/api/v1",accountId:"owner",sessionId:"session",cacheScope:"session-test",storageMode:"server" as const,provider:"none" as const};
const selected=(length=3):BrowserSelectedAttachmentVersion=>({origin,serverPage:{space_id:"third-space",document_id:"moved-page"},rawVersion:JSON.stringify({version_id:"11111111-1111-4111-8111-111111111111",logical_attachment_id:"22222222-2222-4222-8222-222222222222",original_filename:"11111111-1111-4111-8111-111111111111.bin",original_name:"Same name",mime_type:"application/octet-stream",byte_length:length,binary_sha256:"a".repeat(64)})});
function response(bytes=new Uint8Array([0,1,255])){return new Response(bytes,{headers:{"content-length":String(bytes.length)}});}
afterEach(()=>{binding.valid=true;vi.unstubAllGlobals();});
import {prepareServerSpaceStaticArchive} from "./serverStaticArchive";
import {unzipSync} from "fflate";
vi.mock("./nativeAttachmentProduct",()=>({fetchExactAttachmentVersion:async()=>selected()}));
const pin="11111111-1111-4111-8111-111111111111";
function destination(){const chunks:Uint8Array[]=[];return {chunks,write:vi.fn(async(bytes:Uint8Array)=>{chunks.push(bytes.slice());}),abort:vi.fn(async()=>{chunks.length=0;}),close:vi.fn(async()=>{})};}
function all(chunks:Uint8Array[]){const bytes=new Uint8Array(chunks.reduce((n,c)=>n+c.length,0));let at=0;for(const c of chunks){bytes.set(c,at);at+=c.length;}return bytes;}
function fixture(){let metadataValue="[]";let listing=[{id:"moved-page"}],source={id:"moved-page",title:"Original",published_metadata:{title:"Original",icon:null,cover_image:null},content:`![pin](starlane-attachment://${pin})`,published_source_digest:"a".repeat(64)};const paths:string[]=[];vi.stubGlobal("fetch",vi.fn(async(path:string)=>{paths.push(path);if(path.endsWith('/file'))return response();
if(path.endsWith('/spaces'))return new Response(JSON.stringify([{id:"third-space",name:"Original space",slug:"original"},{id:"other-space",name:"Unrelated",slug:"other"}]));
if(path.endsWith('/folders') || path.endsWith('/tags') || path.endsWith('/properties'))return new Response(metadataValue);
return new Response(JSON.stringify(path.includes('?include_nested=true')?listing:source));}));return{paths,changeMetadata:()=>{metadataValue=JSON.stringify([{id:"new-folder",name:"Changed"}]);},changeListing:()=>{listing=[{id:"other"}];},changeSource:()=>{source={...source,content:"new"};}};}
describe("original-session selected-space static archive source",()=>{
 it("enumerates nested pages and owns exact published source and pins",async()=>{const f=fixture(),w=destination(),p=await prepareServerSpaceStaticArchive("third-space",w);expect(f.paths.some(p=>p.endsWith('?include_nested=true'))).toBe(true);expect(f.paths.some(p=>p.includes("/spaces/other-space/"))).toBe(false);const files=unzipSync(all(w.chunks));expect(new TextDecoder().decode(files["sources/moved-page.md"])).toBe(`![pin](starlane-attachment://${pin})`);expect(w.close).not.toHaveBeenCalled();await p.publish();expect(w.close).toHaveBeenCalledOnce();});
 it("aborts a changed complete space listing before publication",async()=>{const f=fixture(),w=destination(),p=await prepareServerSpaceStaticArchive("third-space",w);f.changeListing();await expect(p.publish()).rejects.toThrow("listing changed");expect(w.abort).toHaveBeenCalledOnce();expect(w.close).not.toHaveBeenCalled();});
 it("aborts changed original published source before publication",async()=>{const f=fixture(),w=destination(),p=await prepareServerSpaceStaticArchive("third-space",w);f.changeSource();await expect(p.publish()).rejects.toThrow("published page changed");expect(w.abort).toHaveBeenCalledOnce();});
 it("requires proven published source and refuses partial drafts",async()=>{const w=destination();vi.stubGlobal("fetch",vi.fn(async(path:string)=>new Response(JSON.stringify(path.endsWith('/spaces')?[{id:"space",name:"Original",slug:"original"}]:path.endsWith('/folders') || path.endsWith('/tags') || path.endsWith('/properties')?[]:path.includes('?include_nested=true')?[{id:"page"}]:{id:"page",content:"draft"}))));await expect(prepareServerSpaceStaticArchive("space",w)).rejects.toThrow("proven published");expect(w.write).not.toHaveBeenCalled();expect(w.abort).toHaveBeenCalledOnce();});
 it("aborts changed original export metadata before publication",async()=>{const f=fixture(),w=destination(),p=await prepareServerSpaceStaticArchive("third-space",w);f.changeMetadata();await expect(p.publish()).rejects.toThrow("metadata changed");expect(w.abort).toHaveBeenCalledOnce();expect(w.close).not.toHaveBeenCalled();});

});
