/** Advisory selected-draft transport only. REST CAS owns every save and acknowledgement. */
export interface DraftPeerKey { space_id: string; document_id: string; draft_id: string }
export type DraftPeerMode = "view" | "edit";
/** Exact wire projection: nullable values stay nullable, with no DraftRecord normalization. */
export interface DraftPeerSnapshot {
  id: string; document_id: string; space_id: string; owner_id: string;
  owner_name: string; document_title: string; title: string; content: string;
  icon: string | null; cover_image: string | null; base_updated_at: string;
  access_role: "owner" | "editor" | "viewer"; can_publish: boolean;
  created_at: string; updated_at: string;
  previous_folder_id: string | null; previous_parent_id: string | null;
  source_digest: string; birth_operation_id?: string;
}
export interface DraftPresencePeer {
  user_id: string; connection_id: string; display_name: string;
  state: "active" | "idle"; mode: DraftPeerMode;
}
export interface DraftPresenceRoster {
  type: "presence_roster"; version: 1; key: DraftPeerKey; room_id: string;
  presence_revision: number; connection_id: string; peers: DraftPresencePeer[];
}
type Scoped = { version: 1; key: DraftPeerKey; room_id: string; sequence: number };
export type DraftPeerFrame =
  | DraftPresenceRoster
  | { type: "authenticated"; version: 1; connection_id: string }
  | ({ type: "snapshot"; connection_id: string; draft: DraftPeerSnapshot } & Scoped)
  | ({ type: "invalidate"; source_digest: string; reason: "committed" | "source_changed" | "resync_required" } & Scoped)
  | ({ type: "presence"; user_id: string; connection_id: string; state: "active" | "idle" } & Scoped)
  | ({ type: "cursor"; user_id: string; connection_id: string; source_digest: string; anchor: number; head: number } & Scoped)
  | { type: "unavailable"; version: 1 };
export type DraftPeerStatus = "connecting" | "authenticating" | "subscribing" | "ready" | "reconnecting" | "unavailable" | "failed" | "disposed";
export interface DraftPeerSocket {
  readyState: number; bufferedAmount: number;
  onopen: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: (() => void) | null; onerror: (() => void) | null;
  send(data: string): void; close(code?: number, reason?: string): void;
}
export interface DraftPeerTransportOptions {
  apiBase: string; pageUrl: string; bearer: string;
  key: DraftPeerKey; mode: DraftPeerMode;
  /** Must cover original account/session/server/storage/route/draft selection generation. */
  isCurrent: () => boolean;
  /** Fresh advisory source only: caller must fence dirty/pending/held work before adoption. */
  onSnapshot: (frame: Extract<DraftPeerFrame, { type: "snapshot" }>) => void;
  /** Explicit negotiation keeps cached strict v1 consumers unchanged. */
  presence?: boolean;
  /** Authoritative replacement, including this connection; null clears stale membership. */
  onRoster?: (frame: DraftPresenceRoster | null) => void;
  onPeer?: (frame: Extract<DraftPeerFrame, { type: "presence" | "cursor" }>) => void;
  onInvalidation?: (frame: Extract<DraftPeerFrame, { type: "invalidate" }>) => void;
  onStatus?: (status: DraftPeerStatus) => void;
  socketFactory?: (url: string) => DraftPeerSocket;
}
const INPUT_LIMIT = 16 * 1024, OUTPUT_LIMIT = 32 * 1024 * 1024;
const encoder = new TextEncoder();
const bytes = (value: string) => encoder.encode(value).length;
const record = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const exact = (v: Record<string, unknown>, required: string[], optional: string[] = []) =>
  required.every(key => Object.prototype.hasOwnProperty.call(v, key)) && Object.keys(v).every(key => required.includes(key) || optional.includes(key));
function hasControl(value: string) {
  for (const character of value) {
    const code = character.codePointAt(0)!;
    if (code <= 31 || code >= 127 && code <= 159) return true;
  }
  return false;
}
const id = (v: unknown): v is string => typeof v === "string" && v.length > 0 && v.length <= 256 && bytes(v) <= 256 && !hasControl(v);
const uuid = (v: unknown): v is string => typeof v === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v);
const digest = (v: unknown): v is string => typeof v === "string" && /^[0-9a-f]{64}$/.test(v);
const counter = (v: unknown): v is number => Number.isSafeInteger(v) && (v as number) >= 0;
const sameKey = (a: DraftPeerKey, b: DraftPeerKey) => a.space_id === b.space_id && a.document_id === b.document_id && a.draft_id === b.draft_id;
function validKey(v: unknown): v is DraftPeerKey {
  return record(v) && exact(v, ["space_id", "document_id", "draft_id"]) && id(v.space_id) && id(v.document_id) && id(v.draft_id);
}
export function draftPeerSocketUrl(apiBase: string, pageUrl: string): string {
  if (!apiBase || apiBase !== apiBase.trim()) throw new Error("Invalid draft peer API base");
  let url: URL;
  try { url = new URL(apiBase); }
  catch {
    const page = new URL(pageUrl);
    if (!/^https?:$/.test(page.protocol) || page.username || page.password) throw new Error("Invalid draft peer page origin");
    url = new URL(apiBase, page);
  }
  if (!/^https?:$/.test(url.protocol) || url.username || url.password || url.search || url.hash || !/\/api\/v1\/?$/.test(url.pathname)) throw new Error("Invalid draft peer API base");
  url.pathname = url.pathname.replace(/\/api\/v1\/?$/, "/ws/drafts/v1");
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.href;
}
function validSnapshot(v: unknown, key: DraftPeerKey): v is DraftPeerSnapshot {
  const strings = ["owner_name", "document_title", "title", "content", "base_updated_at", "created_at", "updated_at"];
  if (!record(v) || !exact(v, ["id", "document_id", "space_id", "owner_id", ...strings, "icon", "cover_image", "access_role", "can_publish", "previous_folder_id", "previous_parent_id", "source_digest"], ["birth_operation_id"])) return false;
  return v.id === key.draft_id && v.document_id === key.document_id && v.space_id === key.space_id && id(v.owner_id)
    && strings.every(field => typeof v[field] === "string")
    && ["icon", "cover_image"].every(field => v[field] === null || typeof v[field] === "string")
    && ["previous_folder_id", "previous_parent_id"].every(field => v[field] === null || id(v[field]))
    && ["owner", "editor", "viewer"].includes(v.access_role as string) && typeof v.can_publish === "boolean" && digest(v.source_digest)
    && (!Object.prototype.hasOwnProperty.call(v, "birth_operation_id") || id(v.birth_operation_id));
}
/** Fail closed on unsupported fields/version/identity. No conversion or source reconstruction. */
export function parseDraftPeerFrame(data: unknown, key: DraftPeerKey): DraftPeerFrame | null {
  if (typeof data !== "string" || data.length > OUTPUT_LIMIT || bytes(data) > OUTPUT_LIMIT) return null;
  let v: unknown;
  try { v = JSON.parse(data); } catch { return null; }
  if (!record(v) || v.version !== 1) return null;
  if (v.type === "unavailable") return exact(v, ["type", "version"]) ? v as DraftPeerFrame : null;
  if (v.type === "authenticated") return exact(v, ["type", "version", "connection_id"]) && uuid(v.connection_id) ? v as DraftPeerFrame : null;
  if (v.type === "presence_roster") {
    if (!exact(v,["type","version","key","room_id","presence_revision","connection_id","peers"])
      || !validKey(v.key) || !sameKey(v.key,key) || !uuid(v.room_id) || !uuid(v.connection_id)
      || !counter(v.presence_revision) || !Array.isArray(v.peers) || v.peers.length < 1 || v.peers.length > 64) return null;
    const connections = new Set<string>();
    for (const peer of v.peers) {
      if (!record(peer) || !exact(peer,["user_id","connection_id","display_name","state","mode"])
        || !id(peer.user_id) || !uuid(peer.connection_id) || connections.has(peer.connection_id)
        || typeof peer.display_name !== "string" || !peer.display_name.trim() || bytes(peer.display_name) > 256 || hasControl(peer.display_name)
        || !["active","idle"].includes(peer.state as string) || !["view","edit"].includes(peer.mode as string)) return null;
      connections.add(peer.connection_id);
    }
    return connections.has(v.connection_id) ? v as unknown as DraftPresenceRoster : null;
  }
  if (!validKey(v.key) || !sameKey(v.key, key) || !uuid(v.room_id) || !counter(v.sequence)) return null;
  const base = ["type", "version", "key", "room_id", "sequence"];
  if (v.type === "snapshot") return exact(v, [...base, "connection_id", "draft"]) && uuid(v.connection_id) && validSnapshot(v.draft, key) ? v as DraftPeerFrame : null;
  if (v.type === "invalidate") return exact(v, [...base, "source_digest", "reason"]) && digest(v.source_digest) && ["committed", "source_changed", "resync_required"].includes(v.reason as string) ? v as DraftPeerFrame : null;
  if (!uuid(v.connection_id) || !id(v.user_id)) return null;
  if (v.type === "presence") return exact(v, [...base, "user_id", "connection_id", "state"]) && ["active", "idle"].includes(v.state as string) ? v as DraftPeerFrame : null;
  if (v.type === "cursor") return exact(v, [...base, "user_id", "connection_id", "source_digest", "anchor", "head"]) && digest(v.source_digest) && counter(v.anchor) && v.anchor <= 0xffffffff && counter(v.head) && v.head <= 0xffffffff ? v as DraftPeerFrame : null;
  return null;
}

function browserSocket(url: string): DraftPeerSocket {
  const native = new WebSocket(url);
  const socket: DraftPeerSocket = {
    get readyState() { return native.readyState; },
    get bufferedAmount() { return native.bufferedAmount; },
    onopen: null, onmessage: null, onclose: null, onerror: null,
    send: text => native.send(text), close: (code, reason) => native.close(code, reason),
  };
  native.onopen = () => socket.onopen?.();
  native.onmessage = event => socket.onmessage?.({ data: event.data });
  native.onclose = () => socket.onclose?.();
  native.onerror = () => socket.onerror?.();
  return socket;
}

export function createDraftPeerTransport(options: DraftPeerTransportOptions) {
  const { bearer, mode, isCurrent, onSnapshot, onPeer, onInvalidation, onStatus } = options;
  const key = Object.freeze({ ...options.key });
  if (!validKey(key) || !["view", "edit"].includes(mode) || !bearer || bytes(bearer) > INPUT_LIMIT || hasControl(bearer)) throw new Error("Invalid draft peer scope");
  const url = draftPeerSocketUrl(options.apiBase, options.pageUrl);
  const factory = options.socketFactory ?? browserSocket;
  let socket: DraftPeerSocket | null = null, disposed = false, started = false, terminal = false;
  let epoch = 0, attempts = 0, connection: string | null = null, room: string | null = null, sequence = -1;
  let source: { digest: string; content: string } | null = null, pendingResync = false;
  let retryTimer: ReturnType<typeof setTimeout> | undefined, deadline: ReturnType<typeof setTimeout> | undefined;
  let presenceRevision = -1;
  let presenceDeadline: ReturnType<typeof setTimeout> | undefined;
  function clearRoster() {
    clearTimeout(presenceDeadline); presenceDeadline = undefined; presenceRevision = -1;
    try { options.onRoster?.(null); } catch { /* Cleanup never restarts a disposed scope. */ }
  }
  function armPresenceDeadline() {
    clearTimeout(presenceDeadline);
    presenceDeadline = setTimeout(reconnect,20000);
  }
  let rateWindow = 0, sentInWindow = 0;
  function clearTimers() { clearTimeout(retryTimer); clearTimeout(deadline); retryTimer = deadline = undefined; }
  function detach() {
    clearRoster();
    epoch++;
    const old = socket; socket = null;
    if (old) { old.onopen = old.onmessage = old.onclose = old.onerror = null; try { old.close(1000, "Draft scope closed"); } catch { /* Already closed. */ } }
  }
  function dispose() { if (disposed) return; disposed = true; clearTimers(); detach(); }
  function current() {
    if (disposed || terminal) return false;
    try { if (isCurrent()) return true; } catch { /* Context inspection failed closed. */ }
    dispose(); return false;
  }
  function dispatch(callback: (() => void) | undefined) {
    if (!current()) return false;
    try { callback?.(); } catch { stop("failed"); return false; }
    return current();
  }
  function stop(status: "failed" | "unavailable") {
    if (!current()) return;
    terminal = true; clearTimers(); detach();
    // Current scope was checked immediately above; no socket event can acknowledge a save.
    try { onStatus?.(status); } catch { /* Terminal callback cannot restart transport. */ }
  }
  function status(value: DraftPeerStatus) { return dispatch(() => onStatus?.(value)); }
  function send(text: string) {
    if (!current() || !socket || socket.readyState !== 1 || bytes(text) > INPUT_LIMIT || socket.bufferedAmount + bytes(text) > INPUT_LIMIT) return false;
    const now = Date.now();
    if (now - rateWindow >= 1000) { rateWindow = now; sentInWindow = 0; }
    if (sentInWindow >= 30) return false;
    sentInWindow++;
    try { socket.send(text); return current(); } catch { reconnect(); return false; }
  }
  function armDeadline() { clearTimeout(deadline); deadline = setTimeout(reconnect, 15000); }
  function resync() {
    if (!current() || !room || pendingResync) return false;
    pendingResync = true;
    if (!send(JSON.stringify({ type: "resync", key }))) { pendingResync = false; reconnect(); return false; }
    armDeadline(); return true;
  }
  function reconnect() {
    if (!current() || retryTimer !== undefined) return;
    clearTimers(); detach(); connection = room = null; source = null; sequence = -1; pendingResync = false;
    if (attempts >= 5) { stop("failed"); return; }
    const delay = Math.min(250 * 2 ** attempts++, 8000);
    if (!status("reconnecting")) return;
    retryTimer = setTimeout(connect, delay);
  }
  function connect() {
    if (!current()) return;
    clearTimers(); connection = room = null; source = null; sequence = -1; pendingResync = false;
    const generation = ++epoch;
    if (!status("connecting")) return;
    let selected: DraftPeerSocket;
    try { selected = factory(url); } catch { reconnect(); return; }
    if (!current()) { try { selected.close(1000, "Draft scope closed"); } catch { /* Closed. */ } return; }
    socket = selected; armDeadline();
    const live = () => generation === epoch && selected === socket && current();
    selected.onopen = () => { if (live() && status("authenticating")) { if (!send(bearer)) reconnect(); } };
    selected.onerror = selected.onclose = () => { if (live()) reconnect(); };
    selected.onmessage = event => {
      if (!live()) return;
      const frame = parseDraftPeerFrame(event.data, key);
      if (!frame) { stop("failed"); return; }
      if (frame.type === "unavailable") { stop("unavailable"); return; }
      if (frame.type === "authenticated") {
        if (connection || room) { stop("failed"); return; }
        connection = frame.connection_id;
        if (status("subscribing") && !send(JSON.stringify({ type: "subscribe", key, mode, ...(options.presence ? {presence_version:1} : {}) }))) reconnect();
        return;
      }
      if (!connection) { stop("failed"); return; }
      if (frame.type === "snapshot") {
        if (frame.connection_id !== connection || room !== null && frame.room_id !== room) { stop("failed"); return; }
        if (frame.sequence < sequence || frame.sequence === sequence && room !== null && !pendingResync) return;
        room = frame.room_id; sequence = frame.sequence; pendingResync = false;
        attempts = 0; // Bound consecutive failures; healthy sessions may reconnect over time.
        source = { digest: frame.draft.source_digest, content: frame.draft.content };
        clearTimeout(deadline); deadline = undefined;
        if (dispatch(() => onSnapshot(frame))) { if (options.presence && presenceDeadline === undefined) armPresenceDeadline(); status("ready"); }
        return;
      }
      if (!room || frame.room_id !== room) { stop("failed"); return; }
      if (frame.type === "presence_roster") {
        if (!options.presence || frame.connection_id !== connection || frame.peers.find(peer => peer.connection_id === connection)?.mode !== mode) { stop("failed"); return; }
        if (frame.presence_revision < presenceRevision) return;
        presenceRevision = frame.presence_revision;
        armPresenceDeadline();
        dispatch(() => options.onRoster?.(frame));
        return;
      }
      if (frame.sequence <= sequence) return;
      // The server may suppress peer events: a gap requests inspection, never a data write.
      const gap = frame.sequence > sequence + 1;
      sequence = frame.sequence;
      if (frame.type === "invalidate") {
        if (dispatch(() => onInvalidation?.(frame))) resync();
        return;
      }
      if (gap || pendingResync) { resync(); return; }
      if (frame.type === "cursor") {
        if (!source || frame.source_digest !== source.digest) { resync(); return; }
        const boundary = (offset: number) => offset <= source!.content.length && !(offset > 0 && offset < source!.content.length && /[\ud800-\udbff]/u.test(source!.content[offset - 1]) && /[\udc00-\udfff]/u.test(source!.content[offset]));
        if (!boundary(frame.anchor) || !boundary(frame.head)) { stop("failed"); return; }
      }
      dispatch(() => onPeer?.(frame));
    };
  }
  return {
    start() { if (!started && current()) { started = true; connect(); } },
    dispose,
    resync,
    presence(state: "active" | "idle") { return current() && !!room && !pendingResync && (state === "active" || state === "idle") && send(JSON.stringify({ type: "presence", key, state })); },
    cursor(sourceDigest: string, anchor: number, head: number) {
      if (mode !== "edit" || !current() || !room || pendingResync || !source || source.digest !== sourceDigest) return false;
      const boundary = (offset: number) => Number.isSafeInteger(offset) && offset >= 0 && offset <= 0xffffffff
        && offset <= source!.content.length
        && !(offset > 0 && offset < source!.content.length && /[\ud800-\udbff]/u.test(source!.content[offset - 1]) && /[\udc00-\udfff]/u.test(source!.content[offset]));
      return boundary(anchor) && boundary(head)
        && send(JSON.stringify({ type: "cursor", key, source_digest: sourceDigest, anchor, head }));
    },
  };
}
