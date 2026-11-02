//! Selected-room membership only. This grants no content, save, or cursor authority.
use super::*;
use sqlx::Row;

pub(super) const PING_INTERVAL: Duration = Duration::from_secs(5);
pub(super) const PRESENCE_TTL: Duration = Duration::from_secs(20);
#[derive(Clone, Serialize)]
pub(super) struct RosterPeer {
    pub user_id: String,
    pub connection_id: String,
    pub display_name: String,
    pub state: PresenceState,
    pub mode: Mode,
}
#[derive(Clone)]
struct Member { peer: Peer, state: PresenceState, seen: tokio::time::Instant, label: Option<String> }
struct Membership { revision: u64, members: HashMap<String, Member>, poisoned: bool }
pub(super) struct PresenceRoom { membership: Mutex<Membership>, changed: broadcast::Sender<u64> }
impl PresenceRoom {
    pub(super) fn new() -> Self {
        Self { membership: Mutex::new(Membership { revision: 0, members: HashMap::new(), poisoned: false }), changed: broadcast::channel(ROOM_CAPACITY).0 }
    }
    fn advance(&self, membership: &mut Membership) -> Result<(), ()> {
        membership.revision = match membership.revision.checked_add(1) {
            Some(revision) => revision,
            None => { membership.poisoned = true; for member in membership.members.values() { member.peer.live.store(false, Ordering::Release); } return Err(()); }
        };
        let _ = self.changed.send(membership.revision);
        Ok(())
    }
    pub(super) fn subscribe(&self) -> broadcast::Receiver<u64> { self.changed.subscribe() }
    pub(super) fn join(&self, peer: Peer) -> Result<(), ()> {
        let mut membership = self.membership.lock().map_err(|_| ())?;
        if membership.poisoned || membership.members.len() >= MAX_CONNECTIONS_PER_ROOM || membership.members.contains_key(&peer.connection_id) { return Err(()); }
        membership.members.insert(peer.connection_id.clone(), Member { peer, state: PresenceState::Active, seen: tokio::time::Instant::now(), label: None });
        self.advance(&mut membership)
    }
    pub(super) fn remove(&self, connection_id: &str) {
        if let Ok(mut membership) = self.membership.lock() {
            if let Some(member) = membership.members.remove(connection_id) {
                member.peer.live.store(false, Ordering::Release);
                let _ = self.advance(&mut membership);
            }
        }
    }
    pub(super) fn touch(&self, connection_id: &str, ttl: Duration) -> Result<(), ()> {
        let mut membership = self.membership.lock().map_err(|_| ())?;
        if membership.poisoned { return Err(()); }
        let member = membership.members.get_mut(connection_id).ok_or(())?;
        if !member.peer.live.load(Ordering::Acquire) { return Err(()); }
        let now = tokio::time::Instant::now();
        if now.duration_since(member.seen) >= ttl {
            member.peer.live.store(false, Ordering::Release);
            membership.members.remove(connection_id);
            self.advance(&mut membership)?;
            return Err(());
        }
        member.seen = now;
        Ok(())
    }
    pub(super) fn activity(&self, connection_id: &str, state: PresenceState, ttl: Duration) -> Result<(), ()> {
        self.touch(connection_id, ttl)?;
        let mut membership = self.membership.lock().map_err(|_| ())?;
        if membership.poisoned { return Err(()); }
        let member = membership.members.get_mut(connection_id).ok_or(())?;
        if !member.peer.live.load(Ordering::Acquire) { return Err(()); }
        member.seen = tokio::time::Instant::now();
        if member.state != state { member.state = state; self.advance(&mut membership)?; }
        Ok(())
    }
    pub(super) fn expire(&self, ttl: Duration) -> Result<(), ()> { self.candidates(ttl).map(|_| ()) }
    fn candidates(&self, ttl: Duration) -> Result<(u64, Vec<Member>), ()> {
        let mut membership = self.membership.lock().map_err(|_| ())?;
        if membership.poisoned { return Err(()); }
        let now = tokio::time::Instant::now();
        let before = membership.members.len();
        membership.members.retain(|_, member| {
            let live = member.peer.live.load(Ordering::Acquire) && now.duration_since(member.seen) < ttl;
            if !live { member.peer.live.store(false, Ordering::Release); }
            live
        });
        if before != membership.members.len() { self.advance(&mut membership)?; }
        let mut members: Vec<_> = membership.members.values().cloned().collect();
        members.sort_by(|a, b| a.peer.connection_id.cmp(&b.peer.connection_id));
        Ok((membership.revision, members))
    }
    // This is a short metadata commit, never a SQL critical section. An
    // intervening join/leave/activity causes a fresh bounded projection attempt.
    fn commit(&self, expected: u64, peers: &[RosterPeer], ttl: Duration) -> Result<Option<u64>, ()> {
        let mut membership = self.membership.lock().map_err(|_| ())?;
        if membership.poisoned { return Err(()); }
        if membership.revision != expected { return Ok(None); }
        let mut changed = false;
        let now = tokio::time::Instant::now();
        membership.members.retain(|id, member| {
            let keep = member.peer.live.load(Ordering::Acquire) && now.duration_since(member.seen) < ttl && peers.iter().any(|peer| &peer.connection_id == id);
            if !keep { member.peer.live.store(false, Ordering::Release); changed = true; }
            keep
        });
        if peers.iter().any(|peer| !membership.members.contains_key(&peer.connection_id)) {
            if changed { self.advance(&mut membership)?; }
            return Ok(None);
        }
        for peer in peers {
            let Some(member) = membership.members.get_mut(&peer.connection_id) else { return Ok(None); };
            if member.label.as_deref() != Some(&peer.display_name) { member.label = Some(peer.display_name.clone()); changed = true; }
        }
        if changed { self.advance(&mut membership)?; }
        Ok(Some(membership.revision))
    }
}
fn display_name(raw: &str) -> String {
    let mut normalized = String::new();
    let mut whitespace = false;
    for ch in raw.chars() {
        if ch.is_control() || ch.is_whitespace() { whitespace = !normalized.is_empty(); continue; }
        if whitespace { normalized.push(' '); whitespace = false; }
        normalized.push(ch);
        // Bound work on historical names as well as the wire presentation.
        if normalized.len() > 256 { break; }
    }
    if normalized.is_empty() { return "Unnamed participant".into(); }
    if normalized.len() > 256 {
        let mut end = 253;
        while !normalized.is_char_boundary(end) { end -= 1; }
        normalized.truncate(end); normalized.push('…');
    }
    normalized
}
async fn allowed(conn: &mut sqlx::AnyConnection, auth: &AuthUser, key: &DraftKey, mode: Mode) -> Result<(), ()> {
    use crate::spaces::access::{self, DraftOperation, Purpose};
    if mode == Mode::Edit { access::require_current_human_preference_write(conn, auth).await.map_err(|_| ())?; }
    else { access::require_current_human_read(conn, auth).await.map_err(|_| ())?; }
    super::super::ws_current_member(conn, auth).await?;
    let principal = access::principal_from_auth(auth).map_err(|_| ())?;
    access::resolve_draft_access(conn, principal, &key.space_id, &key.document_id, &key.draft_id,
        if mode == Mode::Edit { DraftOperation::Edit } else { DraftOperation::View }, Purpose::Routine).await.map_err(|_| ())?;
    access::require_current_human_read(conn, auth).await.map_err(|_| ())?;
    Ok(())
}
pub(super) async fn connection_authority(state: &AppState, auth: &AuthUser, key: &DraftKey, mode: Mode) -> Result<(), ()> {
    tokio::time::timeout(BOUNDARY_TIMEOUT, async {
        let mut conn = state.db.acquire().await.map_err(|_| ())?;
        allowed(&mut conn, auth, key, mode).await
    }).await.map_err(|_| ())?
}
async fn projection(state: &AppState, auth: &AuthUser, sub: &Subscription, members: &[Member], connection_id: &str) -> Result<Vec<RosterPeer>, ()> {
    tokio::time::timeout(BOUNDARY_TIMEOUT, async {
        let mut conn = state.db.acquire().await.map_err(|_| ())?;
        allowed(&mut conn, auth, &sub.key, sub.mode).await?;
        let mut peers = Vec::new();
        for member in members {
            if !member.peer.live.load(Ordering::Acquire) { continue; }
            if allowed(&mut conn, &member.peer.auth, &sub.key, member.peer.mode).await.is_err() {
                if member.peer.connection_id == connection_id { return Err(()); }
                continue;
            }
            let row = sqlx::query("SELECT display_name FROM users WHERE id=$1").bind(&member.peer.auth.claims.sub).fetch_one(&mut *conn).await.map_err(|_| ())?;
            let name: String = row.try_get("display_name").map_err(|_| ())?;
            peers.push(RosterPeer { user_id: member.peer.auth.claims.sub.clone(), connection_id: member.peer.connection_id.clone(), display_name: display_name(&name), state: member.state, mode: member.peer.mode });
        }
        // Final phase after all potentially waiting name/projection reads.
        // Every listed sender and the recipient must still have current access;
        // this does not claim to serialize the later network send with writers.
        for member in members {
            if !peers.iter().any(|peer| peer.connection_id == member.peer.connection_id) { continue; }
            if !member.peer.live.load(Ordering::Acquire) || allowed(&mut conn, &member.peer.auth, &sub.key, member.peer.mode).await.is_err() {
                peers.retain(|peer| peer.connection_id != member.peer.connection_id);
            }
        }
        allowed(&mut conn, auth, &sub.key, sub.mode).await?;
        if !sub.live.0.load(Ordering::Acquire) || !peers.iter().any(|peer| peer.connection_id == connection_id) { return Err(()); }
        Ok(peers)
    }).await.map_err(|_| ())?
}
pub(super) async fn roster(socket: &mut WebSocket, state: &AppState, auth: &AuthUser, connection_id: &str, sub: &Subscription, ttl: Duration) -> Result<(), ()> {
    if !sub.presence_version { return Ok(()); }
    for _ in 0..3 {
        let (revision, members) = sub.room.presence.candidates(ttl)?;
        // Deterministic late-revocation fixture, without duplicating production
        // projection work for every room member on every heartbeat.
        #[cfg(test)]
        if state.ws_rooms.drafts.roster_pause.read().await.as_ref().is_some_and(|pause| pause.connection_id == connection_id) {
            let _ = projection(state, auth, sub, &members, connection_id).await?;
            super::pause(&state.ws_rooms.drafts.roster_pause, connection_id).await;
        }
        let peers = projection(state, auth, sub, &members, connection_id).await?;
        if let Some(revision) = sub.room.presence.commit(revision, &peers, ttl)? {
            if !sub.live.0.load(Ordering::Acquire) { return Err(()); }
            send(socket, &ServerFrame::PresenceRoster { version: VERSION, key: &sub.key, room_id: &sub.room.id, presence_revision: revision, connection_id, peers: &peers }).await?;
            sub.presence_sent.store(revision, Ordering::Release);
            return Ok(());
        }
    }
    Err(())
}
