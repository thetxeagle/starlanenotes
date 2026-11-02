import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api, ApiError } from "./api";
import { setCurrentLaunchMode } from "./launchMode";
import { capturePageViewerTransport, parsePageViewerInspection, parsePageViewerReview, parsePageViewerConfirmed, parseSharedPages, createPageViewerUncertainty, loadPageViewerManagerProjection } from "./pageViewerManagement";
import { parsePageAccessInspection } from "./pageAccessInspection";
const proposal={user_id:"person",action:"grant" as const};
const grant={id:"grant",document_id:"page",space_id:"space",user_id:"person",role:"viewer",scope:"page",revision:"revision",created_by:"manager",created_at:"created",updated_at:"updated"};
function review(){return {schema_version:1,space_id:"space",document_id:"page",proposal,source_digest:"a".repeat(64),recipient:{id:"person",display_name:"Person",email:"person@test"},existing_grant:null};}
function report(){return {schema_version:1,space_id:"space",page_id:"page",checked_at:"2026-10-09T12:00:00Z",capabilities:{view:true,comment:false,edit:false,move:false,manage_lock:false,enable_public_link:false,disable_public_link:false},space_permission_sources:[],restrictions:{local:"inherited",ancestor_restricted:false},public_link:{configured:false,policy_allows_anonymous:false,reason:"not_enabled"},selected_private_draft:{scope:"none",view:false,edit:false},sharing:{people_grants_available:false,guests_available:false}};}
beforeEach(()=>{
  vi.restoreAllMocks();vi.unstubAllGlobals();setCurrentLaunchMode("server");
  const storage=new Map<string,string>([["starlane_launch_mode","server"]]);
  const localStorage={getItem:(key:string)=>storage.get(key)??null,setItem:(key:string,value:string)=>storage.set(key,value),removeItem:(key:string)=>storage.delete(key)};
  vi.stubGlobal("localStorage",localStorage);vi.stubGlobal("window",{localStorage,dispatchEvent:vi.fn()});api.setToken("original");api.setRefreshToken(null);
});
afterEach(()=>{vi.restoreAllMocks();vi.unstubAllGlobals();});
describe("direct page viewer original transport and projection",()=>{
  it("validates exact reviewed target/action/source before accepting confirmation",()=>{
    const checked=parsePageViewerReview(review(),"space","page",proposal);
    const reply={schema_version:1,space_id:"space",document_id:"page",user_id:"person",action:"grant",source_digest:checked.source_digest,confirmed:true,grant};
    expect(parsePageViewerConfirmed(reply,checked).grant).toEqual(grant);
    for(const changed of [{...reply,user_id:"foreign"},{...reply,source_digest:"b".repeat(64)},{...reply,action:"revoke"},{...reply,grant:{...grant,space_id:"foreign"}},{...reply,grant:null},{...reply,confirmed:false}]) expect(()=>parsePageViewerConfirmed(changed,checked)).toThrow();
  });
  it("suppresses unavailable labels and refuses malformed or duplicate projections",()=>{
    const value={schema_version:1,space_id:"space",document_id:"page",scope_notice:"Page only",grants:[{user_id:"person",revision:"revision",scope:"page",role:"viewer",display_name:null,available:false}]};
    expect(parsePageViewerInspection(value,"space","page").grants[0].display_name).toBeNull();
    expect(()=>parsePageViewerInspection({...value,grants:[{...value.grants[0],display_name:"secret"}]},"space","page")).toThrow();
    expect(()=>parsePageViewerInspection({...value,grants:[...value.grants,...value.grants]},"space","page")).toThrow();
  });
  it("keeps additive direct page provenance separate from ordinary space sources and legacy reports",()=>{
    expect(parsePageAccessInspection(report(),"space","page").space_permission_sources).toEqual([]);
    expect(parsePageAccessInspection({...report(),sharing:{people_grants_available:true,guests_available:false}},"space","page").sharing.people_grants_available).toBe(true);
    const value={...report(),page_permissions:["view"],page_permission_sources:[{kind:"direct_page_viewer",grant_id:"grant",revision:"revision",permissions:["view"]}]};
    const accepted=parsePageAccessInspection(value,"space","page");expect(accepted.page_permissions).toEqual(["view"]);expect(accepted.space_permission_sources).toEqual([]);
    for(const changed of [{...value,page_permission_sources:[{...value.page_permission_sources[0],permissions:["edit_content"]}]},{...value,page_permissions:[]},{...report(),page_permission_sources:[]},{...value,page_permission_sources:[{...value.page_permission_sources[0],kind:"guest"}]}]) expect(()=>parsePageAccessInspection(changed,"space","page")).toThrow();
  });
  it("uses original bearer without refresh/retry or cache fallback, including an unknown write",async()=>{
    const fetch=vi.fn().mockRejectedValue(new Error("lost response"));vi.stubGlobal("fetch",fetch);
    const transport=capturePageViewerTransport(()=>{});const original={proposal,source_digest:"a".repeat(64),confirm:true};
    await expect(transport("/confirm","POST",original,v=>v)).rejects.toThrow("lost response");
    expect(fetch).toHaveBeenCalledTimes(1);expect(fetch.mock.calls[0][1]).toMatchObject({cache:"no-store",redirect:"error",headers:{Authorization:"Bearer original"},body:JSON.stringify(original)});expect(api.getToken()).toBe("original");
  });
  it("refuses a changed original account after an awaited response before parsing",async()=>{
    const parse=vi.fn();vi.stubGlobal("fetch",vi.fn(async()=>{api.setToken("foreign");return new Response(JSON.stringify(review()),{status:200});}));
    const transport=capturePageViewerTransport(()=>{});await expect(transport("/review","POST",proposal,parse)).rejects.toThrow("changed");expect(parse).not.toHaveBeenCalled();
  });
  it("discovery is page presentation only and rejects foreign-shaped entries",()=>{
    const p={id:"page",space_id:"space",title:"Published",published_at:"published",updated_at:"updated",human_identifier:"PAGE-1",icon:null};
    expect(parseSharedPages({pages:[p],next_cursor:null}).pages).toEqual([p]);
    expect(()=>parseSharedPages({pages:[p,p],next_cursor:null})).toThrow();expect(()=>parseSharedPages({pages:[{...p,published_at:null}],next_cursor:null})).toThrow();
  });
  it("retains unknown original request and requires fresh inspection plus explicit separate review",()=>{
    const state=createPageViewerUncertainty();const original=parsePageViewerReview(review(),"space","page",proposal);
    state.dispatched(original);original.recipient.display_name="later label";
    expect(state.blocked()).toBe(true);expect(state.retained()[0].recipient.display_name).toBe("Person");
    expect(()=>state.dispatched(original)).toThrow();expect(()=>state.separateReview()).toThrow();
    // Fresh current state (possibly already granted) is display evidence, never ACK.
    state.inspected();expect(state.blocked()).toBe(true);expect(state.retained()).toHaveLength(1);
    state.separateReview();expect(state.blocked()).toBe(false);expect(state.retained()).toHaveLength(1);
    const separate={...original,source_digest:"b".repeat(64),proposal:{user_id:"person",action:"revoke" as const}};
    state.dispatched(separate);state.confirmed(separate);expect(state.retained()).toHaveLength(1);expect(state.retained()[0].source_digest).toBe("a".repeat(64));
  });

  it("retains admitted revoke projection when the separate entitled catalog denies selection",async()=>{
    const inspection=parsePageViewerInspection({schema_version:1,space_id:"space",document_id:"page",scope_notice:"Page only",grants:[{user_id:"person",revision:"revision",scope:"page",role:"viewer",display_name:null,available:false}]},"space","page");
    const result=await loadPageViewerManagerProjection(async()=>inspection,async()=>{throw new ApiError("Catalog denied",404);},()=>{});
    expect(result.inspection).toEqual(inspection);expect(result.users).toEqual([]);expect(result.catalog_available).toBe(false);
    await expect(loadPageViewerManagerProjection(async()=>inspection,async()=>{throw new Error("connection failed");},()=>{})).rejects.toThrow("connection failed");
  });

});
