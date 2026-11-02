import { afterEach, describe, expect, it, vi } from "vitest";
import {
  createDraftPeerTransport, draftPeerSocketUrl, parseDraftPeerFrame,
  type DraftPeerKey, type DraftPeerSnapshot, type DraftPeerSocket,
} from "./draftPeerTransport";

const key: DraftPeerKey = { space_id: "space-a", document_id: "page-a", draft_id: "draft-a" };
const connection = "11111111-1111-4111-8111-111111111111";
const peer = "22222222-2222-4222-8222-222222222222";
const room = "33333333-3333-4333-8333-333333333333";
const nextRoom = "44444444-4444-4444-8444-444444444444";
const source = "a".repeat(64);
const draft: DraftPeerSnapshot = {
  id: key.draft_id, document_id: key.document_id, space_id: key.space_id,
  owner_id: "owner-a", owner_name: "Owner", document_title: "Published", title: "Private",
  content: "<!-- retained CSS\r\n\u0000 -->\r\nA😀BC", icon: null, cover_image: "", base_updated_at: "base",
  access_role: "editor", can_publish: false, created_at: "created", updated_at: "updated",
  previous_folder_id: null, previous_parent_id: "parent-a", source_digest: source,
};
const snapshot = (sequence = 0, overrides: Record<string, unknown> = {}) => ({ type: "snapshot", version: 1, key, connection_id: connection, room_id: room, sequence, draft, ...overrides });
const presence = (sequence = 1, overrides: Record<string, unknown> = {}) => ({ type: "presence", version: 1, key, room_id: room, sequence, user_id: "owner-a", connection_id: peer, state: "active", ...overrides });
const invalidation = (sequence = 1) => ({ type: "invalidate", version: 1, key, room_id: room, sequence, source_digest: "b".repeat(64), reason: "committed" });
class Socket implements DraftPeerSocket {
  readyState = 0; bufferedAmount = 0;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: (() => void) | null = null; onerror: (() => void) | null = null;
  sent: string[] = [];
  send = vi.fn((value: string) => { this.sent.push(value); });
  close = vi.fn(() => { this.readyState = 3; });
  open() { this.readyState = 1; this.onopen?.(); }
  receive(value: unknown) { this.onmessage?.({ data: JSON.stringify(value) }); }
}
function fixture(overrides: Partial<Parameters<typeof createDraftPeerTransport>[0]> = {}) {
  vi.useFakeTimers();
  let current = true;
  const sockets: Socket[] = [], urls: string[] = [];
  const onSnapshot = vi.fn(), onPeer = vi.fn(), onInvalidation = vi.fn(), onStatus = vi.fn();
  const options = {
    apiBase: "https://notes.example.test/prefix/api/v1", pageUrl: "https://notes.example.test/page/a", bearer: "captured-fictional-token",
    key: { ...key }, mode: "edit" as const, isCurrent: () => current,
    onSnapshot, onPeer, onInvalidation, onStatus,
    socketFactory: (url: string) => { urls.push(url); const socket = new Socket(); sockets.push(socket); return socket; },
    ...overrides,
  };
  const transport = createDraftPeerTransport(options);
  transport.start();
  function authenticate(socket = sockets[sockets.length - 1]) {
    socket.open(); socket.receive({ type: "authenticated", version: 1, connection_id: connection });
  }
  function ready() { authenticate(); sockets[sockets.length - 1].receive(snapshot()); }
  return { transport, sockets, urls, options, ready, authenticate, onSnapshot, onPeer, onInvalidation, onStatus, stale: () => { current = false; } };
}
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });

describe("draft peer wire validation", () => {
  it("preserves API deployment prefix and keeps all credentials out of the URL", () => {
    expect(draftPeerSocketUrl("/api/v1", "https://notes.example.test/page?a=1#details")).toBe("wss://notes.example.test/ws/drafts/v1");
    expect(draftPeerSocketUrl("http://localhost:3400/team/api/v1/", "https://notes.example.test")).toBe("ws://localhost:3400/team/ws/drafts/v1");
    for (const base of ["https://u:p@notes.example.test/api/v1", "/api/v1?token=x", "/api/v1#x", "file:///api/v1", "wss://notes.example.test/api/v1", "/api/v2"]) {
      expect(() => draftPeerSocketUrl(base, "https://notes.example.test")).toThrow();
    }
    expect(() => draftPeerSocketUrl("/api/v1", "file:///page")).toThrow();
    expect(draftPeerSocketUrl("https://server.example.test/prefix/api/v1", "tauri://localhost/page")).toBe("wss://server.example.test/prefix/ws/drafts/v1");
    expect(() => draftPeerSocketUrl("/api/v1", "tauri://localhost/page")).toThrow();
    expect(() => draftPeerSocketUrl("", "https://notes.example.test/api/v1")).toThrow();
  });
  it("preserves exact raw content and nullable metadata without normalizing a snapshot", () => {
    const frame = parseDraftPeerFrame(JSON.stringify(snapshot()), key);
    expect(frame?.type).toBe("snapshot");
    if (frame?.type !== "snapshot") throw new Error("Missing snapshot");
    expect(frame.draft).toEqual(draft);
    expect(frame.draft.content).toContain("\r\n\u0000");
    expect(frame.draft.icon).toBeNull(); expect(frame.draft.cover_image).toBe("");
  });
  it.each([
    snapshot(0, { version: 2 }), snapshot(0, { key: { ...key, draft_id: "other" } }),
    snapshot(0, { draft: { ...draft, id: "other" } }), snapshot(0, { draft: { ...draft, source_digest: "A".repeat(64) } }),
    snapshot(0, { draft: { ...draft, icon: undefined } }), snapshot(0, { draft: { ...draft, access_role: "admin" } }),
    snapshot(0, { room_id: "not-uuid" }), snapshot(Number.MAX_SAFE_INTEGER + 1), snapshot(-1),
    { ...presence(), injected: "unknown" }, { type: "body", version: 1, content: "write" },
  ])("refuses malformed/cross-draft/unsupported envelopes %#", value => {
    expect(parseDraftPeerFrame(JSON.stringify(value), key)).toBeNull();
  });
  it("rejects binary and frames over the UTF8 output bound", () => {
    expect(parseDraftPeerFrame(new Uint8Array([1]), key)).toBeNull();
    // UTF8 bytes exceed32MiB although JS UTF16 length does not.
    expect(parseDraftPeerFrame('"' + "é".repeat(16 * 1024 * 1024) + '"', key)).toBeNull();
  });
});

describe("captured scoped advisory controller", () => {
  it("sends captured token first only, then exact subscribe, with no save/ACK callbacks", () => {
    const f = fixture();
    f.options.bearer = "new-storage-token"; f.options.key.draft_id = "later-selection";
    f.ready();
    expect(f.urls).toEqual(["wss://notes.example.test/prefix/ws/drafts/v1"]);
    expect(f.urls[0]).not.toContain("token");
    expect(f.sockets[0].sent[0]).toBe("captured-fictional-token");
    expect(JSON.parse(f.sockets[0].sent[1])).toEqual({ type: "subscribe", key, mode: "edit" });
    expect(f.onSnapshot).toHaveBeenCalledTimes(1);
    expect(f.onStatus.mock.calls.flat()).not.toContain("saved");
    expect(f.transport).toHaveProperty("cursor");
    f.transport.dispose();
  });
  it("quietly disposes stale scope before dispatch and rejects old socket callbacks", () => {
    const f = fixture(); f.ready();
    const late = f.sockets[0].onmessage!;
    f.stale(); late({ data: JSON.stringify(presence()) });
    expect(f.onPeer).not.toHaveBeenCalled(); expect(f.sockets[0].close).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(60000); expect(f.sockets).toHaveLength(1);
    expect(f.onStatus).not.toHaveBeenCalledWith("disposed");
  });
  it("disposes on context inspection failure without callbacks or retry", () => {
    const f = fixture({ isCurrent: () => { throw new Error("changed"); } });
    expect(f.sockets).toHaveLength(0); expect(f.onStatus).not.toHaveBeenCalled();
  });
  it("rechecks scope after a callback before sending any subsequent command", () => {
    let active = true;
    const f = fixture({ isCurrent: () => active, onStatus: status => { if (status === "subscribing") active = false; } });
    f.authenticate();
    expect(f.sockets[0].sent).toEqual(["captured-fictional-token"]);
    expect(f.sockets[0].close).toHaveBeenCalledTimes(1);
  });
  it("terminates unavailable without reconnecting or delivering further source", () => {
    const f = fixture(); f.ready();
    const late = f.sockets[0].onmessage!;
    f.sockets[0].receive({ type: "unavailable", version: 1 });
    late({ data: JSON.stringify(snapshot(2)) }); vi.advanceTimersByTime(60000);
    expect(f.onSnapshot).toHaveBeenCalledTimes(1); expect(f.onStatus).toHaveBeenLastCalledWith("unavailable");
    expect(f.sockets).toHaveLength(1);
  });
  it.each([snapshot(0, { connection_id: peer }), snapshot(0, { key: { ...key, space_id: "other" } }), { bad: true }])("fails closed on forged identity/malformed source %#", value => {
    const f = fixture(); f.authenticate(); f.sockets[0].receive(value);
    expect(f.onSnapshot).not.toHaveBeenCalled(); expect(f.onStatus).toHaveBeenLastCalledWith("failed");
    vi.advanceTimersByTime(60000); expect(f.sockets).toHaveLength(1);
  });
  it("ignores duplicate/older events, preserves same-user peers, and coalesces gaps into a fresh advisory snapshot", () => {
    const f = fixture(); f.ready(); const socket = f.sockets[0];
    socket.receive(presence(1)); socket.receive(presence(1)); socket.receive(presence(0));
    expect(f.onPeer).toHaveBeenCalledTimes(1);
    socket.receive(presence(4)); socket.receive(presence(5));
    expect(socket.sent.filter(text => text.startsWith("{") && JSON.parse(text).type === "resync")).toHaveLength(1);
    expect(f.onPeer).toHaveBeenCalledTimes(1);
    socket.receive(snapshot(5)); socket.receive(snapshot(5));
    expect(f.onSnapshot).toHaveBeenCalledTimes(2);
    socket.receive(presence(6)); expect(f.onPeer).toHaveBeenCalledTimes(2);
    f.transport.dispose();
  });
  it("coalesces invalidations without treating matching source as a save acknowledgement", () => {
    const f = fixture(); f.ready(); const socket = f.sockets[0];
    socket.receive(invalidation(1)); socket.receive(invalidation(2));
    expect(f.onInvalidation).toHaveBeenCalledTimes(2); expect(f.onSnapshot).toHaveBeenCalledTimes(1);
    expect(socket.sent).toHaveLength(3);
    socket.receive(snapshot(2, { draft: { ...draft, source_digest: "b".repeat(64) } }));
    expect(f.onSnapshot).toHaveBeenCalledTimes(2);
    expect(f.onStatus.mock.calls.flat()).not.toContain("synced");
    f.transport.dispose();
  });
  it("allows equal-sequence manual fresh projection without changing raw source", () => {
    const f = fixture(); f.ready(); expect(f.transport.resync()).toBe(true); expect(f.transport.resync()).toBe(false);
    f.sockets[0].receive(snapshot(0, { draft: { ...draft, owner_name: "Changed name" } }));
    expect(f.onSnapshot).toHaveBeenCalledTimes(2);
    f.transport.dispose();
  });
  it("requires a fresh connection/room snapshot on reconnect and never rereads a changed token", () => {
    const f = fixture(); f.ready();
    const oldMessage = f.sockets[0].onmessage!;
    f.options.bearer = "changed-token"; f.sockets[0].onclose?.();
    vi.advanceTimersByTime(249); expect(f.sockets).toHaveLength(1);
    vi.advanceTimersByTime(1); f.authenticate();
    expect(f.sockets[1].sent[0]).toBe("captured-fictional-token");
    oldMessage({ data: JSON.stringify(snapshot(99)) }); expect(f.onSnapshot).toHaveBeenCalledTimes(1);
    f.sockets[1].receive(snapshot(0, { room_id: nextRoom }));
    expect(f.onSnapshot).toHaveBeenCalledTimes(2); f.transport.dispose();
  });
  it("resets consecutive failure budget only after a validated fresh snapshot", () => {
    const f = fixture(); f.ready();
    for (let i = 0; i < 8; i++) {
      f.sockets[f.sockets.length - 1].onclose?.();
      vi.advanceTimersByTime(250);
      f.ready();
    }
    expect(f.sockets).toHaveLength(9);
    expect(f.onSnapshot).toHaveBeenCalledTimes(9);
    expect(f.onStatus).not.toHaveBeenCalledWith("failed");
    f.transport.dispose();
  });
  it("bounds attempts/timeouts and disposal cancels all reconnect work", () => {
    const f = fixture();
    vi.advanceTimersByTime(200000);
    expect(f.sockets).toHaveLength(6); expect(f.onStatus).toHaveBeenLastCalledWith("failed");
    const g = fixture(); g.sockets[0].onclose?.(); g.transport.dispose();
    vi.advanceTimersByTime(60000); expect(g.sockets).toHaveLength(1);
  });
  it("bounds advisory frames and only publishes exact snapshot-bound UTF16 cursor positions", () => {
    const f = fixture(); f.ready();
    let admitted = 0;
    const emoji = draft.content.indexOf("😀");
    expect(f.transport.cursor(source, emoji, emoji + 2)).toBe(true);
    expect(JSON.parse(f.sockets[0].sent[2])).toEqual({type:"cursor",key,source_digest:source,anchor:emoji,head:emoji+2});
    expect(f.transport.cursor(source, emoji + 1, emoji + 2)).toBe(false);
    expect(f.transport.cursor("b".repeat(64), 0, 0)).toBe(false);
    for (let i = 0; i < 100; i++) if (f.transport.presence("active")) admitted++;
    expect(admitted).toBe(27); // Captured bearer, subscribe and cursor occupy three of30 slots.
    expect(f.sockets[0].sent.every(text => new TextEncoder().encode(text).length <= 16384)).toBe(true);
    vi.advanceTimersByTime(1000); f.sockets[0].bufferedAmount = 16385;
    expect(f.transport.presence("idle")).toBe(false);
    f.transport.dispose();
  });
  it("refuses cursor publication from view mode or while a resync is pending", () => {
    const viewer = fixture({mode:"view"}); viewer.ready();
    expect(viewer.transport.cursor(source,0,0)).toBe(false);
    const editor = fixture(); editor.ready(); expect(editor.transport.resync()).toBe(true);
    expect(editor.transport.cursor(source,0,0)).toBe(false);
    viewer.transport.dispose(); editor.transport.dispose();
  });
  it("parses peer cursors only against matching exact raw UTF16 source", () => {
    const f = fixture(); f.ready();
    const index = draft.content.indexOf("😀");
    f.sockets[0].receive({ ...presence(), type: "cursor", state: undefined, source_digest: source, anchor: index, head: index + 2 });
    expect(f.onPeer).toHaveBeenCalledTimes(1);
    f.sockets[0].receive({ ...presence(2), type: "cursor", state: undefined, source_digest: source, anchor: index + 1, head: index + 2 });
    expect(f.onStatus).toHaveBeenLastCalledWith("failed");
    expect(f.onPeer).toHaveBeenCalledTimes(1);
  });
  it("resyncs a cursor source mismatch without publishing stale positions", () => {
    const f = fixture(); f.ready();
    f.sockets[0].receive({ ...presence(), type: "cursor", state: undefined, source_digest: "b".repeat(64), anchor: 0, head: 0 });
    expect(f.onPeer).not.toHaveBeenCalled(); expect(f.sockets[0].sent).toHaveLength(3);
    f.transport.dispose();
  });
});


const roster = (revision=1, overrides: Record<string,unknown>={}) => ({
  type:"presence_roster",version:1,key,room_id:room,presence_revision:revision,connection_id:connection,
  peers:[{user_id:"editor-a",connection_id:connection,display_name:"Editor",state:"active",mode:"edit"},
    {user_id:"owner-a",connection_id:peer,display_name:"Owner",state:"idle",mode:"view"}],...overrides,
});
describe("negotiated authoritative participant rosters",()=>{
  it("negotiates explicitly and replaces membership independently of source sequence or save callbacks",()=>{
    const onRoster=vi.fn();const f=fixture({presence:true,onRoster});f.ready();
    expect(JSON.parse(f.sockets[0].sent[1])).toEqual({type:"subscribe",key,mode:"edit",presence_version:1});
    f.sockets[0].receive(roster(100));f.sockets[0].receive(roster(101));
    expect(onRoster).toHaveBeenCalledTimes(2);expect(f.onSnapshot).toHaveBeenCalledTimes(1);
    expect(f.onPeer).not.toHaveBeenCalled();expect(f.sockets[0].sent).toHaveLength(2);
    f.sockets[0].receive(invalidation(1));
    expect(f.onInvalidation).toHaveBeenCalledTimes(1);
    expect(f.sockets[0].sent.filter(text=>text.startsWith("{")&&JSON.parse(text).type==="resync")).toHaveLength(1);
    f.transport.dispose();
  });
  it("accepts fresh equal-revision rosters but never restores an older membership",()=>{
    const onRoster=vi.fn();const f=fixture({presence:true,onRoster});f.ready();
    const first=roster(3);const ownOnly=roster(4,{peers:first.peers.slice(0,1)});
    f.sockets[0].receive(first);f.sockets[0].receive(ownOnly);f.sockets[0].receive(first);f.sockets[0].receive(ownOnly);
    expect(onRoster.mock.calls.map(([frame])=>frame?.peers.length)).toEqual([2,1,1]);f.transport.dispose();
  });
  it.each([
    roster(1,{connection_id:peer}),roster(1,{room_id:nextRoom}),roster(1,{presence_revision:-1}),
    roster(1,{peers:[]}),roster(1,{peers:[roster().peers[1]]}),
    roster(1,{peers:[roster().peers[0],roster().peers[0]]}),
    roster(1,{peers:[{...roster().peers[0],display_name:"é".repeat(129)}]}),
    roster(1,{peers:[{...roster().peers[0],display_name:"bad\nname"}]}),
    roster(1,{peers:[{...roster().peers[0],mode:"view"}]}),
    roster(1,{peers:[{...roster().peers[0],email:"not-exported@example.invalid"}]}),
  ])("rejects malformed, foreign or inconsistent roster %# and clears displayed membership",value=>{
    const onRoster=vi.fn();const f=fixture({presence:true,onRoster});f.ready();f.sockets[0].receive(roster());
    f.sockets[0].receive(value);expect(f.onStatus).toHaveBeenLastCalledWith("failed");expect(onRoster).toHaveBeenLastCalledWith(null);
    expect(f.onSnapshot).toHaveBeenCalledTimes(1);
  });
  it("does not accept an unsolicited extension on a legacy connection",()=>{
    const onRoster=vi.fn();const f=fixture({onRoster});f.ready();f.sockets[0].receive(roster());
    expect(f.onStatus).toHaveBeenLastCalledWith("failed");expect(onRoster.mock.calls.every(([value])=>value===null)).toBe(true);
  });
  it("clears membership on lost liveness/reconnect and fences late socket delivery",()=>{
    const onRoster=vi.fn();const f=fixture({presence:true,onRoster});f.ready();f.sockets[0].receive(roster());
    const late=f.sockets[0].onmessage!;
    vi.advanceTimersByTime(20000);expect(onRoster).toHaveBeenLastCalledWith(null);
    expect(f.onStatus).toHaveBeenLastCalledWith("reconnecting");late({data:JSON.stringify(roster(99))});
    expect(onRoster).toHaveBeenLastCalledWith(null);vi.advanceTimersByTime(250);expect(f.sockets).toHaveLength(2);
    f.transport.dispose();
  });
  it("fresh rosters renew only the presence deadline and terminal unavailability consumes the roster",()=>{
    const onRoster=vi.fn();const f=fixture({presence:true,onRoster});f.ready();
    for(let i=0;i<6;i++){vi.advanceTimersByTime(5000);f.sockets[0].receive(roster(1));}
    expect(f.sockets).toHaveLength(1);expect(f.onSnapshot).toHaveBeenCalledTimes(1);
    f.sockets[0].receive({type:"unavailable",version:1});expect(onRoster).toHaveBeenLastCalledWith(null);
    vi.advanceTimersByTime(60000);expect(f.sockets).toHaveLength(1);
  });
});
