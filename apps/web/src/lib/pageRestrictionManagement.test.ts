import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api, ApiError } from "./api";
import { setCurrentLaunchMode } from "./launchMode";
import {
  applyPageRestrictions, inspectPageRestrictions, previewPageRestrictions,
  normalizeRestrictionProposal, parseRestrictionApplied, parseRestrictionInspection,
  parseRestrictionPreview, type RestrictionPreview,
} from "./pageRestrictionManagement";

export function restrictionInspectionFixture() {
  return {
    schema_version: 1, space_id: "space", document_id: "page", checked_at: "2026-10-09T12:00:00Z",
    policy: { mode: "restricted", revision: "revision", allowlist: {
      users: [{ id: "self", available: true }, { id: "unavailable", available: false }], groups: [],
    } }, eligible_recipients: { users: [{ id: "self", display_name: "Self" }], groups: [] }, can_apply: true,
  };
}
export function restrictionPreviewFixture(): RestrictionPreview {
  return {
    schema_version: 1, space_id: "space", document_id: "page", preview_token: "opaque-preview",
    expires_at: new Date(Date.now() + 600_000).toISOString(), scope_notice: "Applies to all descendants.",
    readable_pages: [{ document_id: "page", public_link_configured: true, public_link_policy_before: true, public_link_policy_after: false }],
  };
}
function applied() {
  return { schema_version: 1, space_id: "space", document_id: "page", revision: "new-revision", mode: "restricted", scope_notice: "Applies to all descendants." };
}
function held<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
beforeEach(() => {
  vi.restoreAllMocks(); vi.unstubAllGlobals(); setCurrentLaunchMode("server");
  const storage = new Map<string, string>([["starlane_launch_mode", "server"]]);
  const localStorage = { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value), removeItem: (key: string) => storage.delete(key) };
  vi.stubGlobal("localStorage", localStorage);
  vi.stubGlobal("window", { localStorage, dispatchEvent: vi.fn() });
  api.setToken("original-token"); api.setRefreshToken("seeded-refresh-token");
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
describe("fresh restriction management", () => {
  it("retains unavailable local IDs without any label lookup and rejects contradictory/extra policy data", () => {
    const value = restrictionInspectionFixture();
    expect(parseRestrictionInspection(value, "space", "page").policy.allowlist.users[1]).toEqual({ id: "unavailable", available: false });
    for (const bad of [
      { ...value, document_id: "foreign" }, { ...value, schema_version: 2 }, { ...value, hidden_pages: [] },
      { ...value, checked_at: "yesterday" }, { ...value, can_apply: "true" },
      { ...value, policy: { ...value.policy, revision: null } },
      { ...value, eligible_recipients: { ...value.eligible_recipients, users: [...value.eligible_recipients.users, ...value.eligible_recipients.users] } },
      { ...value, policy: { ...value.policy, allowlist: { users: [{ id: "unavailable", available: true }], groups: [] } } },
      { ...value, policy: { ...value.policy, allowlist: { users: [{ id: "unavailable", available: false, display_name: "SECRET" }], groups: [] } } },
    ]) expect(() => parseRestrictionInspection(bad, "space", "page")).toThrow("Unable to verify");
  });
  it("validates missing inherited policy and exact preview/apply target/revision/source shapes", () => {
    const value = restrictionInspectionFixture();
    expect(parseRestrictionInspection({ ...value, policy: { mode: "inherited", revision: null, allowlist: { users: [], groups: [] } }, can_apply: false }, "space", "page").can_apply).toBe(false);
    const review = restrictionPreviewFixture();
    for (const bad of [{ ...review, space_id: "foreign" }, { ...review, readable_pages: [...review.readable_pages, ...review.readable_pages] }, { ...review, expires_at: "never" }, { ...review, preview_token: "" }, { ...review, recipients: [] }])
      expect(() => parseRestrictionPreview(bad, "space", "page")).toThrow("Unable to verify");
    expect(parseRestrictionApplied(applied(), "space", "page", "restricted").revision).toBe("new-revision");
    for (const bad of [{ ...applied(), revision: "" }, { ...applied(), mode: "inherited" }, { ...applied(), document_id: "foreign" }, { ...applied(), grants: [] }])
      expect(() => parseRestrictionApplied(bad, "space", "page", "restricted")).toThrow("Unable to verify");
  });
  it("rejects impossible calendar dates and field/offset ranges instead of Date.parse normalization", () => {
    for (const time of ["2026-02-30T12:00:00Z", "2025-02-29T12:00:00Z", "2026-04-31T12:00:00Z", "2026-00-01T12:00:00Z", "2026-01-00T12:00:00Z", "2026-01-01T24:00:00Z", "2026-01-01T12:60:00Z", "2026-01-01T12:00:60Z", "2026-01-01T12:00:00+24:00", "2026-01-01T12:00:00+01:60"]) {
      expect(() => parseRestrictionInspection({ ...restrictionInspectionFixture(), checked_at: time }, "space", "page")).toThrow("Unable to verify");
      expect(() => parseRestrictionPreview({ ...restrictionPreviewFixture(), expires_at: time }, "space", "page")).toThrow("Unable to verify");
    }
    for (const time of ["2024-02-29T23:59:59.123Z", "2000-02-29T00:00:00+05:30", "2026-04-30T12:00:00-07:00"])
      expect(parseRestrictionInspection({ ...restrictionInspectionFixture(), checked_at: time }, "space", "page").checked_at).toBe(time);
  });
  it("treats valid-looking 201/202 confirmations as unknown and never retries them", async () => {
    for (const status of [201, 202]) {
      const request = vi.fn().mockResolvedValue(new Response(JSON.stringify(applied()), { status })); vi.stubGlobal("fetch", request);
      await expect(applyPageRestrictions("space", "page", restrictionPreviewFixture(), "restricted", () => {})).rejects.toMatchObject({ status, code: "unexpected_status" });
      expect(request).toHaveBeenCalledTimes(1); expect(request.mock.calls[0][0]).toContain("/restrictions/apply");
    }
    const request = vi.fn().mockResolvedValue(new Response(JSON.stringify(applied()), { status: 200 })); vi.stubGlobal("fetch", request);
    expect((await applyPageRestrictions("space", "page", restrictionPreviewFixture(), "restricted", () => {})).revision).toBe("new-revision");
    expect(request).toHaveBeenCalledTimes(1);
  });
  it("uses generic unavailable wording for inspection without projecting an opaque server detail", async () => {
    const request = vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: "not_found", message: "SECRET policy or participant detail" }), { status: 404 })); vi.stubGlobal("fetch", request);
    await expect(inspectPageRestrictions("space", "page", () => {})).rejects.toMatchObject({ status: 404, message: "Restriction management is unavailable for this page." });
    expect(request).toHaveBeenCalledTimes(1);
  });
  it("matches exact proposal constraints without silently deleting or rewriting identifiers", () => {
    expect(normalizeRestrictionProposal({ mode: "restricted", allowlist: { users: ["z", "a"], groups: [] } }).allowlist.users).toEqual(["a", "z"]);
    for (const value of [
      { mode: "inherited", allowlist: { users: ["self"], groups: [] } },
      { mode: "restricted", allowlist: { users: ["self", "self"], groups: [] } },
      { mode: "restricted", allowlist: { users: [" self"], groups: [] } },
      { mode: "restricted", allowlist: { users: ["é".repeat(129)], groups: [] } },
      { mode: "restricted", allowlist: { users: Array.from({ length: 101 }, (_, i) => `user${i}`), groups: [] } },
    ] as const) expect(() => normalizeRestrictionProposal(value as never)).toThrow("Unable to verify");
  });
  it("sends exactly one captured no-body inspection without the API cache/refresh/mirror pipeline", async () => {
    const spy = vi.spyOn(api, "fetch");
    const request = vi.fn().mockResolvedValue(new Response(JSON.stringify(restrictionInspectionFixture()), { status: 200 })); vi.stubGlobal("fetch", request);
    const guard = vi.fn();
    expect((await inspectPageRestrictions("space", "page", guard)).policy.revision).toBe("revision");
    expect(request).toHaveBeenCalledTimes(1); expect(spy).not.toHaveBeenCalled();
    expect(request.mock.calls[0]).toEqual(["/api/v1/spaces/space/documents/page/restrictions/inspect", { method: "POST", headers: { Authorization: "Bearer original-token" }, cache: "no-store", redirect: "error" }]);
    expect(guard.mock.calls.length).toBeGreaterThanOrEqual(4);
  });
  it("keeps seeded 401 refresh credentials untouched and never redispatches confirmation", async () => {
    const request = vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: "session_expired", message: "Sign in again" }), { status: 401 })); vi.stubGlobal("fetch", request);
    await expect(applyPageRestrictions("space", "page", restrictionPreviewFixture(), "restricted", () => {})).rejects.toMatchObject({ status: 401, code: "session_expired" });
    expect(request).toHaveBeenCalledTimes(1); expect(request.mock.calls[0][0]).toContain("/restrictions/apply");
    expect(api.getToken()).toBe("original-token"); expect(api.getRefreshToken()).toBe("seeded-refresh-token");
  });
  it("sends the normalized immutable proposal and exact token confirmation, not current cached policy", async () => {
    const request = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(restrictionPreviewFixture()), { status: 200 })).mockResolvedValueOnce(new Response(JSON.stringify(applied()), { status: 200 })); vi.stubGlobal("fetch", request);
    const review = await previewPageRestrictions("space", "page", { mode: "restricted", allowlist: { users: ["z", "self"], groups: [] } }, () => {});
    expect(JSON.parse(request.mock.calls[0][1].body)).toEqual({ mode: "restricted", allowlist: { users: ["self", "z"], groups: [] } });
    await applyPageRestrictions("space", "page", review, "restricted", () => {});
    expect(JSON.parse(request.mock.calls[1][1].body)).toEqual({ preview_token: review.preview_token, confirm: true });
  });
  it("rejects context changes during fetch without a second target dispatch", async () => {
    const response = held<Response>(); const request = vi.fn().mockReturnValue(response.promise); vi.stubGlobal("fetch", request);
    let current = true;
    const result = inspectPageRestrictions("space", "page", () => { if (!current) throw new Error("Page changed"); });
    const rejected = expect(result).rejects.toThrow("changed");
    current = false; api.setToken("new-account-token");
    response.resolve(new Response(JSON.stringify(restrictionInspectionFixture()), { status: 200 }));
    await rejected;
    expect(request).toHaveBeenCalledTimes(1);
  });
  it("checks original native base/token/account after the response JSON await", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} }); localStorage.setItem("server_url", "https://original.example.test");
    const body = held<unknown>(); const started = held<void>();
    const request = vi.fn().mockResolvedValue({ ok: true, status: 200, json: () => { started.resolve(); return body.promise; } }); vi.stubGlobal("fetch", request);
    let account = "original";
    const result = inspectPageRestrictions("space", "page", () => { if (account !== "original") throw new Error("Account changed"); });
    const failure = expect(result).rejects.toThrow("changed");
    await started.promise; account = "other"; localStorage.setItem("server_url", "https://other.example.test");
    body.resolve(restrictionInspectionFixture()); await failure;
    expect(request).toHaveBeenCalledTimes(1); expect(request.mock.calls[0][0]).toContain("https://original.example.test/api/v1/");
    expect(request.mock.calls[0][1].headers.Authorization).toBe("Bearer original-token");
  });
  it("guards rejection and error JSON boundaries and never retries uncertain outcomes", async () => {
    const body = held<unknown>(); const started = held<void>();
    const request = vi.fn().mockResolvedValue({ ok: false, status: 500, json: () => { started.resolve(); return body.promise; } }); vi.stubGlobal("fetch", request);
    const result = applyPageRestrictions("space", "page", restrictionPreviewFixture(), "restricted", () => {});
    const failure = expect(result).rejects.toThrow("changed"); await started.promise; api.setRefreshToken("different-session");
    body.reject(new Error("bad JSON")); await failure; expect(request).toHaveBeenCalledTimes(1);
  });
  it("does not dispatch an expired review or a local-mode request", async () => {
    const request = vi.fn(); vi.stubGlobal("fetch", request);
    expect(() => applyPageRestrictions("space", "page", { ...restrictionPreviewFixture(), expires_at: "2000-01-01T00:00:00Z" }, "restricted", () => {})).toThrow("expired");
    setCurrentLaunchMode("local"); await expect(inspectPageRestrictions("space", "page", () => {})).rejects.toThrow("connected server");
    expect(request).not.toHaveBeenCalled();
  });
  it("keeps genuine HTTP errors distinct from a malformed success response", async () => {
    const request = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ error: "conflict", message: "Inspect again" }), { status: 409 })).mockResolvedValueOnce(new Response(JSON.stringify({ ...restrictionInspectionFixture(), space_id: "wrong" }), { status: 200 })); vi.stubGlobal("fetch", request);
    await expect(inspectPageRestrictions("space", "page", () => {})).rejects.toBeInstanceOf(ApiError);
    await expect(inspectPageRestrictions("space", "page", () => {})).rejects.toThrow("Unable to verify");
    expect(request).toHaveBeenCalledTimes(2);
  });
});
