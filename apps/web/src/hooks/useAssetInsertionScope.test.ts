import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Editor as TipTapEditor } from "@tiptap/react";
import { createAssetInsertionScopeOwner, AssetInsertionScopeError } from "./useAssetInsertionScope";

const state = vi.hoisted(() => ({
  launch: "local",
  mode: "local",
  provider: "none",
  base: "/api/v1",
  token: null as string | null,
  valid: true,
}));
vi.mock("@/lib/api", () => ({
  api: { getToken: () => state.token },
  getApiBase: () => state.base,
  captureOfflineServerContext: vi.fn(() => ({ originalToken: state.token })),
}));
vi.mock("@/lib/launchMode", () => ({ getCurrentLaunchMode: () => state.launch }));
vi.mock("@/lib/storageProviders", () => ({
  getStorageSelection: () => ({ mode: state.mode, provider: state.provider }),
}));
vi.mock("@/lib/offlineReplayOrigin", () => ({
  verifyOfflineReplayContext: vi.fn(() => {
    if (!state.valid) throw new Error("original session changed");
  }),
}));

const selection = { space_id: "space", document_id: "page", draft_id: "A" };
const editor = () => ({ isEditable: true, isDestroyed: false }) as TipTapEditor;

describe("asset insertion render ownership", () => {
  beforeEach(() => {
    Object.assign(state, {
      launch: "local",
      mode: "local",
      provider: "none",
      base: "/api/v1",
      token: null,
      valid: true,
    });
    vi.clearAllMocks();
  });
  it("preserves a capture across ordinary typing/unchanged renders", () => {
    const owner = createAssetInsertionScopeOwner(),
      ed = editor(),
      capture = owner.render(ed, selection),
      verify = capture();
    expect(owner.render(ed, { ...selection })).toBe(capture);
    verify();
  });
  it("permanently rejects A→B→A operations and stale original event callbacks", () => {
    const owner = createAssetInsertionScopeOwner(),
      ed = editor(),
      captureA = owner.render(ed, selection),
      verifyA = captureA();
    owner.render(ed, { ...selection, draft_id: "B" });
    expect(() => captureA()).toThrow(AssetInsertionScopeError);
    const newCaptureA = owner.render(ed, { ...selection });
    expect(() => verifyA()).toThrow(AssetInsertionScopeError);
    expect(() => captureA()).toThrow(AssetInsertionScopeError);
    newCaptureA()();
  });
  it("invalidates editor replacement, readonly transitions and return to editable", () => {
    const owner = createAssetInsertionScopeOwner(),
      ed = editor(),
      capture = owner.render(ed, selection),
      verify = capture();
    owner.render(ed, { ...selection, readOnly: true });
    owner.render(ed, selection);
    expect(verify).toThrow(AssetInsertionScopeError);
    const next = owner.render(ed, selection)();
    owner.render(editor(), selection);
    expect(next).toThrow(AssetInsertionScopeError);
  });
  it("refuses readonly, destroyed or noneditable at capture and before adoption", () => {
    for (const field of ["isDestroyed", "isEditable"] as const) {
      const owner = createAssetInsertionScopeOwner(),
        ed = editor(),
        capture = owner.render(ed, selection),
        verify = capture();
      Object.assign(ed, { [field]: field === "isDestroyed" });
      expect(() => capture()).toThrow(AssetInsertionScopeError);
      expect(verify).toThrow(AssetInsertionScopeError);
    }
    expect(() =>
      createAssetInsertionScopeOwner().render(editor(), { ...selection, readOnly: true })(),
    ).toThrow(AssetInsertionScopeError);
  });
  it("invalidates unmount and does not revive pending work after StrictMode reactivation", () => {
    const owner = createAssetInsertionScopeOwner(),
      capture = owner.render(editor(), selection),
      verify = capture();
    owner.dispose();
    expect(() => capture()).toThrow(AssetInsertionScopeError);
    expect(verify).toThrow(AssetInsertionScopeError);
    owner.activate();
    expect(verify).toThrow(AssetInsertionScopeError);
    capture()();
  });
  it("rejects provider and launch changes, including observed round trips", () => {
    const owner = createAssetInsertionScopeOwner(),
      ed = editor(),
      capture = owner.render(ed, selection),
      verify = capture();
    state.provider = "google-drive";
    owner.render(ed, selection);
    state.provider = "none";
    owner.render(ed, selection);
    expect(verify).toThrow(AssetInsertionScopeError);
    expect(() => capture()).toThrow(AssetInsertionScopeError);
    const current = owner.render(ed, selection)();
    state.launch = "server";
    expect(current).toThrow(AssetInsertionScopeError);
  });
  it("captures original authenticated server context and permanently refuses failed verification", () => {
    Object.assign(state, { launch: "server", mode: "server", token: "original" });
    const owner = createAssetInsertionScopeOwner(),
      capture = owner.render(editor(), selection),
      verify = capture();
    verify();
    state.valid = false;
    expect(verify).toThrow(AssetInsertionScopeError);
    state.valid = true;
    expect(verify).toThrow(AssetInsertionScopeError);
  });
  it("a stale server/base callback cannot acquire the new environment", () => {
    Object.assign(state, { launch: "server", mode: "server", token: "original" });
    const owner = createAssetInsertionScopeOwner(),
      ed = editor(),
      capture = owner.render(ed, selection);
    state.base = "https://other.invalid/api/v1";
    owner.render(ed, selection);
    expect(() => capture()).toThrow(AssetInsertionScopeError);
    state.token = "other";
    expect(() => owner.render(ed, selection)()()).not.toThrow();
  });
});
