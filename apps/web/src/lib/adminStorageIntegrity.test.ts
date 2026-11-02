import { beforeEach, describe, expect, it, vi } from "vitest";
const context = vi.hoisted(() => ({ base: "/api/v1", token: "original", refresh: "refresh", mode: "server" }));
vi.mock("./api", () => ({ api: { getToken: () => context.token, getRefreshToken: () => context.refresh }, getApiBase: () => context.base }));
vi.mock("./launchMode", () => ({ getCurrentLaunchMode: () => context.mode }));
import { parseStorageIntegrity, readStorageIntegrity } from "./adminStorageIntegrity";
const healthy = () => ({ healthy: true, documents: 1, drafts: 2, attachments: 3, issues: [] });
beforeEach(() => { Object.assign(context, { base: "/api/v1", token: "original", refresh: "refresh", mode: "server" }); vi.restoreAllMocks(); vi.stubGlobal("localStorage", {getItem: () => context.token}); });
describe("strict administrator path observation", () => {
  it("accepts actual healthy and findings contracts without turning findings into a grant", () => {
    expect(parseStorageIntegrity(healthy()).healthy).toBe(true);
    expect(parseStorageIntegrity({ ...healthy(), healthy: false, issues: [{ kind: "missing_file", record_type: "document", record_id: "id", path: "spaces/test/docs/page.md", detail: "Database record points to a missing file" }] }).issues).toHaveLength(1);
  });
  it.each([null, { ...healthy(), healthy: false }, { ...healthy(), documents: -1 }, { ...healthy(), drafts: 1.5 }, { ...healthy(), attachments: Number.MAX_SAFE_INTEGER + 1 }, { ...healthy(), issues: Array(10001).fill({}) }, { ...healthy(), healthy: false, issues: [{kind:"delete_authorized",record_type:"document",record_id:null,path:"x",detail:"x"}] }])("refuses malformed or contradictory observation", value => expect(() => parseStorageIntegrity(value)).toThrow());
  it("uses no-store and original token, then marks the prior observation invalid after origin change", async () => {
    const fetcher=vi.spyOn(globalThis,"fetch").mockResolvedValue(new Response(JSON.stringify(healthy())));
    const value=await readStorageIntegrity(new AbortController().signal); expect(value.isCurrent()).toBe(true);
    expect(fetcher).toHaveBeenCalledWith("/api/v1/admin/storage-integrity",expect.objectContaining({cache:"no-store",headers:{Authorization:"Bearer original"}}));
    context.base="/different";expect(value.isCurrent()).toBe(false);
  });
  it.each(["token","refresh","base","mode"])("refuses %s drift during a real asynchronous response", async key => {
    let resolve!: (r: Response)=>void;vi.spyOn(globalThis,"fetch").mockImplementation(()=>new Promise(r=>{resolve=r;}));
    const task=readStorageIntegrity(new AbortController().signal);context[key as keyof typeof context]="changed";resolve(new Response(JSON.stringify(healthy())));await expect(task).rejects.toThrow();
  });
  it.each([401,403,500])("does not convert HTTP %i into an empty success",async status=>{vi.spyOn(globalThis,"fetch").mockResolvedValue(new Response("{}",{status}));await expect(readStorageIntegrity(new AbortController().signal)).rejects.toThrow();});
  it("refuses response bodies above the explicit 4MiB bound", async () => {
    vi.spyOn(globalThis,"fetch").mockResolvedValue(new Response(" ".repeat(4 * 1024 * 1024 + 1)));
    await expect(readStorageIntegrity(new AbortController().signal)).rejects.toThrow();
  });
  it("refuses aborted requests and standalone context before transport",async()=>{const f=vi.spyOn(globalThis,"fetch");const c=new AbortController();c.abort();await expect(readStorageIntegrity(c.signal)).rejects.toThrow();context.mode="local";await expect(readStorageIntegrity(new AbortController().signal)).rejects.toThrow();expect(f).not.toHaveBeenCalled();});
});
