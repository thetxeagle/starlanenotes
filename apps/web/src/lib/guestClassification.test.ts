import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { setCurrentLaunchMode } from "./launchMode";
import { setStorageMode, setSyncProvider } from "./storageProviders";
import { accountKindLabel, canReviewGuestAccount, canUseHumanAdministration, captureGuestClassificationTransport, createGuestClassificationSession, createGuestClassificationUncertainty, parseGuestAccountRows, parseGuestClassificationConfirmed, parseGuestClassificationReview } from "./guestClassification";
const target={id:"reader",email:"fictional@example.invalid",display_name:"Reader",role:"member",is_active:true,created_at:"2026-10-09",account_kind:"member" as const};
const review={schema_version:1 as const,target:{id:"reader",display_name:"Reader",account_kind:"member" as const},source_digest:"a".repeat(64),expires_at:"2099-01-01T00:00:00Z"};
const confirmed={confirmed:true,user_id:"reader",account_kind:"guest"};
const token=(actor="admin-a")=>`e30.${btoa(JSON.stringify({sub:actor,sid:`${actor}-session`,token_type:"access"}))}.fixture`;
beforeEach(()=>{
  const values=new Map<string,string>();const storage={get length(){return values.size;},key:(i:number)=>[...values.keys()][i]??null,getItem:(k:string)=>values.get(k)??null,setItem:(k:string,v:string)=>{values.set(k,v);},removeItem:(k:string)=>{values.delete(k);}};
  vi.stubGlobal("localStorage",storage);vi.stubGlobal("window",{localStorage:storage});vi.stubGlobal("navigator",{onLine:true});setCurrentLaunchMode("server");setStorageMode("server");setSyncProvider("none");api.setToken(token());
});
afterEach(()=>{api.setToken(null);vi.unstubAllGlobals();vi.restoreAllMocks();});
describe("actual original guest classification transport and controller",()=>{
  it("uses fresh exact200 original review then one explicit confirm without generic refresh/cache",async()=>{
    const generic=vi.spyOn(api,"fetch");const fetcher=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(review),{status:200})).mockResolvedValueOnce(new Response(JSON.stringify(confirmed),{status:200}));vi.stubGlobal("fetch",fetcher);
    const session=createGuestClassificationSession("reader",()=>{});const returned=await session.review();returned.source_digest="b".repeat(64);
    expect(await session.confirm()).toEqual(confirmed);expect(fetcher.mock.calls[1][1]).toMatchObject({method:"POST",cache:"no-store",body:JSON.stringify({source_digest:review.source_digest,confirm:true}),headers:{Authorization:`Bearer ${token()}`}});
    await expect(session.confirm()).rejects.toThrow();expect(fetcher).toHaveBeenCalledTimes(2);expect(generic).not.toHaveBeenCalled();expect(session.uncertainty.retained()).toEqual([]);
  });
  it.each([201,202,204,401,403,404,409,500])("retains dispatched source on status %i and refuses automatic retry",async(status)=>{
    const fetcher=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(review),{status:200})).mockResolvedValueOnce(new Response(status===204?null:JSON.stringify(confirmed),{status}));vi.stubGlobal("fetch",fetcher);
    const session=createGuestClassificationSession("reader",()=>{});await session.review();await expect(session.confirm()).rejects.toThrow();expect(session.uncertainty.blocked()).toBe(true);expect(session.uncertainty.retained()).toEqual([review]);await expect(session.confirm()).rejects.toThrow();expect(fetcher).toHaveBeenCalledTimes(2);
  });
  it.each([{...confirmed,user_id:"foreign"},{...confirmed,account_kind:"member"},{confirmed:false,user_id:"reader",account_kind:"guest"},null,{...confirmed,token:"extra"}])("treats malformed200 as unknown without synthesized account success %j",async(value)=>{
    vi.stubGlobal("fetch",vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(review),{status:200})).mockResolvedValueOnce(new Response(JSON.stringify(value),{status:200})));
    const session=createGuestClassificationSession("reader",()=>{});await session.review();await expect(session.confirm()).rejects.toThrow();expect(session.uncertainty.retained()).toEqual([review]);
  });
  it("keeps network failure retained and fresh review separate from original acknowledgement",async()=>{
    const second={...review,source_digest:"b".repeat(64)};const fetcher=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(review))).mockRejectedValueOnce(new Error("network lost")).mockResolvedValueOnce(new Response(JSON.stringify(second))).mockResolvedValueOnce(new Response(JSON.stringify(confirmed)));vi.stubGlobal("fetch",fetcher);
    const retained=createGuestClassificationUncertainty();const first=createGuestClassificationSession("reader",()=>{},retained);await first.review();await expect(first.confirm()).rejects.toThrow();
    const reopened=createGuestClassificationSession("reader",()=>{},retained);await reopened.review();expect(retained.blocked()).toBe(true);await expect(reopened.confirm()).rejects.toThrow();expect(fetcher).toHaveBeenCalledTimes(3);
    reopened.separateReview();expect(retained.retained()).toEqual([review]);await reopened.confirm();expect(retained.retained()).toEqual([review]);
  });
  it("does not dispatch a second confirmation while the original response is held",async()=>{
    let release!:(value:Response)=>void;
    const held=new Promise<Response>(resolve=>{release=resolve;});
    const fetcher=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(review))).mockReturnValueOnce(held);
    vi.stubGlobal("fetch",fetcher);
    const session=createGuestClassificationSession("reader",()=>{});
    await session.review();const original=session.confirm();
    await expect(session.confirm()).rejects.toThrow();expect(fetcher).toHaveBeenCalledTimes(2);
    release(new Response(JSON.stringify(confirmed)));expect(await original).toEqual(confirmed);
    expect(session.uncertainty.retained()).toEqual([]);
  });
  it("a failed separate fresh review cannot release an uncertain original request",async()=>{
    const fetcher=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(review)))
      .mockRejectedValueOnce(new Error("lost confirmation"))
      .mockResolvedValueOnce(new Response(JSON.stringify({error:"denied"}),{status:403}));
    vi.stubGlobal("fetch",fetcher);
    const retained=createGuestClassificationUncertainty();
    const original=createGuestClassificationSession("reader",()=>{},retained);
    await original.review();await expect(original.confirm()).rejects.toThrow();
    const fresh=createGuestClassificationSession("reader",()=>{},retained);
    await expect(fresh.review()).rejects.toThrow();expect(()=>fresh.separateReview()).toThrow();
    expect(retained.blocked()).toBe(true);expect(retained.retained()).toEqual([review]);
    await expect(fresh.confirm()).rejects.toThrow();expect(fetcher).toHaveBeenCalledTimes(3);
  });
  it("expiry blocks confirmation before any dispatch",async()=>{
    const fetcher=vi.fn().mockResolvedValue(new Response(JSON.stringify({...review,expires_at:"2000-01-01T00:00:00Z"})));vi.stubGlobal("fetch",fetcher);const session=createGuestClassificationSession("reader",()=>{});await session.review();await expect(session.confirm()).rejects.toThrow("expired");expect(fetcher).toHaveBeenCalledTimes(1);expect(session.uncertainty.retained()).toEqual([]);
  });
  it("rejects changed selected target before review or confirm dispatch",async()=>{
    let current=true;const guard=()=>{if(!current)throw new Error("selected target changed");};const fetcher=vi.fn().mockResolvedValue(new Response(JSON.stringify(review)));vi.stubGlobal("fetch",fetcher);const session=createGuestClassificationSession("reader",guard);current=false;await expect(session.review()).rejects.toThrow();expect(fetcher).not.toHaveBeenCalled();
  });
  it("rejects account change before dispatch and after a held response without re-enrollment",async()=>{
    const fetcher=vi.fn();vi.stubGlobal("fetch",fetcher);const transport=captureGuestClassificationTransport(()=>{});api.setToken(token("foreign"));await expect(transport.review("reader")).rejects.toThrow();expect(fetcher).not.toHaveBeenCalled();
    api.setToken(token());let release!:(v:Response)=>void;const held=new Promise<Response>(resolve=>{release=resolve;});fetcher.mockReturnValue(held);const session=createGuestClassificationSession("reader",()=>{});const pending=session.review();api.setToken(token("foreign"));release(new Response(JSON.stringify(review)));await expect(pending).rejects.toThrow();expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it("checks original context after JSON await and retains original confirmed-dispatch unknown",async()=>{
    const fetcher=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(review))).mockResolvedValueOnce({status:200,json:async()=>{api.setToken(token("foreign"));return confirmed;}});vi.stubGlobal("fetch",fetcher);const session=createGuestClassificationSession("reader",()=>{});await session.review();await expect(session.confirm()).rejects.toThrow();expect(session.uncertainty.retained()).toEqual([review]);
  });
  it("checks provider and native server base changes before dispatch",async()=>{
    const fetcher=vi.fn();vi.stubGlobal("fetch",fetcher);const provider=captureGuestClassificationTransport(()=>{});setSyncProvider("google-drive");await expect(provider.users()).rejects.toThrow();expect(fetcher).not.toHaveBeenCalled();setSyncProvider("none");
    Object.assign(window,{__TAURI_INTERNALS__:{}});localStorage.setItem("server_url","http://original.invalid");const server=captureGuestClassificationTransport(()=>{});localStorage.setItem("server_url","http://foreign.invalid");await expect(server.users()).rejects.toThrow();expect(fetcher).not.toHaveBeenCalled();
  });
  it("fresh users never uses cached fallback and malformed present kind cannot promote",async()=>{
    const generic=vi.spyOn(api,"fetch");const fetcher=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify([target]))).mockResolvedValueOnce(new Response(JSON.stringify([{...target,account_kind:null}])));vi.stubGlobal("fetch",fetcher);const transport=captureGuestClassificationTransport(()=>{});expect(await transport.users()).toEqual([target]);await expect(transport.users()).rejects.toThrow();expect(generic).not.toHaveBeenCalled();
  });
});
describe("finite real kind and strict source parsing",()=>{
  it("permits only active other ordinary member-kind; no synthetic guest role",()=>{
    expect(canReviewGuestAccount(target,"admin-a")).toBe(true);
    for(const altered of [{...target,id:"admin-a"},{...target,is_active:false},{...target,role:"admin"},{...target,account_kind:"guest" as const},{...target,account_kind:undefined}])expect(canReviewGuestAccount(altered,"admin-a")).toBe(false);
    expect(accountKindLabel("guest")).toBe("Guest");expect(canUseHumanAdministration("admin","guest")).toBe(false);expect(canUseHumanAdministration("admin",null)).toBe(false);expect(canUseHumanAdministration("admin",undefined)).toBe(true);
  });
  it.each([{...review,target:{...review.target,id:"other"}},{...review,source_digest:"A".repeat(64)},{...review,expires_at:"2026-02-30T00:00:00Z"},{...review,expires_at:"2099-01-01T25:00:00Z"},{...review,target:{...review.target,account_kind:"guest"}},{...review,role:"guest"}])("rejects malformed source/target/expiry %j",value=>{expect(()=>parseGuestClassificationReview(value,"reader")).toThrow();});
  it("accepts old kind omission for display only but rejects malformed present kind and duplicates",()=>{
    const legacy: Omit<typeof target,"account_kind"> & {account_kind?:typeof target.account_kind}={...target};delete legacy.account_kind;expect(parseGuestAccountRows([legacy])).toEqual([legacy]);for(const rows of [[target,target],[{...target,account_kind:"unknown"}],[{...target,account_kind:null}]])expect(()=>parseGuestAccountRows(rows)).toThrow();expect(parseGuestClassificationConfirmed(confirmed,"reader")).toEqual(confirmed);
  });
});
