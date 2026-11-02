import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api, captureOfflineServerContext } from "./api";
import { setCurrentLaunchMode } from "./launchMode";
import { setSyncProvider } from "./storageProviders";
import { fetchOriginalCleanup, reconcileAcknowledgedCleanup } from "./originalCleanupTransport";
import { readCleanupAttempts, retainAcknowledgedCleanup, type CleanupAttempt } from "./permanentDocumentCleanup";
const operation = "11111111-2222-3333-4444-555555555555";
const token = (sid = "original") => `e30.${btoa(JSON.stringify({ sub: "actor", sid, token_type: "access" }))}.fixture`;
let context: ReturnType<typeof captureOfflineServerContext>;
let attempt: CleanupAttempt;
const complete = () => ({ operation_id: operation, metadata_deleted: true, outcome: "physically_completed", reason: null, assets: [{ asset_id: "original-version", outcome: "physically_completed", reason: null }] });
beforeEach(() => {
  const store = new Map<string, string>();
  const storage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, v); }, removeItem: (k: string) => { store.delete(k); }, get length() { return store.size; }, key: (i: number) => [...store.keys()][i] ?? null };
  vi.stubGlobal("localStorage", storage); vi.stubGlobal("window", { localStorage: storage, addEventListener: vi.fn(), removeEventListener: vi.fn() });
  vi.stubGlobal("navigator", {locks:{request:(_name:string,fn:()=>unknown)=>fn()}});
  setCurrentLaunchMode("server"); setSyncProvider("none"); api.setToken(token());
  context = captureOfflineServerContext();
  attempt = { version: 1, operation, space: "space", document: "original", original_target: { created_at: "raw birth", deleted_at: "raw Trash" }, origin: context.origin };
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
describe("bounded original cleanup transport", () => {
  it("sends exact original UUID/witness once and accepts separately proved physical completion", async () => {
    const request = vi.fn(async () => Response.json(complete())); vi.stubGlobal("fetch", request);
    expect((await fetchOriginalCleanup(context, attempt)).outcome).toBe("physically_completed");
    expect(request).toHaveBeenCalledTimes(1);
    expect(request.mock.calls[0]).toEqual([`${context.origin.serverBase}/spaces/space/documents/original/permanent-deletions/${operation}/cleanup`, { method: "POST", headers: { Authorization: `Bearer ${context.bearer}`, "Content-Type": "application/json" }, body: JSON.stringify({ original_target: attempt.original_target }) }]);
  });
  it("refuses foreign original sessions before dispatch", async () => {
    const request = vi.fn(); vi.stubGlobal("fetch", request); attempt.origin = { ...attempt.origin, sessionId: "other" };
    await expect(fetchOriginalCleanup(context, attempt)).rejects.toThrow("session differs"); expect(request).not.toHaveBeenCalled();
  });
  it("does not refresh or redispatch a metadata acknowledgment or denied cleanup", async () => {
    for (const status of [204, 401, 403, 409]) {
      const request = vi.fn(async () => new Response(null, { status })); vi.stubGlobal("fetch", request);
      await expect(fetchOriginalCleanup(context, attempt)).rejects.toThrow("acknowledgment unavailable"); expect(request).toHaveBeenCalledTimes(1);
    }
  });
  it("refuses an oversized streamed body without trusting absent length headers", async () => {
    const stream = new ReadableStream<Uint8Array>({ start(controller) { controller.enqueue(new Uint8Array(65537)); controller.close(); } });
    vi.stubGlobal("fetch", vi.fn(async () => new Response(stream)));
    await expect(fetchOriginalCleanup(context, attempt)).rejects.toThrow("exceeds bound");
  });
  it("cannot publish completion after the original session changes while reading", async () => {
    const stream = new ReadableStream<Uint8Array>({ pull(controller) { api.setToken(token("replacement")); controller.enqueue(new TextEncoder().encode(JSON.stringify(complete()))); controller.close(); } });
    const request = vi.fn(async () => new Response(stream)); vi.stubGlobal("fetch", request);
    await expect(fetchOriginalCleanup(context, attempt)).rejects.toThrow(); expect(request).toHaveBeenCalledTimes(1);
  });
  it("refuses another operation's otherwise valid completion", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Response.json({ ...complete(), operation_id: "66666666-2222-3333-4444-555555555555" })));
    await expect(fetchOriginalCleanup(context, attempt)).rejects.toThrow("unproven");
  });
  it("caller mutation after dispatch cannot retarget the held operation", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => { attempt.operation = "66666666-2222-3333-4444-555555555555"; return Response.json(complete()); }));
    expect((await fetchOriginalCleanup(context, attempt)).operation_id).toBe(operation);
  });
});

describe("explicit acknowledged cleanup reconciliation",()=>{
 it("removes only completed original receipt and preserves a different page",async()=>{
  await retainAcknowledgedCleanup(attempt,()=>{});const other={...attempt,operation:"66666666-2222-3333-4444-555555555555",document:"other"};await retainAcknowledgedCleanup(other,()=>{});
  vi.stubGlobal("fetch",vi.fn(async()=>Response.json(complete())));expect((await reconcileAcknowledgedCleanup(attempt)).outcome).toBe("physically_completed");expect(readCleanupAttempts()).toEqual([other]);
 });
 it.each(["pending","retained_review"])("keeps the exact receipt for %s and reuses original identity on manual retry",async outcome=>{
  await retainAcknowledgedCleanup(attempt,()=>{});const request=vi.fn(async()=>Response.json({...complete(),outcome,assets:[{asset_id:"original-version",outcome,reason:"bounded_original_review"}]}));vi.stubGlobal("fetch",request);
  expect((await reconcileAcknowledgedCleanup(attempt)).outcome).toBe(outcome);expect(readCleanupAttempts()).toEqual([attempt]);await reconcileAcknowledgedCleanup(attempt);expect(request).toHaveBeenCalledTimes(2);expect(request.mock.calls[0]).toEqual(request.mock.calls[1]);
 });
 it("missing or changed selected receipt never dispatches or creates identity",async()=>{
  const request=vi.fn();vi.stubGlobal("fetch",request);await expect(reconcileAcknowledgedCleanup(attempt)).rejects.toThrow("absent");await retainAcknowledgedCleanup({...attempt,document:"replacement"},()=>{});await expect(reconcileAcknowledgedCleanup(attempt)).rejects.toThrow("changed");expect(request).not.toHaveBeenCalled();expect(readCleanupAttempts()[0].document).toBe("replacement");
 });
 it("foreign session keeps original receipt without dispatch",async()=>{
  await retainAcknowledgedCleanup(attempt,()=>{});api.setToken(token("foreign"));const request=vi.fn();vi.stubGlobal("fetch",request);await expect(reconcileAcknowledgedCleanup(attempt)).rejects.toThrow("session differs");expect(request).not.toHaveBeenCalled();expect(readCleanupAttempts()).toEqual([attempt]);
 });
 it("receipt substitution during request cannot erase the new evidence",async()=>{
  await retainAcknowledgedCleanup(attempt,()=>{});const replacement={...attempt,document:"replacement"};vi.stubGlobal("fetch",vi.fn(async()=>{localStorage.setItem("starlane_permanent_cleanup_v1",JSON.stringify([replacement]));return Response.json(complete());}));
  await expect(reconcileAcknowledgedCleanup(attempt)).rejects.toThrow("receipt changed");expect(readCleanupAttempts()).toEqual([replacement]);
 });
});
