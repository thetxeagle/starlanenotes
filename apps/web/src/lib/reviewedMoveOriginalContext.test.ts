import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { captureReviewedMoveContext, prepareReviewedMove, dispatchReviewedMove, completeReviewedMove, reviewedServerMoves } from "./reviewedServerMoves";
import { setCurrentLaunchMode } from "./launchMode";
import { setStorageMode, setSyncProvider } from "./storageProviders";
import { listOfflineServerMutations } from "./offlineServerCache";
import type { ServerMoveCommit } from "@starlane-notes/shared-types";
const token = (sub = "owner-a") => `e30.${btoa(JSON.stringify({ sub, sid: `session-${sub}`, token_type: "access" }))}.fixture`;
const request = (): ServerMoveCommit => ({ fingerprint: "original-source", decisions: [], incident_relations: [], public_links: "preserve", confirm_audience_change: true, confirm_draft_space_change: true });
describe("consumed original reviewed move transport and completion", () => {
  beforeEach(() => {
    const values = new Map<string, string>();
    const storage = { get length() { return values.size; }, key: (i: number) => [...values.keys()][i] ?? null, getItem: (k: string) => values.get(k) ?? null, setItem: (k: string, v: string) => { values.set(k, v); }, removeItem: (k: string) => { values.delete(k); } };
    vi.stubGlobal("localStorage", storage); vi.stubGlobal("window", { localStorage: storage }); vi.stubGlobal("navigator", { onLine: true });
    setCurrentLaunchMode("server"); setStorageMode("server"); setSyncProvider("none"); api.setToken(token());
  });
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
  it("same historical request after unknown409 reuses exact original copied bytes", async () => {
    const original = captureReviewedMoveContext(); const mutable = request(); const pending = prepareReviewedMove(original, "preview-a", mutable); const bytes = pending.requestJson;
    mutable.fingerprint = "caller-edited-B";
    const fetcher = vi.fn(async (_url: unknown, _init?: RequestInit) => new Response(JSON.stringify({ message: "Move outcome unknown" }), { status: 409 })); vi.stubGlobal("fetch", fetcher);
    await expect(dispatchReviewedMove(pending)).rejects.toMatchObject({ status: 409 });
    fetcher.mockImplementation(async () => new Response(JSON.stringify({ operation_id: "original-op", committed: true, historical_receipt: true }), { status: 200 }));
    expect(await dispatchReviewedMove(pending)).toMatchObject({ operation_id: "original-op", historical_receipt: true });
    expect(fetcher.mock.calls.map(call => call[1]?.body)).toEqual([bytes, bytes]); expect(listOfflineServerMutations()).toEqual([]);
  });
  for (const status of [401, 403, 404, 409]) it(`later${status} leaves original pending object and bytes intact`, async () => {
    const pending = prepareReviewedMove(captureReviewedMoveContext(), "preview-a", request()); const bytes = pending.requestJson;
    const fetcher = vi.fn(async () => new Response('{"message":"Original unavailable"}', { status })); vi.stubGlobal("fetch", fetcher);
    await expect(dispatchReviewedMove(pending)).rejects.toMatchObject({ status }); expect(pending.requestJson).toBe(bytes); expect(pending.id).toBe("preview-a"); expect(fetcher).toHaveBeenCalledTimes(1); expect(listOfflineServerMutations()).toEqual([]);
  });
  it("accepted A preview context cannot dispatch identical route under B", async () => {
    const original = captureReviewedMoveContext(); const pending = prepareReviewedMove(original, "preview-a", request()); api.setToken(token("owner-b")); const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
    await expect(dispatchReviewedMove(pending)).rejects.toThrow(); await expect(reviewedServerMoves.cancel("preview-a", original)).rejects.toThrow(); expect(fetcher).not.toHaveBeenCalled();
  });
  it("offline same request never enters the generic queue", async () => {
    const pending = prepareReviewedMove(captureReviewedMoveContext(), "preview-a", request()); vi.stubGlobal("navigator", { onLine: false }); const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
    await expect(dispatchReviewedMove(pending)).rejects.toThrow("not dispatched"); expect(fetcher).not.toHaveBeenCalled(); expect(listOfflineServerMutations()).toEqual([]);
  });
  for (const status of [201, 202]) it(`valid-looking${status} cannot acknowledge a move`, async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response('{"operation_id":"op","committed":true}', { status })));
    await expect(dispatchReviewedMove(prepareReviewedMove(captureReviewedMoveContext(), "preview-a", request()))).rejects.toThrow("unconfirmed");
  });
  it("cancel requires actual empty204 without a committed-body substitute", async () => {
    const original = captureReviewedMoveContext(); const fetcher = vi.fn(async () => new Response('{}', { status: 200 })); vi.stubGlobal("fetch", fetcher);
    await expect(reviewedServerMoves.cancel("preview-a", original)).rejects.toThrow("unconfirmed"); fetcher.mockImplementation(async () => new Response(null, { status: 204 })); await expect(reviewedServerMoves.cancel("preview-a", original)).resolves.toBeUndefined();
  });
  it("held actual200 response cannot admit A outcome after disk-only token drift", async () => {
    const pending = prepareReviewedMove(captureReviewedMoveContext(), "preview-a", request()); let release!: () => void; const hold = new Promise<void>(r => { release = r; }); let reached!: () => void; const ready = new Promise<void>(r => { reached = r; });
    vi.stubGlobal("fetch", vi.fn(async () => { reached(); await hold; return new Response('{"operation_id":"op","committed":true}', { status: 200 }); }));
    const rejected = expect(dispatchReviewedMove(pending)).rejects.toThrow("unconfirmed"); await ready; localStorage.setItem("access_token", token("owner-b")); release(); await rejected;
  });
  it("held JSON text cannot admit A outcome after provider drift", async () => {
    const pending = prepareReviewedMove(captureReviewedMoveContext(), "preview-a", request()); let release!: () => void; const hold = new Promise<void>(r => { release = r; }); let reached!: () => void; const ready = new Promise<void>(r => { reached = r; });
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, status: 200, text: async () => { reached(); await hold; return '{"operation_id":"op","committed":true}'; } })));
    const rejected = expect(dispatchReviewedMove(pending)).rejects.toThrow("unconfirmed"); await ready; setSyncProvider("google-drive"); release(); await rejected;
  });
  it("actual Sidebar completion seam never navigates B after held refresh", async () => {
    const context = captureReviewedMoveContext(); let release!: () => void; const hold = new Promise<void>(r => { release = r; }); let selected = true; const finish = vi.fn();
    const rejected = expect(completeReviewedMove(context, () => selected, () => hold, finish)).rejects.toThrow("selection changed"); selected = false; release(); await rejected; expect(finish).not.toHaveBeenCalled();
  });
  it("unchanged Sidebar completion refreshes then finishes once", async () => {
    const refresh = vi.fn(async () => {}); const finish = vi.fn(); await completeReviewedMove(captureReviewedMoveContext(), () => true, refresh, finish); expect(refresh).toHaveBeenCalledTimes(1); expect(finish).toHaveBeenCalledTimes(1);
  });
});
