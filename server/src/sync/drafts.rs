//! Draft-scoped advisory transport. REST source/CAS remains the only save owner.
use super::{validate_ws_session, ws_identity};
use crate::{AppState, auth::{jwt, middleware::AuthUser}, spaces::{draft_save, drafts::DraftResponse}};
use axum::{extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}}, response::IntoResponse};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}}, time::{Duration, Instant}};
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;
#[path = "drafts_presence.rs"]
mod presence;

const VERSION: u8 = 1;
const MAX_INPUT: usize = 16 * 1024;
const MAX_OUTPUT: usize = 32 * 1024 * 1024;
const ROOM_CAPACITY: usize = 128;
const MAX_ROOMS: usize = 1024;
const MAX_CONNECTIONS_PER_ROOM: usize = 64;
const BOUNDARY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftKey {
    pub space_id: String,
    pub document_id: String,
    pub draft_id: String,
}
impl DraftKey {
    fn valid(&self) -> bool {
        [&self.space_id, &self.document_id, &self.draft_id].into_iter()
            .all(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Mode { View, Edit }
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PresenceState { Active, Idle }
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ClientFrame {
    Subscribe { key: DraftKey, mode: Mode, #[serde(default, deserialize_with = "presence_version")] presence_version: Option<u8> },
    Resync { key: DraftKey },
    Presence { key: DraftKey, state: PresenceState },
    Cursor { key: DraftKey, source_digest: String, anchor: u32, head: u32 },
}
fn presence_version<'de, D: serde::Deserializer<'de>>(input: D) -> Result<Option<u8>, D::Error> {
    u8::deserialize(input).map(Some)
}
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerFrame<'a> {
    Authenticated { version: u8, connection_id: &'a str },
    Snapshot { version: u8, key: &'a DraftKey, connection_id: &'a str, room_id: &'a str, sequence: u64, draft: &'a DraftResponse },
    Invalidate { version: u8, key: &'a DraftKey, room_id: &'a str, sequence: u64, source_digest: &'a str, reason: &'a str },
    Presence { version: u8, key: &'a DraftKey, room_id: &'a str, sequence: u64, user_id: &'a str, connection_id: &'a str, state: PresenceState },
    Cursor { version: u8, key: &'a DraftKey, room_id: &'a str, sequence: u64, user_id: &'a str, connection_id: &'a str, source_digest: &'a str, anchor: u32, head: u32 },
    PresenceRoster { version: u8, key: &'a DraftKey, room_id: &'a str, presence_revision: u64, connection_id: &'a str, peers: &'a [presence::RosterPeer] },
    Unavailable { version: u8 },
}
#[derive(Clone)]
enum EventKind { Committed, SourceChanged, Presence(PresenceState), Cursor { source_digest: String, anchor: u32, head: u32 } }
#[derive(Clone)]
struct Peer { auth: Arc<AuthUser>, mode: Mode, connection_id: String, live: Arc<AtomicBool> }
#[derive(Clone)]
struct Event { sequence: u64, draft_sequence: u64, peer: Option<Peer>, kind: EventKind }
enum RoomIncoming { Source(Result<Event, broadcast::error::RecvError>), Roster(Result<u64, broadcast::error::RecvError>) }
struct Order { sequence: u64, draft_sequence: u64, sender: broadcast::Sender<Event>, draft_sender: broadcast::Sender<Event> }
struct Room { id: String, order: Mutex<Order>, presence: Arc<presence::PresenceRoom> }
impl Room {
    fn new() -> Self { Self { id: Uuid::new_v4().to_string(), order: Mutex::new(Order { sequence: 0, draft_sequence: 0, sender: broadcast::channel(ROOM_CAPACITY).0, draft_sender: broadcast::channel(ROOM_CAPACITY).0 }), presence: Arc::new(presence::PresenceRoom::new()) } }
    fn subscribe(&self) -> Result<broadcast::Receiver<Event>, ()> { self.subscribe_mode(false) }
    fn subscribe_mode(&self, negotiated: bool) -> Result<broadcast::Receiver<Event>, ()> {
        let order = self.order.lock().map_err(|_| ())?;
        if order.sender.receiver_count() + order.draft_sender.receiver_count() >= MAX_CONNECTIONS_PER_ROOM { return Err(()); }
        Ok(if negotiated { order.draft_sender.subscribe() } else { order.sender.subscribe() })
    }
    fn relisten(&self) -> Result<broadcast::Receiver<Event>, ()> { self.relisten_mode(false) }
    fn relisten_mode(&self, negotiated: bool) -> Result<broadcast::Receiver<Event>, ()> {
        let order = self.order.lock().map_err(|_| ())?;
        Ok(if negotiated { order.draft_sender.subscribe() } else { order.sender.subscribe() })
    }
    fn sequence(&self) -> Result<u64, ()> { Ok(self.order.lock().map_err(|_| ())?.sequence) }
    fn draft_sequence(&self) -> Result<u64, ()> { Ok(self.order.lock().map_err(|_| ())?.draft_sequence) }
    fn empty(&self) -> bool { self.order.lock().map(|o| o.sender.receiver_count() == 0 && o.draft_sender.receiver_count() == 0).unwrap_or(false) }
    fn emit(&self, kind: EventKind, peer: Option<Peer>) -> Result<(), ()> {
        let mut order = self.order.lock().map_err(|_| ())?;
        order.sequence = order.sequence.checked_add(1).ok_or(())?;
        let source_event = !matches!(kind, EventKind::Presence(_));
        if source_event { order.draft_sequence = order.draft_sequence.checked_add(1).ok_or(())?; }
        let event = Event { sequence: order.sequence, draft_sequence: order.draft_sequence, peer, kind };
        // Legacy subscribers retain every original event and sequence. Negotiated
        // subscribers have an independent bounded source queue: presence-only
        // pressure can never manufacture a source lag/resync condition.
        let _ = order.sender.send(event.clone());
        if source_event { let _ = order.draft_sender.send(event); }
        Ok(())
    }
}
#[derive(Clone, Default)]
pub(crate) struct Rooms {
    rooms: Arc<RwLock<HashMap<DraftKey, Arc<Room>>>>,
    #[cfg(test)]
    receive_pause: Arc<RwLock<Option<Arc<ReceivePause>>>>,
    #[cfg(test)]
    pair_pause: Arc<RwLock<Option<Arc<ReceivePause>>>>,
    #[cfg(test)]
    roster_pause: Arc<RwLock<Option<Arc<ReceivePause>>>>,
    #[cfg(test)]
    presence_timing: Arc<RwLock<Option<(Duration, Duration)>>>,
}
#[cfg(test)]
struct ReceivePause { connection_id: String, reached: tokio::sync::Notify, release: tokio::sync::Notify }
#[cfg(test)]
async fn pause(slot: &RwLock<Option<Arc<ReceivePause>>>, connection_id: &str) {
    let selected = {
        let mut slot = slot.write().await;
        if slot.as_ref().is_some_and(|p| p.connection_id == connection_id) { slot.take() } else { None }
    };
    if let Some(pause) = selected { pause.reached.notify_one(); pause.release.notified().await; }
}
impl Rooms {
    async fn join(&self, key: &DraftKey, negotiated: bool) -> Result<(Arc<Room>, broadcast::Receiver<Event>), ()> {
        let mut rooms = self.rooms.write().await;
        rooms.retain(|_, room| !room.empty());
        if !rooms.contains_key(key) && rooms.len() >= MAX_ROOMS { return Err(()); }
        let room = rooms.entry(key.clone()).or_insert_with(|| Arc::new(Room::new())).clone();
        let receiver = room.subscribe_mode(negotiated)?;
        Ok((room, receiver))
    }
    async fn release(&self, key: &DraftKey, room: &Arc<Room>) {
        let mut rooms = self.rooms.write().await;
        if rooms.get(key).is_some_and(|current| Arc::ptr_eq(current, room) && current.empty()) { rooms.remove(key); }
    }
}
/// Advisory only: invoked after the existing SQL commit, never for a historical ACK.
pub(crate) async fn notify_commit(state: &AppState, key: &DraftKey) {
    let rooms = state.ws_rooms.drafts.rooms.read().await;
    if let Some(room) = rooms.get(key) { let _ = room.emit(EventKind::Committed, None); }
}
struct Inspection { draft: DraftResponse, projection_digest: String }
async fn identity(state: &AppState, auth: &AuthUser) -> Result<(), ()> {
    tokio::time::timeout(BOUNDARY_TIMEOUT, ws_identity(state, auth)).await.map_err(|_| ())?
}
async fn inspect(state: &AppState, auth: &AuthUser, key: &DraftKey, mode: Mode) -> Result<Inspection, ()> {
    tokio::time::timeout(BOUNDARY_TIMEOUT, async {
        ws_identity(state, auth).await?;
        let saved = draft_save::inspect(state, auth, &key.space_id, &key.document_id, &key.draft_id, mode == Mode::Edit, None).await.map_err(|_| ())?;
        let draft = crate::spaces::drafts::response_from_saved(saved, &auth.claims.sub).map_err(|_| ())?;
        // Includes display name/title and role changes not present in raw-row source_digest.
        let bytes = serde_json::to_vec(&draft).map_err(|_| ())?;
        if bytes.len() > MAX_OUTPUT - 4096 { return Err(()); }
        let projection_digest = hex::encode(Sha256::digest(&bytes));
        ws_identity(state, auth).await?;
        Ok(Inspection { draft, projection_digest })
    }).await.map_err(|_| ())?
}
/// Final phase-local authority check after potentially waiting projection reads.
/// No claim is made to serialize the later network send with every policy writer.
async fn pair_fence(state: &AppState, recipient: &AuthUser, mode: Mode, peer: &Peer, key: &DraftKey, cursor_source: Option<&str>) -> Result<(), ()> {
    use crate::spaces::access::{self, DraftOperation, Purpose};
    tokio::time::timeout(BOUNDARY_TIMEOUT, async {
        let mut conn = state.db.acquire().await.map_err(|_| ())?;
        for (auth, mode) in [(peer.auth.as_ref(), peer.mode), (recipient, mode)] {
            if mode == Mode::Edit { access::require_current_human_preference_write(&mut conn, auth).await.map_err(|_| ())?; }
            else { access::require_current_human_read(&mut conn, auth).await.map_err(|_| ())?; }
            let principal = access::principal_from_auth(auth).map_err(|_| ())?;
            access::resolve_draft_access(&mut conn, principal, &key.space_id, &key.document_id, &key.draft_id, if mode == Mode::Edit { DraftOperation::Edit } else { DraftOperation::View }, Purpose::Routine).await.map_err(|_| ())?;
        }
        if let Some(source) = cursor_source {
            let row = draft_save::selected(&mut conn, &key.space_id, &key.document_id, &key.draft_id).await.map_err(|_| ())?;
            if draft_save::digest(&row).map_err(|_| ())? != source { return Err(()); }
        }
        for auth in [peer.auth.as_ref(), recipient] {
            super::ws_current_member(&mut conn, auth).await?;
            access::require_current_human_read(&mut conn, auth).await.map_err(|_| ())?;
        }
        Ok(())
    }).await.map_err(|_| ())?
}
async fn send(socket: &mut WebSocket, frame: &ServerFrame<'_>) -> Result<(), ()> {
    let bytes = serde_json::to_string(frame).map_err(|_| ())?;
    if bytes.len() > MAX_OUTPUT { return Err(()); }
    tokio::time::timeout(BOUNDARY_TIMEOUT, socket.send(Message::Text(bytes.into()))).await.map_err(|_| ())?.map_err(|_| ())
}
struct LiveLease(Arc<AtomicBool>, Arc<presence::PresenceRoom>, String);
impl Drop for LiveLease { fn drop(&mut self) { self.0.store(false, Ordering::Release); self.1.remove(&self.2); } }
struct Subscription { key: DraftKey, mode: Mode, room: Arc<Room>, receiver: broadcast::Receiver<Event>, projection_digest: String, live: LiveLease, presence_version: bool, presence_receiver: broadcast::Receiver<u64>, presence_sent: AtomicU64 }
impl Drop for Subscription {
    fn drop(&mut self) {
        // Invalidate queued identity before dropping receivers or any other owner.
        self.live.0.store(false, Ordering::Release);
        self.room.presence.remove(&self.live.2);
    }
}
async fn detach(state: &AppState, subscription: &mut Option<Subscription>) {
    if let Some(previous) = subscription.take() {
        let key = previous.key.clone(); let room = previous.room.clone();
        drop(previous);
        state.ws_rooms.drafts.release(&key, &room).await;
    }
}
async fn snapshot(socket: &mut WebSocket, state: &AppState, auth: &AuthUser, connection_id: &str, sub: &mut Subscription) -> Result<(), ()> {
    // Receiver already exists. This sequence precedes inspection: subsequent events
    // remain queued and cannot be mistaken for body operations covered by this read.
    let sequence = if sub.presence_version { sub.room.draft_sequence()? } else { sub.room.sequence()? };
    let current = inspect(state, auth, &sub.key, sub.mode).await?;
    if !sub.live.0.load(Ordering::Acquire) { return Err(()); }
    send(socket, &ServerFrame::Snapshot { version: VERSION, key: &sub.key, connection_id, room_id: &sub.room.id, sequence, draft: &current.draft }).await?;
    sub.projection_digest = current.projection_digest;
    Ok(())
}
async fn invalidate(socket: &mut WebSocket, sub: &mut Subscription, current: Inspection, sequence: u64, reason: &str) -> Result<(), ()> {
    send(socket, &ServerFrame::Invalidate { version: VERSION, key: &sub.key, room_id: &sub.room.id, sequence, source_digest: &current.draft.source_digest, reason }).await?;
    sub.projection_digest = current.projection_digest;
    Ok(())
}
fn cursor_boundaries(content: &str, anchor: u32, head: u32) -> bool {
    let mut offset = 0u32;
    let mut a = anchor == 0; let mut h = head == 0;
    for ch in content.chars() {
        offset += ch.len_utf16() as u32;
        a |= offset == anchor; h |= offset == head;
        if a && h { return true; }
    }
    a && h
}
pub(crate) async fn handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.max_message_size(MAX_INPUT).max_frame_size(MAX_INPUT).on_upgrade(move |socket| handle(socket, state))
}
async fn handle(mut socket: WebSocket, state: AppState) {
    let token = match tokio::time::timeout(Duration::from_secs(10), socket.recv()).await {
        Ok(Some(Ok(Message::Text(token)))) if token.len() <= MAX_INPUT => token,
        _ => return,
    };
    let auth = match tokio::time::timeout(BOUNDARY_TIMEOUT, async {
        let claims = jwt::validate_token(&token, &state.jwt_secret, jwt::TokenType::Access).map_err(|_| ())?;
        let auth = validate_ws_session(&state, &claims).await?;
        ws_identity(&state, &auth).await?;
        Ok::<_, ()>(auth)
    }).await {
        Ok(Ok(auth)) => auth,
        _ => return,
    };
    let auth = Arc::new(auth);
    let connection_id = Uuid::new_v4().to_string();
    if send(&mut socket, &ServerFrame::Authenticated { version: VERSION, connection_id: &connection_id }).await.is_err() { return; }
    let mut subscription: Option<Subscription> = None;
    let (ping_interval, presence_ttl) = (presence::PING_INTERVAL, presence::PRESENCE_TTL);
    #[cfg(test)]
    let (ping_interval, presence_ttl) = state.ws_rooms.drafts.presence_timing.read().await.unwrap_or((ping_interval, presence_ttl));
    let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + ping_interval, ping_interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut frame_window = Instant::now(); let mut frames = 0usize;
    let result: Result<(), ()> = async {
        loop {
            if subscription.as_ref().is_some_and(|sub| !sub.live.0.load(Ordering::Acquire)) { return Err(()); }
            #[cfg(test)]
            if subscription.is_some() { pause(&state.ws_rooms.drafts.receive_pause, &connection_id).await; }
            tokio::select! {
                _ = tick.tick() => {
                    if let Some(sub) = subscription.as_mut() {
                        let current = inspect(&state, &auth, &sub.key, sub.mode).await?;
                        if !sub.live.0.load(Ordering::Acquire) { return Err(()); }
                        // Expiration is irreversible even if a late Pong arrives.
                        sub.room.presence.expire(presence_ttl)?;
                        if !sub.live.0.load(Ordering::Acquire) { return Err(()); }
                        tokio::time::timeout(BOUNDARY_TIMEOUT, socket.send(Message::Ping(Vec::new().into()))).await.map_err(|_| ())?.map_err(|_| ())?;
                        presence::roster(&mut socket, &state, &auth, &connection_id, sub, presence_ttl).await?;
                        if current.projection_digest != sub.projection_digest {
                            sub.room.emit(EventKind::SourceChanged, None)?;
                        }
                    } else { identity(&state, &auth).await?; }
                }
                incoming = socket.recv() => {
                    if frame_window.elapsed() >= Duration::from_secs(1) { frame_window = Instant::now(); frames = 0; }
                    frames += 1;
                    if frames > 60 { return Err(()); }
                    let text = match incoming {
                        Some(Ok(Message::Text(text))) if text.len() <= MAX_INPUT => text,
                        Some(Ok(Message::Close(_))) | None => return Ok(()),
                        Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {
                            if let Some(sub) = subscription.as_ref() { presence::connection_authority(&state, &auth, &sub.key, sub.mode).await?; sub.room.presence.touch(&connection_id, presence_ttl)?; }
                            else { identity(&state, &auth).await?; }
                            continue;
                        }
                        _ => return Err(()),
                    };
                    let frame: ClientFrame = serde_json::from_str(&text).map_err(|_| ())?;
                    match frame {
                        ClientFrame::Subscribe { key, mode, presence_version } => {
                            // A failed target change cannot keep forwarding the previous room.
                            detach(&state, &mut subscription).await;
                            if !key.valid() || presence_version.is_some_and(|version| version != 1) { return Err(()); }
                            let (room, receiver) = state.ws_rooms.drafts.join(&key, presence_version == Some(1)).await?;
                            let live = Arc::new(AtomicBool::new(true));
                            let presence_receiver = room.presence.subscribe();
                            let lease = LiveLease(live.clone(), room.presence.clone(), connection_id.clone());
                            subscription = Some(Subscription { key, mode, room, receiver, projection_digest: String::new(), live: lease, presence_version: presence_version == Some(1), presence_receiver, presence_sent: AtomicU64::new(0) });
                            let sub = subscription.as_mut().ok_or(())?;
                            snapshot(&mut socket, &state, &auth, &connection_id, sub).await?;
                            sub.room.presence.join(Peer { auth: auth.clone(), mode: sub.mode, connection_id: connection_id.clone(), live })?;
                            presence::roster(&mut socket, &state, &auth, &connection_id, sub, presence_ttl).await?;
                        }
                        ClientFrame::Resync { key } => {
                            let sub = subscription.as_mut().ok_or(())?;
                            if !key.valid() || key != sub.key { return Err(()); }
                            sub.receiver = sub.room.relisten_mode(sub.presence_version)?;
                            sub.room.presence.touch(&connection_id, presence_ttl)?;
                            snapshot(&mut socket, &state, &auth, &connection_id, sub).await?;
                            presence::roster(&mut socket, &state, &auth, &connection_id, sub, presence_ttl).await?;
                        }
                        ClientFrame::Presence { key, state: presence } => {
                            let sub = subscription.as_mut().ok_or(())?;
                            if !key.valid() || key != sub.key { return Err(()); }
                            inspect(&state, &auth, &key, sub.mode).await?;
                            sub.room.presence.activity(&connection_id, presence, presence_ttl)?;
                            sub.room.emit(EventKind::Presence(presence), Some(Peer { auth: auth.clone(), mode: sub.mode, connection_id: connection_id.clone(), live: sub.live.0.clone() }))?;
                        }
                        ClientFrame::Cursor { key, source_digest, anchor, head } => {
                            let sub = subscription.as_mut().ok_or(())?;
                            if !key.valid() || key != sub.key || sub.mode != Mode::Edit { return Err(()); }
                            let current = inspect(&state, &auth, &key, Mode::Edit).await?;
                            if source_digest != current.draft.source_digest || !cursor_boundaries(&current.draft.content, anchor, head) { return Err(()); }
                            sub.room.presence.touch(&connection_id, presence_ttl)?;
                            sub.room.emit(EventKind::Cursor { source_digest, anchor, head }, Some(Peer { auth: auth.clone(), mode: Mode::Edit, connection_id: connection_id.clone(), live: sub.live.0.clone() }))?;
                        }
                    }
                }
                event = async {
                    if let Some(sub) = subscription.as_mut() {
                        if sub.presence_version {
                            tokio::select! {
                                source = sub.receiver.recv() => RoomIncoming::Source(source),
                                roster = sub.presence_receiver.recv() => RoomIncoming::Roster(roster),
                            }
                        } else { RoomIncoming::Source(sub.receiver.recv().await) }
                    } else { std::future::pending().await }
                } => {
                    let sub = subscription.as_mut().ok_or(())?;
                    let event = match event {
                        RoomIncoming::Source(event) => event,
                        RoomIncoming::Roster(event) => {
                            match event {
                                Ok(revision) if revision <= sub.presence_sent.load(Ordering::Acquire) => continue,
                                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => (),
                                Err(broadcast::error::RecvError::Closed) => return Err(()),
                            }
                            presence::roster(&mut socket, &state, &auth, &connection_id, sub, presence_ttl).await?;
                            continue;
                        }
                    };
                    match event {
                        Ok(event) => {
                            if sub.presence_version && matches!(event.kind, EventKind::Presence(_)) { continue; }
                            let sequence = if sub.presence_version { event.draft_sequence } else { event.sequence };
                            let current = inspect(&state, &auth, &sub.key, sub.mode).await?;
                            if !sub.live.0.load(Ordering::Acquire) { return Err(()); }
                            if let Some(peer) = event.peer {
                                if peer.connection_id == connection_id || !peer.live.load(Ordering::Acquire) { continue; }
                                // A queued frame never borrows a historical sender's authority.
                                let sender = match inspect(&state, &peer.auth, &sub.key, peer.mode).await { Ok(sender) => sender, Err(()) => continue };
                                // Recheck recipient after the sender inspection, which can wait.
                                let recipient = inspect(&state, &auth, &sub.key, sub.mode).await?;
                                #[cfg(test)] pause(&state.ws_rooms.drafts.pair_pause, &connection_id).await;
                                let cursor_source = match &event.kind { EventKind::Cursor { source_digest, .. } => Some(source_digest.as_str()), _ => None };
                                if pair_fence(&state, &auth, sub.mode, &peer, &sub.key, cursor_source).await.is_err() || !sub.live.0.load(Ordering::Acquire) || !peer.live.load(Ordering::Acquire) { continue; }
                                match event.kind {
                                    EventKind::Presence(presence) => send(&mut socket, &ServerFrame::Presence { version: VERSION, key: &sub.key, room_id: &sub.room.id, sequence, user_id: &peer.auth.claims.sub, connection_id: &peer.connection_id, state: presence }).await?,
                                    EventKind::Cursor { source_digest, anchor, head } => {
                                        if sender.draft.source_digest != source_digest || recipient.draft.source_digest != source_digest || !cursor_boundaries(&recipient.draft.content, anchor, head) { continue; }
                                        send(&mut socket, &ServerFrame::Cursor { version: VERSION, key: &sub.key, room_id: &sub.room.id, sequence, user_id: &peer.auth.claims.sub, connection_id: &peer.connection_id, source_digest: &source_digest, anchor, head }).await?;
                                    }
                                    EventKind::Committed | EventKind::SourceChanged => return Err(()),
                                }
                            } else {
                                match event.kind {
                                    EventKind::Committed => invalidate(&mut socket, sub, current, sequence, "committed").await?,
                                    EventKind::SourceChanged => invalidate(&mut socket, sub, current, sequence, "source_changed").await?,
                                    _ => return Err(()),
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            // Explicit gap plus a newly inspected snapshot, never a durable ACK.
                            sub.receiver = sub.room.relisten_mode(sub.presence_version)?;
                            let sequence = if sub.presence_version { sub.room.draft_sequence()? } else { sub.room.sequence()? };
                            let current = inspect(&state, &auth, &sub.key, sub.mode).await?;
                            invalidate(&mut socket, sub, current, sequence, "resync_required").await?;
                            snapshot(&mut socket, &state, &auth, &connection_id, sub).await?;
                            presence::roster(&mut socket, &state, &auth, &connection_id, sub, presence_ttl).await?;
                        }
                        Err(broadcast::error::RecvError::Closed) => return Err(()),
                    }
                }
            }
        }
    }.await;
    detach(&state, &mut subscription).await;
    if result.is_err() { let _ = send(&mut socket, &ServerFrame::Unavailable { version: VERSION }).await; }
    let _ = tokio::time::timeout(BOUNDARY_TIMEOUT, socket.send(Message::Close(None))).await;
}

#[cfg(test)]
#[path = "drafts_tests.rs"]
mod tests;
