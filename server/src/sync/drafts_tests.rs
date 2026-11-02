//! Real socket prerequisite acceptance. This does not exercise an editor, offline
//! device queues, discussion, or supported-device convergence.
use crate::{AppState, auth::jwt};
use axum::{body::{Body, to_bytes}, http::{Request, StatusCode}};
use serde_json::{Value, json};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};
use tower::ServiceExt;
use std::time::Duration;

struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server { fn drop(&mut self) { self.0.abort(); } }
fn key(draft: &str) -> Value { json!({"space_id":"space-a","document_id":"target-a","draft_id":draft}) }
async fn seed_drafts(state: &AppState) {
    for id in ["draft-a", "draft-b"] {
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES($1,'target-a','space-a','owner-a',$1,$2,'published')")
            .bind(id).bind(if id == "draft-a" { "A😀BC" } else { "SECOND PRIVATE DRAFT" }).execute(&state.db).await.unwrap();
    }
    sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('draft-a','editor-a','editor','owner-a')").execute(&state.db).await.unwrap();
}
async fn fixture() -> AppState {
    let state = crate::spaces::properties::tests::property_test_state().await;
    seed_drafts(&state).await;
    state
}
async fn server(state: &AppState) -> (std::net::SocketAddr, Server) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = crate::sync::router().with_state(state.clone());
    (address, Server(tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); })))
}
fn token(state: &AppState, user: &str) -> String {
    jwt::create_token_pair_for_session(user, "draft-socket@test.invalid", "member", &state.jwt_secret, Some(&format!("metadata-{user}")), 30).unwrap().access_token
}
async fn send(socket: &mut TcpStream, value: &Value) { send_text(socket, &value.to_string()).await; }
async fn send_text(socket: &mut TcpStream, text: &str) {
    let bytes = text.as_bytes();
    let mut frame = vec![0x81];
    match bytes.len() {
        n if n < 126 => frame.push(0x80 | n as u8),
        n if n <= u16::MAX as usize => { frame.push(0xfe); frame.extend_from_slice(&(n as u16).to_be_bytes()); }
        n => { frame.push(0xff); frame.extend_from_slice(&(n as u64).to_be_bytes()); }
    }
    let mask = [13, 37, 59, 83];
    frame.extend_from_slice(&mask);
    frame.extend(bytes.iter().enumerate().map(|(i, byte)| byte ^ mask[i % 4]));
    socket.write_all(&frame).await.unwrap();
}
async fn read(socket: &mut TcpStream) -> Value {
    loop {
        let (opcode, body) = presence_wire(socket).await;
        match opcode {
            1 => return serde_json::from_slice(&body).unwrap(),
            9 => { assert!(body.len() <= 125); presence_control(socket, 10, &body).await; }
            10 => assert!(body.len() <= 125),
            _ => panic!("expected text before socket retirement (unexpected opcode {opcode})"),
        }
    }
}
async fn next(socket: &mut TcpStream) -> Value {
    tokio::time::timeout(Duration::from_secs(8), read(socket)).await.expect("bounded draft socket response")
}
async fn matching(socket: &mut TcpStream, kind: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop { let value = read(socket).await; if value["type"] == kind { return value; } }
    }).await.expect("bounded matching draft socket response")
}
async fn quiet(socket: &mut TcpStream) {
    assert!(tokio::time::timeout(Duration::from_millis(250), read(socket)).await.is_err(), "unrelated or failed work emitted a frame");
}
async fn connect(address: std::net::SocketAddr, state: &AppState, user: &str) -> (TcpStream, String) {
    let mut socket = TcpStream::connect(address).await.unwrap();
    socket.write_all(format!("GET /ws/drafts/v1 HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").as_bytes()).await.unwrap();
    let mut response = Vec::new();
    while !response.ends_with(b"\r\n\r\n") { let mut byte = [0]; socket.read_exact(&mut byte).await.unwrap(); response.push(byte[0]); assert!(response.len() < 8192); }
    assert!(String::from_utf8(response).unwrap().starts_with("HTTP/1.1 101"));
    send_text(&mut socket, &token(state, user)).await;
    let authenticated = next(&mut socket).await;
    assert_eq!(authenticated["type"], "authenticated"); assert_eq!(authenticated["version"], 1);
    let connection = authenticated["connection_id"].as_str().expect("server connection ID").to_owned();
    assert!(!connection.is_empty());
    (socket, connection)
}
async fn subscribe(socket: &mut TcpStream, draft: &str, mode: &str) -> Value {
    send(socket, &json!({"type":"subscribe","key":key(draft),"mode":mode})).await;
    let snapshot = next(socket).await;
    assert_eq!(snapshot["type"], "snapshot"); assert_eq!(snapshot["version"], 1); assert_eq!(snapshot["key"], key(draft));
    assert_eq!(snapshot["draft"]["id"], draft); assert_eq!(snapshot["draft"]["document_id"], "target-a");
    assert_eq!(snapshot["draft"]["space_id"], "space-a");
    assert_eq!(snapshot["draft"]["source_digest"].as_str().unwrap().len(), 64);
    assert!(snapshot["room_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(snapshot["sequence"].as_u64().is_some()); snapshot
}
fn unavailable(value: &Value) {
    assert!(value["type"] == "unavailable" || value["error"] == "unavailable", "{value}");
    assert!(value.get("draft").is_none()); assert!(value.get("content").is_none());
}
async fn rest_save(state: &AppState, user: &str, source: &str, content: &str) -> (StatusCode, Value) {
    let response = crate::api_routes().with_state(state.clone()).oneshot(Request::builder()
        .method("PUT").uri("/spaces/space-a/documents/target-a/drafts/draft-a")
        .header("Authorization", format!("Bearer {}", token(state, user))).header("Content-Type", "application/json")
        .body(Body::from(json!({"source_digest":source,"content":content}).to_string())).unwrap()).await.unwrap();
    let status = response.status(); let bytes = to_bytes(response.into_body(), 6 * 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn actual_sockets_isolate_drafts_and_deliver_same_user_connection_presence_and_utf16_cursor() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut sender, sender_id) = connect(address, &state, "owner-a").await;
    let (mut peer, peer_id) = connect(address, &state, "owner-a").await;
    let (mut other, _) = connect(address, &state, "owner-a").await;
    assert_ne!(sender_id, peer_id);
    let snapshot = subscribe(&mut sender, "draft-a", "edit").await;
    subscribe(&mut peer, "draft-a", "view").await; subscribe(&mut other, "draft-b", "view").await;
    send(&mut sender, &json!({"type":"presence","key":key("draft-a"),"state":"active"})).await;
    let presence = matching(&mut peer, "presence").await;
    assert_eq!(presence["user_id"], "owner-a"); assert_eq!(presence["connection_id"], sender_id); assert_eq!(presence["key"], key("draft-a"));
    send(&mut sender, &json!({"type":"cursor","key":key("draft-a"),"source_digest":snapshot["draft"]["source_digest"],"anchor":5,"head":5})).await;
    let cursor = matching(&mut peer, "cursor").await;
    assert_eq!(cursor["user_id"], "owner-a"); assert_eq!(cursor["connection_id"], sender_id); assert_eq!(cursor["anchor"], 5); assert_eq!(cursor["head"], 5);
    quiet(&mut other).await;
}

#[tokio::test]
async fn published_page_reader_never_inherits_private_draft_membership() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut reader, _) = connect(address, &state, "viewer-a").await;
    send(&mut reader, &json!({"type":"subscribe","key":key("draft-a"),"mode":"view"})).await;
    unavailable(&next(&mut reader).await);
    let (mut editor, _) = connect(address, &state, "editor-a").await;
    subscribe(&mut editor, "draft-a", "edit").await;
    send(&mut editor, &json!({"type":"subscribe","key":key("draft-b"),"mode":"edit"})).await;
    unavailable(&next(&mut editor).await);
}

#[tokio::test]
async fn unpublished_custom_edit_without_view_uses_mutation_inspection_without_canonical_subscription() {
    let state = fixture().await;
    sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'").execute(&state.db).await.unwrap();
    sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('draft-edit-only','space-a','Edit only','[\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
    sqlx::query("UPDATE space_members SET role='draft-edit-only' WHERE user_id='editor-a'").execute(&state.db).await.unwrap();
    let (address, _server) = server(&state).await;
    let (mut editor, _) = connect(address, &state, "editor-a").await;
    let snapshot = subscribe(&mut editor, "draft-a", "edit").await;
    assert_eq!(snapshot["draft"]["content"], "A😀BC");
    let (status, _) = rest_save(&state, "editor-a", snapshot["draft"]["source_digest"].as_str().unwrap(), "EDIT ONLY SHARED").await;
    assert_eq!(status, StatusCode::OK);
    let (mut viewer_mode, _) = connect(address, &state, "editor-a").await;
    send(&mut viewer_mode, &json!({"type":"subscribe","key":key("draft-a"),"mode":"view"})).await;
    unavailable(&next(&mut viewer_mode).await);
}

#[tokio::test]
async fn locked_draft_view_is_readable_but_edit_and_cursor_are_not_authorized() {
    let state = fixture().await;
    sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'").execute(&state.db).await.unwrap();
    let (address, _server) = server(&state).await;
    let (mut viewer_mode, _) = connect(address, &state, "editor-a").await;
    let snapshot = subscribe(&mut viewer_mode, "draft-a", "view").await;
    send(&mut viewer_mode, &json!({"type":"cursor","key":key("draft-a"),"source_digest":snapshot["draft"]["source_digest"],"anchor":0,"head":0})).await;
    unavailable(&next(&mut viewer_mode).await);
    let (mut editor_mode, _) = connect(address, &state, "editor-a").await;
    send(&mut editor_mode, &json!({"type":"subscribe","key":key("draft-a"),"mode":"edit"})).await;
    unavailable(&next(&mut editor_mode).await);
}

async fn rest_commit_contract(state: AppState) {
    let (address, _server) = server(&state).await;
    let (mut socket, _) = connect(address, &state, "editor-a").await;
    let before = subscribe(&mut socket, "draft-a", "edit").await;
    let source = before["draft"]["source_digest"].as_str().unwrap();
    let (status, _) = rest_save(&state, "editor-a", &"0".repeat(64), "MUST NOT APPLY").await;
    assert_eq!(status, StatusCode::CONFLICT); quiet(&mut socket).await;
    let (status, saved) = rest_save(&state, "editor-a", source, "CONFIRMED SHARED BODY\r\n").await;
    assert_eq!(status, StatusCode::OK);
    let event = matching(&mut socket, "invalidate").await;
    assert_eq!(event["reason"], "committed"); assert_eq!(event["key"], key("draft-a")); assert_eq!(event["room_id"], before["room_id"]);
    assert!(event["sequence"].as_u64().unwrap() > before["sequence"].as_u64().unwrap());
    assert_eq!(event["source_digest"], saved["source_digest"]);
    assert!(event.get("draft").is_none()); assert!(event.get("content").is_none());
    send(&mut socket, &json!({"type":"resync","key":key("draft-a")})).await;
    let after = matching(&mut socket, "snapshot").await;
    assert_eq!(after["draft"]["content"], "CONFIRMED SHARED BODY\r\n"); assert_eq!(after["draft"]["source_digest"], saved["source_digest"]);
    let canonical: Option<String> = sqlx::query_scalar("SELECT published_at FROM documents WHERE id='target-a'").fetch_one(&state.db).await.unwrap();
    assert_eq!(canonical.as_deref(), Some("published"));
}
#[tokio::test]
async fn actual_rest_commit_invalidates_current_draft_and_failed_cas_emits_nothing() { rest_commit_contract(fixture().await).await; }

#[tokio::test]
async fn reconnect_and_new_application_room_require_fresh_source_without_reusing_old_incarnation() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut first, first_id) = connect(address, &state, "editor-a").await;
    let before = subscribe(&mut first, "draft-a", "edit").await;
    let (status, saved) = rest_save(&state, "editor-a", before["draft"]["source_digest"].as_str().unwrap(), "RECONNECT RETAINED SOURCE").await;
    assert_eq!(status, StatusCode::OK);
    matching(&mut first, "invalidate").await; drop(first);
    let (mut reconnected, second_id) = connect(address, &state, "editor-a").await;
    assert_ne!(first_id, second_id);
    let current = subscribe(&mut reconnected, "draft-a", "edit").await;
    assert_eq!(current["draft"]["source_digest"], saved["source_digest"]);
    assert_eq!(current["draft"]["content"], "RECONNECT RETAINED SOURCE");
    // A new application room registry models process-local transport restart;
    // it grants no durable save ACK and never reuses the prior room incarnation.
    let mut restarted = state.clone(); restarted.ws_rooms = crate::sync::WsRooms::default();
    let (new_address, _new_server) = server(&restarted).await;
    let (mut after_restart, _) = connect(new_address, &restarted, "editor-a").await;
    let fresh = subscribe(&mut after_restart, "draft-a", "edit").await;
    assert_ne!(fresh["room_id"], current["room_id"]);
    assert_eq!(fresh["draft"]["source_digest"], current["draft"]["source_digest"]);
    assert!(fresh.get("operation_id").is_none());
}

#[tokio::test]
async fn subscribe_waiting_for_source_connection_reads_current_committed_draft() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut socket, _) = connect(address, &state, "editor-a").await;
    // The actual single-connection SQLite pool blocks subscription inspection
    // until this original source transaction commits; no socket pause mock.
    let mut transaction = state.db.begin().await.unwrap();
    send(&mut socket, &json!({"type":"subscribe","key":key("draft-a"),"mode":"edit"})).await;
    sqlx::query("UPDATE document_drafts SET content='COMMITTED WHILE SUBSCRIBE WAITED' WHERE id='draft-a'").execute(&mut *transaction).await.unwrap();
    transaction.commit().await.unwrap();
    let snapshot = next(&mut socket).await;
    assert_eq!(snapshot["type"], "snapshot");
    assert_eq!(snapshot["draft"]["content"], "COMMITTED WHILE SUBSCRIBE WAITED");
    let raw = crate::spaces::draft_save::selected(&mut state.db.acquire().await.unwrap(), "space-a", "target-a", "draft-a").await.unwrap();
    assert_eq!(snapshot["draft"]["source_digest"], crate::spaces::draft_save::digest(&raw).unwrap());
}

#[tokio::test]
async fn eventless_writer_is_detected_by_bounded_fresh_source_and_requires_resync() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut socket, _) = connect(address, &state, "editor-a").await;
    let original = subscribe(&mut socket, "draft-a", "edit").await;
    sqlx::query("UPDATE document_drafts SET content='EVENTLESS REPLAY REFERENCE',title='Remote metadata' WHERE id='draft-a'").execute(&state.db).await.unwrap();
    let event = matching(&mut socket, "invalidate").await;
    assert_eq!(event["reason"], "source_changed"); assert_ne!(event["source_digest"], original["draft"]["source_digest"]);
    send(&mut socket, &json!({"type":"resync","key":key("draft-a")})).await;
    let snapshot = matching(&mut socket, "snapshot").await;
    assert_eq!(snapshot["draft"]["content"], "EVENTLESS REPLAY REFERENCE"); assert_eq!(snapshot["draft"]["title"], "Remote metadata");
}

#[tokio::test]
async fn eventless_owner_display_and_viewer_role_changes_invalidate_projection_without_changing_raw_source() {
    for (statement, field, expected) in [
        ("UPDATE users SET display_name='Renamed original owner' WHERE id='owner-a'", "owner_name", "Renamed original owner"),
        ("UPDATE document_draft_access SET role='viewer' WHERE draft_id='draft-a' AND user_id='editor-a'", "access_role", "viewer"),
    ] {
        let state = fixture().await; let (address, _server) = server(&state).await;
        let (mut socket, _) = connect(address, &state, "editor-a").await;
        let before = subscribe(&mut socket, "draft-a", "view").await;
        sqlx::query(statement).execute(&state.db).await.unwrap();
        let event = matching(&mut socket, "invalidate").await;
        assert_eq!(event["reason"], "source_changed");
        assert_eq!(event["source_digest"], before["draft"]["source_digest"]);
        send(&mut socket, &json!({"type":"resync","key":key("draft-a")})).await;
        let after = matching(&mut socket, "snapshot").await;
        assert_eq!(after["draft"][field], expected);
        assert_eq!(after["draft"]["source_digest"], before["draft"]["source_digest"]);
        assert_eq!(after["draft"]["content"], before["draft"]["content"]);
    }
}

#[tokio::test]
async fn current_restricted_ancestor_denies_existing_draft_without_disclosing_source() {
    let state = fixture().await;
    sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('draft-parent','space-a','Parent','','owner-a','owner-a','published')").execute(&state.db).await.unwrap();
    sqlx::query("UPDATE documents SET parent_id='draft-parent' WHERE id='target-a'").execute(&state.db).await.unwrap();
    let (address, _server) = server(&state).await;
    let (mut socket, _) = connect(address, &state, "editor-a").await;
    subscribe(&mut socket, "draft-a", "edit").await;
    sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('draft-parent','space-a','restricted','later-policy','owner-a')").execute(&state.db).await.unwrap();
    send(&mut socket, &json!({"type":"resync","key":key("draft-a")})).await;
    unavailable(&next(&mut socket).await);
}

#[tokio::test]
async fn bounded_room_receiver_reports_actual_lag_and_resubscribes_to_current_sequence() {
    use super::{Room, EventKind, ROOM_CAPACITY};
    let room = Room::new(); let mut receiver = room.subscribe().unwrap();
    for _ in 0..(2 * ROOM_CAPACITY + 1) { room.emit(EventKind::SourceChanged, None).unwrap(); }
    assert!(matches!(receiver.recv().await, Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) if skipped > 0));
    let observed = room.sequence().unwrap();
    let mut fresh = room.relisten().unwrap();
    assert!(fresh.is_empty());
    room.emit(EventKind::SourceChanged, None).unwrap();
    let next = fresh.recv().await.unwrap();
    assert_eq!(next.sequence, observed + 1);
}

#[tokio::test]
async fn actual_socket_lag_emits_explicit_resync_gap_and_fresh_snapshot_without_save_ack() {
    use super::{DraftKey, EventKind, ReceivePause, ROOM_CAPACITY};
    use std::sync::Arc;
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut socket, connection_id) = connect(address, &state, "editor-a").await;
    let gate = Arc::new(ReceivePause { connection_id, reached: tokio::sync::Notify::new(), release: tokio::sync::Notify::new() });
    *state.ws_rooms.drafts.receive_pause.write().await = Some(gate.clone());
    let before = subscribe(&mut socket, "draft-a", "edit").await;
    tokio::time::timeout(Duration::from_secs(8), gate.reached.notified()).await.expect("actual socket receiver pause reached");
    let typed: DraftKey = serde_json::from_value(key("draft-a")).unwrap();
    let room = state.ws_rooms.drafts.rooms.read().await.get(&typed).unwrap().clone();
    for _ in 0..(2 * ROOM_CAPACITY + 1) { room.emit(EventKind::SourceChanged, None).unwrap(); }
    let last_sequence = room.sequence().unwrap();
    gate.release.notify_one();
    let gap = matching(&mut socket, "invalidate").await;
    assert_eq!(gap["reason"], "resync_required"); assert_eq!(gap["sequence"], last_sequence);
    assert_eq!(gap["source_digest"], before["draft"]["source_digest"]);
    let fresh = matching(&mut socket, "snapshot").await;
    assert_eq!(fresh["room_id"], before["room_id"]);
    assert_eq!(fresh["draft"]["source_digest"], before["draft"]["source_digest"]);
    assert_eq!(fresh["draft"]["content"], before["draft"]["content"]);
    assert!(gap.get("operation_id").is_none()); assert!(fresh.get("operation_id").is_none());
}

#[tokio::test]
async fn original_sender_revocation_after_recipient_projection_drops_queued_presence_and_cursor() {
    use super::ReceivePause;
    use std::sync::Arc;
    for kind in ["presence", "cursor"] {
        let state = fixture().await; let (address, _server) = server(&state).await;
        let (mut sender, _) = connect(address, &state, "editor-a").await;
        let source = subscribe(&mut sender, "draft-a", "edit").await;
        let (mut recipient, recipient_id) = connect(address, &state, "owner-a").await;
        subscribe(&mut recipient, "draft-a", "view").await;
        let gate = Arc::new(ReceivePause { connection_id: recipient_id, reached: tokio::sync::Notify::new(), release: tokio::sync::Notify::new() });
        *state.ws_rooms.drafts.pair_pause.write().await = Some(gate.clone());
        let frame = if kind == "presence" { json!({"type":"presence","key":key("draft-a"),"state":"active"}) }
            else { json!({"type":"cursor","key":key("draft-a"),"source_digest":source["draft"]["source_digest"],"anchor":0,"head":0}) };
        send(&mut sender, &frame).await;
        tokio::time::timeout(Duration::from_secs(8), gate.reached.notified()).await.expect("actual recipient projection reached pair fence");
        sqlx::query("DELETE FROM document_draft_access WHERE draft_id='draft-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
        gate.release.notify_one();
        quiet(&mut recipient).await;
        // Recipient remains authorized and can explicitly inspect the same source.
        send(&mut recipient, &json!({"type":"resync","key":key("draft-a")})).await;
        assert_eq!(matching(&mut recipient, "snapshot").await["draft"]["source_digest"], source["draft"]["source_digest"]);
    }
}

#[tokio::test]
async fn current_share_session_lock_lifecycle_and_consumed_draft_stop_socket_activity() {
    for sql in [
        "DELETE FROM document_draft_access WHERE draft_id='draft-a' AND user_id='editor-a'",
        "DELETE FROM sessions WHERE id='metadata-editor-a'",
        "UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE id='metadata-editor-a'",
        "UPDATE users SET account_kind='guest' WHERE id='editor-a'",
        "UPDATE users SET is_active=0 WHERE id='editor-a'",
        "UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'",
        "UPDATE documents SET archived_at='later' WHERE id='target-a'",
        "UPDATE documents SET deleted_at='later' WHERE id='target-a'",
        "DELETE FROM document_drafts WHERE id='draft-a'",
    ] {
        let state = fixture().await; let (address, _server) = server(&state).await;
        let (mut socket, _) = connect(address, &state, "editor-a").await; subscribe(&mut socket, "draft-a", "edit").await;
        sqlx::query(sql).execute(&state.db).await.unwrap();
        send(&mut socket, &json!({"type":"presence","key":key("draft-a"),"state":"active"})).await;
        unavailable(&next(&mut socket).await);
    }
}

#[tokio::test]
async fn idle_revocation_stops_future_delivery_without_requiring_client_activity() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut socket, _) = connect(address, &state, "editor-a").await; subscribe(&mut socket, "draft-a", "view").await;
    sqlx::query("DELETE FROM document_draft_access WHERE draft_id='draft-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
    unavailable(&next(&mut socket).await);
}

#[tokio::test]
async fn malformed_body_write_spoof_cross_room_and_invalid_cursor_frames_never_mutate_draft() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let before = crate::spaces::draft_save::selected(&mut state.db.acquire().await.unwrap(), "space-a", "target-a", "draft-a").await.unwrap();
    for mut invalid in [
        json!({"type":"change","key":key("draft-a"),"content":"FORGED BODY"}),
        json!({"type":"presence","key":key("draft-a"),"state":"active","user_id":"owner-a"}),
        json!({"type":"presence","key":key("draft-b"),"state":"active"}),
        json!({"type":"cursor","key":key("draft-a"),"source_digest":"0".repeat(64),"anchor":0,"head":0}),
        json!({"type":"cursor","key":key("draft-a"),"source_digest":null,"anchor":u32::MAX,"head":u32::MAX}),
        json!({"type":"cursor","key":key("draft-a"),"source_digest":"current","anchor":6,"head":6}),
        // UTF16 offset2 splits the emoji's surrogate pair: not a text boundary.
        json!({"type":"cursor","key":key("draft-a"),"source_digest":"current","anchor":2,"head":2}),
        json!({"type":"resync","key":{"space_id":"x".repeat(10000),"document_id":"target-a","draft_id":"draft-a"}}),
    ] {
        let (mut socket, _) = connect(address, &state, "editor-a").await;
        let snapshot = subscribe(&mut socket, "draft-a", "edit").await;
        if invalid["source_digest"] == "current" { invalid["source_digest"] = snapshot["draft"]["source_digest"].clone(); }
        send(&mut socket, &invalid).await; unavailable(&next(&mut socket).await);
    }
    let after = crate::spaces::draft_save::selected(&mut state.db.acquire().await.unwrap(), "space-a", "target-a", "draft-a").await.unwrap();
    assert_eq!(after, before);
}

#[tokio::test]
#[ignore = "requires owned STARLANE_CORE_WORKSPACE, STARLANE_CORE_PG_URL and DATA_DIR; run alone"]
async fn required_postgres_actual_draft_socket_rest_commit_and_source_ack() {
    use sqlx::Connection;
    sqlx::any::install_default_drivers();
    let owned = std::path::PathBuf::from(std::env::var("STARLANE_CORE_WORKSPACE").expect("owned workspace required"));
    let root = crate::storage::data_dir(); assert!(owned.is_absolute() && root.starts_with(&owned) && root != owned);
    let url = std::env::var("STARLANE_CORE_PG_URL").expect("owned PostgreSQL URL required"); assert!(url.starts_with("postgres://"));
    let schema = format!("draft_socket_{}", uuid::Uuid::new_v4().simple());
    let mut control = sqlx::AnyConnection::connect(&url).await.unwrap();
    sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut control).await.unwrap();
    let selected = schema.clone();
    let pool = sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(move |conn, _| { let schema = selected.clone(); Box::pin(async move { sqlx::query(&format!("SET search_path TO {schema}")).execute(conn).await?; Ok(()) }) }).connect(&url).await.unwrap();
    crate::db::migrate_selected(&pool, crate::db::DatabaseEngine::Postgres).await.unwrap();
    let mut state = crate::spaces::properties::tests::property_test_state().await; state.db.close().await; state.db = pool;
    for user in ["owner-a", "viewer-a", "editor-a"] {
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')").bind(user).bind(format!("{user}@draft.test.invalid")).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'fixture','2999-01-01T00:00:00Z')").bind(format!("metadata-{user}")).bind(user).execute(&state.db).await.unwrap();
    }
    sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('space-a','Draft sockets','draft-sockets','private','owner-a')").execute(&state.db).await.unwrap();
    for (user, role) in [("owner-a","owner"),("viewer-a","viewer"),("editor-a","editor")] {
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)").bind(format!("member-{user}")).bind(user).bind(role).execute(&state.db).await.unwrap();
    }
    sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Published page','','owner-a','owner-a','published')").execute(&state.db).await.unwrap();
    seed_drafts(&state).await;
    rest_commit_contract(state.clone()).await;
    // Real PostgreSQL projections and membership checks, using the same owned
    // migrated schema and the original REST-confirmed source above.
    let (address, _presence_server) = server(&state).await;
    let (mut legacy, owner_id) = connect(address, &state, "owner-a").await;
    let original = subscribe(&mut legacy, "draft-a", "view").await;
    assert_eq!(original["draft"]["content"], "CONFIRMED SHARED BODY\r\n");
    let (mut editor, editor_id) = connect(address, &state, "editor-a").await;
    let (before, roster) = subscribe_roster(&mut editor, "draft-a", "edit", &editor_id).await;
    assert_eq!(before["draft"]["source_digest"], original["draft"]["source_digest"]);
    assert_eq!(roster_peer(&roster, &owner_id)["user_id"], "owner-a");
    assert_eq!(roster_peer(&roster, &owner_id)["mode"], "view");
    assert_eq!(roster_peer(&roster, &editor_id)["mode"], "edit");
    quiet(&mut legacy).await; // Opt-in roster JSON must not reach legacy v1.
    send(&mut editor, &json!({"type":"presence","key":key("draft-a"),"state":"idle"})).await;
    let old_frame = next(&mut legacy).await;
    assert_eq!(old_frame["type"], "presence");
    assert_eq!(old_frame["connection_id"], editor_id); assert_eq!(old_frame["state"], "idle");
    assert!(old_frame.get("peers").is_none());
    roster_until(&mut editor, "draft-a", &editor_id, |r| roster_peer(r, &editor_id)["state"] == "idle").await;
    send(&mut editor, &json!({"type":"resync","key":key("draft-a")})).await;
    let after = tokio::time::timeout(Duration::from_secs(8), async {
        loop { let frame = presence_next(&mut editor).await; if frame["type"] == "snapshot" { break frame; } }
    }).await.unwrap();
    assert_eq!(after["sequence"], before["sequence"], "PostgreSQL presence cannot advance negotiated source ordering");
    assert_eq!(after["draft"], before["draft"], "Presence cannot acknowledge or rewrite the REST source");
    sqlx::query("UPDATE users SET display_name='PostgreSQL Current Participant' WHERE id='editor-a'").execute(&state.db).await.unwrap();
    send(&mut editor, &json!({"type":"resync","key":key("draft-a")})).await;
    let fresh = roster_until(&mut editor, "draft-a", &editor_id, |r| roster_peer(r, &editor_id)["display_name"] == "PostgreSQL Current Participant").await;
    assert_eq!(roster_peer(&fresh, &editor_id)["user_id"], "editor-a");
    // Upgrade this exact legacy connection deliberately; it must now receive
    // the same fresh PostgreSQL-backed membership, without a new identity.
    let (_, owner_roster) = subscribe_roster(&mut legacy, "draft-a", "view", &owner_id).await;
    assert_eq!(roster_peer(&owner_roster, &editor_id)["display_name"], "PostgreSQL Current Participant");
    sqlx::query("DELETE FROM document_draft_access WHERE draft_id='draft-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
    send(&mut editor, &json!({"type":"resync","key":key("draft-a")})).await;
    let denied = tokio::time::timeout(Duration::from_secs(8), async {
        loop { let frame = presence_next(&mut editor).await; if frame["type"] == "unavailable" || frame["type"] == "closed" { break frame; } }
    }).await.unwrap();
    assert!(denied.get("draft").is_none()); assert!(denied.get("peers").is_none());
    send(&mut legacy, &json!({"type":"resync","key":key("draft-a")})).await;
    let remaining = roster_until(&mut legacy, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().iter().all(|peer| peer["connection_id"] != editor_id)).await;
    assert_eq!(remaining["connection_id"], owner_id);
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT content FROM document_drafts WHERE id='draft-a'").fetch_one(&state.db).await.unwrap(), "CONFIRMED SHARED BODY\r\n");
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT content FROM document_drafts WHERE id='draft-b'").fetch_one(&state.db).await.unwrap(), "SECOND PRIVATE DRAFT");
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT published_at FROM documents WHERE id='target-a'").fetch_one(&state.db).await.unwrap(), "published");
    // Retire the real sockets before dropping the uniquely owned SQL schema.
    drop(editor); drop(legacy); drop(_presence_server);
    state.db.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&mut control).await.unwrap();
    control.close().await.unwrap();
}


// Opt-in roster acceptance uses real framed sockets. Legacy helpers/assertions
// above intentionally remain unchanged to catch accidental v1 wire widening.
async fn presence_control(socket: &mut TcpStream, opcode: u8, bytes: &[u8]) {
    assert!(bytes.len() <= 125);
    let mask = [7, 19, 43, 71];
    let mut frame = vec![0x80 | opcode, 0x80 | bytes.len() as u8];
    frame.extend_from_slice(&mask);
    frame.extend(bytes.iter().enumerate().map(|(index, byte)| byte ^ mask[index % 4]));
    socket.write_all(&frame).await.unwrap();
}
async fn presence_wire(socket: &mut TcpStream) -> (u8, Vec<u8>) {
    let mut header = [0u8; 2]; socket.read_exact(&mut header).await.unwrap();
    assert_eq!(header[1] & 0x80, 0);
    let mut length = (header[1] & 0x7f) as u64;
    if length == 126 { let mut n = [0u8; 2]; socket.read_exact(&mut n).await.unwrap(); length = u16::from_be_bytes(n) as u64; }
    if length == 127 { let mut n = [0u8; 8]; socket.read_exact(&mut n).await.unwrap(); length = u64::from_be_bytes(n); }
    assert!(length <= 6 * 1024 * 1024);
    let mut bytes = vec![0u8; length as usize]; socket.read_exact(&mut bytes).await.unwrap();
    (header[0] & 15, bytes)
}
async fn presence_next(socket: &mut TcpStream) -> Value {
    loop {
        let (opcode, bytes) = presence_wire(socket).await;
        match opcode {
            1 => return serde_json::from_slice(&bytes).unwrap(),
            9 => presence_control(socket, 10, &bytes).await,
            10 => (),
            8 => return json!({"type":"closed"}),
            _ => panic!("Unexpected presence wire opcode {opcode}"),
        }
    }
}
fn assert_roster(value: &Value, draft: &str, connection: &str) {
    assert_eq!(value["type"], "presence_roster"); assert_eq!(value["version"], 1);
    assert_eq!(value["key"], key(draft)); assert_eq!(value["connection_id"], connection);
    assert!(value["room_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(value["presence_revision"].as_u64().is_some());
    assert!(value.get("sequence").is_none()); assert!(value.get("source_digest").is_none());
    assert_eq!(value.as_object().unwrap().len(), 7, "roster must not carry draft/source/ACK fields");
    let peers = value["peers"].as_array().unwrap();
    assert!(!peers.is_empty() && peers.len() <= 64);
    let mut connections = std::collections::BTreeSet::new();
    for peer in peers {
        assert_eq!(peer.as_object().unwrap().len(), 5);
        assert!(peer["user_id"].as_str().is_some_and(|id| !id.is_empty()));
        let id = peer["connection_id"].as_str().unwrap();
        assert!(connections.insert(id));
        assert!(matches!(peer["state"].as_str(), Some("active" | "idle")));
        assert!(matches!(peer["mode"].as_str(), Some("view" | "edit")));
        let name = peer["display_name"].as_str().unwrap();
        assert!(!name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control));
    }
    assert_eq!(peers.iter().filter(|peer| peer["connection_id"] == connection).count(), 1);
}
async fn roster_until(socket: &mut TcpStream, draft: &str, connection: &str, matches: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let value = presence_next(socket).await;
            assert_ne!(value["type"], "unavailable", "authorized roster recipient retired unexpectedly: {value}");
            assert_ne!(value["type"], "closed", "authorized roster recipient closed unexpectedly");
            if value["type"] == "presence_roster" {
                assert_roster(&value, draft, connection);
                if matches(&value) { return value; }
            }
        }
    }).await.expect("bounded authorized presence roster")
}
async fn subscribe_roster(socket: &mut TcpStream, draft: &str, mode: &str, connection: &str) -> (Value, Value) {
    send(socket, &json!({"type":"subscribe","key":key(draft),"mode":mode,"presence_version":1})).await;
    let snapshot = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let value = presence_next(socket).await;
            if value["type"] == "snapshot" { break value; }
            assert_ne!(value["type"], "unavailable"); assert_ne!(value["type"], "closed");
        }
    }).await.unwrap();
    assert_eq!(snapshot["key"], key(draft)); assert_eq!(snapshot["connection_id"], connection);
    let roster = roster_until(socket, draft, connection, |_| true).await;
    assert_eq!(roster["room_id"], snapshot["room_id"]);
    (snapshot, roster)
}
fn roster_peer<'a>(value: &'a Value, connection: &str) -> &'a Value {
    value["peers"].as_array().unwrap().iter().find(|peer| peer["connection_id"] == connection).expect("specific server connection in roster")
}

#[tokio::test]
async fn roster_initial_members_activity_and_same_user_connections_do_not_advance_source_sequence() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut owner, owner_id) = connect(address, &state, "owner-a").await;
    let (original, first) = subscribe_roster(&mut owner, "draft-a", "view", &owner_id).await;
    assert_eq!(first["peers"].as_array().unwrap().len(), 1);
    let (mut editor, editor_id) = connect(address, &state, "editor-a").await;
    let (_, editor_roster) = subscribe_roster(&mut editor, "draft-a", "edit", &editor_id).await;
    assert_eq!(editor_roster["peers"].as_array().unwrap().len(), 2);
    assert_eq!(roster_peer(&editor_roster, &owner_id)["display_name"], "owner-a");
    assert_eq!(roster_peer(&editor_roster, &owner_id)["mode"], "view");
    let joined = roster_until(&mut owner, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().len() == 2).await;
    assert_eq!(roster_peer(&joined, &editor_id)["state"], "active");
    let (mut same_user, same_id) = connect(address, &state, "owner-a").await;
    assert_ne!(same_id, owner_id);
    subscribe_roster(&mut same_user, "draft-a", "edit", &same_id).await;
    let three = roster_until(&mut owner, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().len() == 3).await;
    assert_eq!(roster_peer(&three, &same_id)["user_id"], "owner-a");
    assert_eq!(roster_peer(&three, &same_id)["mode"], "edit");
    send(&mut editor, &json!({"type":"presence","key":key("draft-a"),"state":"idle"})).await;
    let idle = roster_until(&mut owner, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().iter().any(|p| p["connection_id"] == editor_id && p["state"] == "idle")).await;
    assert!(idle["presence_revision"].as_u64().unwrap() > three["presence_revision"].as_u64().unwrap());
    send(&mut editor, &json!({"type":"presence","key":key("draft-a"),"state":"active"})).await;
    roster_until(&mut owner, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().iter().any(|p| p["connection_id"] == editor_id && p["state"] == "active")).await;
    send(&mut owner, &json!({"type":"resync","key":key("draft-a")})).await;
    let current = tokio::time::timeout(Duration::from_secs(8), async {
        loop { let frame = presence_next(&mut owner).await; if frame["type"] == "snapshot" { break frame; } }
    }).await.unwrap();
    assert_eq!(current["sequence"], original["sequence"], "presence cannot consume source/CAS ordering");
    assert_eq!(current["draft"], original["draft"], "presence must not mutate source metadata");
    assert_eq!(current["draft"]["content"], "A😀BC");
}

#[tokio::test]
async fn roster_legacy_subscriptions_are_visible_but_receive_only_strict_original_v1_frames() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut legacy, legacy_id) = connect(address, &state, "owner-a").await;
    let original = subscribe(&mut legacy, "draft-a", "view").await;
    let (mut opted, opted_id) = connect(address, &state, "editor-a").await;
    let (_, roster) = subscribe_roster(&mut opted, "draft-a", "edit", &opted_id).await;
    assert_eq!(roster["peers"].as_array().unwrap().len(), 2);
    assert_eq!(roster_peer(&roster, &legacy_id)["user_id"], "owner-a");
    quiet(&mut legacy).await;
    send(&mut legacy, &json!({"type":"resync","key":key("draft-a")})).await;
    let snapshot = next(&mut legacy).await;
    assert_eq!(snapshot["type"], "snapshot"); assert_eq!(snapshot["sequence"], original["sequence"]);
    quiet(&mut legacy).await;
    send(&mut legacy, &json!({"type":"presence","key":key("draft-a"),"state":"idle"})).await;
    roster_until(&mut opted, "draft-a", &opted_id, |r| r["peers"].as_array().unwrap().iter().any(|p| p["connection_id"] == legacy_id && p["state"] == "idle")).await;
    quiet(&mut legacy).await;
}

#[tokio::test]
async fn roster_explicit_leave_abrupt_close_reconnect_and_draft_switch_replace_membership() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut observer, observer_id) = connect(address, &state, "owner-a").await;
    subscribe_roster(&mut observer, "draft-a", "view", &observer_id).await;
    let (mut peer, peer_id) = connect(address, &state, "owner-a").await;
    subscribe_roster(&mut peer, "draft-a", "edit", &peer_id).await;
    roster_until(&mut observer, "draft-a", &observer_id, |r| r["peers"].as_array().unwrap().len() == 2).await;
    subscribe_roster(&mut peer, "draft-b", "view", &peer_id).await;
    let switched = roster_until(&mut observer, "draft-a", &observer_id, |r| r["peers"].as_array().unwrap().len() == 1).await;
    assert!(switched["peers"].as_array().unwrap().iter().all(|p| p["connection_id"] != peer_id));
    let (_, back) = subscribe_roster(&mut peer, "draft-a", "edit", &peer_id).await;
    assert_eq!(back["peers"].as_array().unwrap().len(), 2);
    presence_control(&mut peer, 8, &[]).await;
    roster_until(&mut observer, "draft-a", &observer_id, |r| r["peers"].as_array().unwrap().len() == 1).await;
    let (mut reconnected, next_id) = connect(address, &state, "owner-a").await;
    assert_ne!(next_id, peer_id);
    subscribe_roster(&mut reconnected, "draft-a", "view", &next_id).await;
    let joined = roster_until(&mut observer, "draft-a", &observer_id, |r| r["peers"].as_array().unwrap().iter().any(|p| p["connection_id"] == next_id)).await;
    assert!(joined["peers"].as_array().unwrap().iter().all(|p| p["connection_id"] != peer_id));
    drop(reconnected);
    roster_until(&mut observer, "draft-a", &observer_id, |r| r["peers"].as_array().unwrap().len() == 1).await;
}

#[tokio::test]
async fn roster_current_authority_removes_revoked_locked_and_consumed_peers_without_private_fallback() {
    for sql in [
        "DELETE FROM document_draft_access WHERE draft_id='draft-a' AND user_id='editor-a'",
        "UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'",
        "DELETE FROM document_drafts WHERE id='draft-a'",
    ] {
        let state = fixture().await; let (address, _server) = server(&state).await;
        let (mut owner, owner_id) = connect(address, &state, "owner-a").await;
        subscribe_roster(&mut owner, "draft-a", "view", &owner_id).await;
        let (mut editor, editor_id) = connect(address, &state, "editor-a").await;
        subscribe_roster(&mut editor, "draft-a", "edit", &editor_id).await;
        roster_until(&mut owner, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().len() == 2).await;
        sqlx::query(sql).execute(&state.db).await.unwrap();
        send(&mut editor, &json!({"type":"presence","key":key("draft-a"),"state":"idle"})).await;
        let denied = tokio::time::timeout(Duration::from_secs(8), async {
            loop { let frame = presence_next(&mut editor).await; if frame["type"] == "unavailable" || frame["type"] == "closed" { break frame; } }
        }).await.unwrap();
        assert!(denied.get("draft").is_none()); assert!(denied.get("peers").is_none());
        if sql.starts_with("DELETE FROM document_drafts") {
            let retired = tokio::time::timeout(Duration::from_secs(8), async {
                loop { let frame = presence_next(&mut owner).await; if frame["type"] == "unavailable" || frame["type"] == "closed" { break frame; } }
            }).await.unwrap();
            assert!(retired.get("draft").is_none()); assert!(retired.get("peers").is_none());
        } else {
            roster_until(&mut owner, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().iter().all(|p| p["connection_id"] != editor_id)).await;
        }
    }
}

#[tokio::test]
async fn roster_historical_display_names_are_normalized_bounded_and_never_deny_participation() {
    for raw in [" \n Legacy\t  Name \u{2028} ".to_owned(), "😀".repeat(200), " \t\n".to_owned()] {
        let state = fixture().await;
        sqlx::query("UPDATE users SET display_name=$1 WHERE id='editor-a'").bind(&raw).execute(&state.db).await.unwrap();
        let (address, _server) = server(&state).await;
        let (mut owner, owner_id) = connect(address, &state, "owner-a").await;
        subscribe_roster(&mut owner, "draft-a", "view", &owner_id).await;
        let (mut editor, editor_id) = connect(address, &state, "editor-a").await;
        let (_, joined) = subscribe_roster(&mut editor, "draft-a", "edit", &editor_id).await;
        let label = roster_peer(&joined, &editor_id)["display_name"].as_str().unwrap();
        if raw.contains("Legacy") { assert_eq!(label, "Legacy Name"); }
        else if raw.trim().is_empty() { assert_eq!(label, "Unnamed participant"); }
        else { assert!(label.ends_with('…')); assert!(label.len() <= 256); }
        let unchanged: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id='editor-a'").fetch_one(&state.db).await.unwrap();
        assert_eq!(unchanged, raw, "presentation normalization cannot rewrite original stored names");
        let owner_roster = roster_until(&mut owner, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().len() == 2).await;
        assert_eq!(roster_peer(&owner_roster, &editor_id)["display_name"], label);
    }
}


#[tokio::test]
async fn roster_room_capacity_is_bounded_without_leaking_other_draft_membership() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut observer, observer_id) = connect(address, &state, "owner-a").await;
    let (original, _) = subscribe_roster(&mut observer, "draft-a", "view", &observer_id).await;
    let (mut other, other_id) = connect(address, &state, "owner-a").await;
    let (_, isolated) = subscribe_roster(&mut other, "draft-b", "view", &other_id).await;
    assert_eq!(isolated["peers"].as_array().unwrap().len(), 1);
    let mut held = Vec::new();
    for _ in 1..64 {
        let (mut peer, id) = connect(address, &state, "owner-a").await;
        // Legacy subscribers consume the SAME capacity and remain visible.
        subscribe(&mut peer, "draft-a", "view").await;
        held.push((peer, id));
    }
    let full = roster_until(&mut observer, "draft-a", &observer_id, |r| r["peers"].as_array().unwrap().len() == 64).await;
    assert!(full["peers"].as_array().unwrap().iter().all(|p| p["connection_id"] != other_id));
    for (_, id) in &held { assert_eq!(roster_peer(&full, id)["user_id"], "owner-a"); }
    let (mut excess, _) = connect(address, &state, "owner-a").await;
    send(&mut excess, &json!({"type":"subscribe","key":key("draft-a"),"mode":"view","presence_version":1})).await;
    let refused = next(&mut excess).await; unavailable(&refused);
    assert!(refused.get("peers").is_none());
    send(&mut other, &json!({"type":"resync","key":key("draft-b")})).await;
    let other_current = tokio::time::timeout(Duration::from_secs(8), async {
        loop { let frame = presence_next(&mut other).await; if frame["type"] == "presence_roster" { break frame; } }
    }).await.unwrap();
    assert_roster(&other_current, "draft-b", &other_id);
    assert_eq!(other_current["peers"].as_array().unwrap().len(), 1);
    send(&mut observer, &json!({"type":"resync","key":key("draft-a")})).await;
    let current = tokio::time::timeout(Duration::from_secs(8), async {
        loop { let frame = presence_next(&mut observer).await; if frame["type"] == "snapshot" { break frame; } }
    }).await.unwrap();
    assert_eq!(current["sequence"], original["sequence"]);
    assert_eq!(current["draft"]["source_digest"], original["draft"]["source_digest"]);
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "current_thread")]
async fn roster_actual_reviewed_publication_consumes_draft_and_retires_all_participants() {
    let _root = crate::storage::fixture_root::FixtureRoot::private();
    let state = fixture().await;
    let relative = "spaces/space-a/docs/target-a.md";
    let path = crate::storage::data_dir().join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"PUBLISHED ORIGINAL\n").unwrap();
    sqlx::query("UPDATE documents SET content_path=$1,updated_at='published',published_at='published' WHERE id='target-a'")
        .bind(relative).execute(&state.db).await.unwrap();
    let (address, _server) = server(&state).await;
    let (mut owner, owner_id) = connect(address, &state, "owner-a").await;
    subscribe_roster(&mut owner, "draft-a", "view", &owner_id).await;
    let (mut editor, editor_id) = connect(address, &state, "editor-a").await;
    subscribe_roster(&mut editor, "draft-a", "edit", &editor_id).await;
    roster_until(&mut owner, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().len() == 2).await;
    let app = crate::api_routes().with_state(state.clone());
    let proof = app.clone().oneshot(Request::builder().method("GET")
        .uri("/spaces/space-a/documents/target-a/drafts/draft-a/publish-source")
        .header("Authorization", format!("Bearer {}", token(&state, "owner-a")))
        .body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(proof.status(), StatusCode::OK);
    let proof: Value = serde_json::from_slice(&to_bytes(proof.into_body(), 6 * 1024 * 1024).await.unwrap()).unwrap();
    let operation = uuid::Uuid::new_v4().to_string();
    let response = app.oneshot(Request::builder().method("POST")
        .uri("/spaces/space-a/documents/target-a/drafts/draft-a/publish")
        .header("Authorization", format!("Bearer {}", token(&state, "owner-a")))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"schema_version":1,"operation_id":operation,
            "draft_source_digest":proof["draft_source_digest"],"page_source_digest":proof["page_source_digest"]}).to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let response = String::from_utf8(to_bytes(response.into_body(), 6 * 1024 * 1024).await.unwrap().to_vec()).unwrap();
    assert_eq!(status, StatusCode::OK, "Actual publication failed: {response}");
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM document_drafts WHERE id='draft-a'").fetch_one(&state.db).await.unwrap(), 0);
    assert_eq!(std::fs::read(&path).unwrap(), b"PUBLISHED ORIGINAL\n", "Publication retains the old canonical bytes");
    let current_path: String = sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'").fetch_one(&state.db).await.unwrap();
    assert_ne!(current_path, relative);
    assert_eq!(current_path, format!("spaces/space-a/docs/revisions/{operation}.md"));
    assert!(std::path::Path::new(&current_path).components().all(|part| matches!(part, std::path::Component::Normal(_))));
    let current_bytes = std::fs::read(crate::storage::data_dir().join(&current_path)).unwrap();
    assert!(std::str::from_utf8(&current_bytes).unwrap().contains("A😀BC"));
    let acknowledgement: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(acknowledgement["schema_version"], 1);
    assert_eq!(acknowledgement["operation_id"], operation);
    assert_eq!(acknowledgement["draft_id"], "draft-a");
    assert_eq!(acknowledgement["document_id"], "target-a");
    assert_eq!(acknowledgement["space_id"], "space-a");
    assert_eq!(acknowledgement["draft_source_digest"], proof["draft_source_digest"]);
    assert_eq!(acknowledgement["page_source_digest"], proof["page_source_digest"]);
    assert_eq!(acknowledgement["disposition"], "applied");
    let (recorded_ack, recorded_request, recorded_draft, recorded_page): (String, String, String, String) =
        sqlx::query_as("SELECT response_json,request_json,draft_json,result_page_json FROM draft_publication_receipts WHERE operation_id=$1 AND actor_id='owner-a' AND draft_id='draft-a' AND document_id='target-a' AND space_id='space-a'")
            .bind(&operation).fetch_one(&state.db).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&recorded_ack).unwrap(), acknowledgement);
    let recorded_request: Value = serde_json::from_str(&recorded_request).unwrap();
    assert_eq!(recorded_request["operation_id"], operation);
    assert_eq!(recorded_request["draft_source_digest"], proof["draft_source_digest"]);
    assert_eq!(recorded_request["page_source_digest"], proof["page_source_digest"]);
    assert_eq!(serde_json::from_str::<Value>(&recorded_draft).unwrap()["content"], "A😀BC");
    assert_eq!(serde_json::from_str::<Value>(&recorded_page).unwrap()["content_path"], current_path);
    let storage_receipt: (String, String, String) = sqlx::query_as("SELECT actor_id,source_path,content_path FROM publication_receipts WHERE op_id=$1")
        .bind(&operation).fetch_one(&state.db).await.unwrap();
    assert_eq!(storage_receipt, ("owner-a".to_owned(), relative.to_owned(), current_path));
    let captured: Vec<u8> = sqlx::query_scalar("SELECT raw_markdown FROM document_publication_snapshots WHERE kind='publication' AND publication_operation_id=$1")
        .bind(&operation).fetch_one(&state.db).await.unwrap();
    assert_eq!(captured, current_bytes, "Immutable history records the actual new revision bytes");
    for socket in [&mut owner, &mut editor] {
        let retired = tokio::time::timeout(Duration::from_secs(8), async {
            loop { let frame = presence_next(socket).await; if frame["type"] == "unavailable" || frame["type"] == "closed" { break frame; } }
        }).await.unwrap();
        assert!(retired.get("draft").is_none()); assert!(retired.get("peers").is_none());
    }
    // Other private draft identity/source is preserved by real publication.
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT content FROM document_drafts WHERE id='draft-b'").fetch_one(&state.db).await.unwrap(), "SECOND PRIVATE DRAFT");
}


#[tokio::test]
async fn roster_protocol_ping_pong_preserves_legacy_json_and_source() {
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut socket, _) = connect(address, &state, "owner-a").await;
    let before = subscribe(&mut socket, "draft-a", "view").await;
    presence_control(&mut socket, 9, b"actual-control-probe").await;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let (opcode, bytes) = presence_wire(&mut socket).await;
            match opcode {
                10 => { assert_eq!(bytes, b"actual-control-probe"); break; }
                9 => presence_control(&mut socket, 10, &bytes).await,
                _ => panic!("Control probe unexpectedly emitted data or retired: {opcode}"),
            }
        }
    }).await.unwrap();
    send(&mut socket, &json!({"type":"resync","key":key("draft-a")})).await;
    let after = next(&mut socket).await;
    assert_eq!(after["type"], "snapshot"); assert_eq!(after["draft"], before["draft"]);
    assert_eq!(after["sequence"], before["sequence"]);
    quiet(&mut socket).await;
}

async fn presence_pump(mut socket: TcpStream, frames: Option<tokio::sync::mpsc::UnboundedSender<Value>>,
    answer: std::sync::Arc<std::sync::atomic::AtomicBool>, pongs: std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    loop {
        let (opcode, bytes) = presence_wire(&mut socket).await;
        match opcode {
            1 => { if let Some(frames) = &frames { let _ = frames.send(serde_json::from_slice(&bytes).unwrap()); } }
            9 => if answer.load(std::sync::atomic::Ordering::SeqCst) {
                presence_control(&mut socket, 10, &bytes).await;
                pongs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            },
            10 => (),
            8 => break,
            _ => panic!("Unexpected keepalive opcode {opcode}"),
        }
    }
}
#[tokio::test]
async fn roster_actual_pong_survives_ttl_then_silent_open_connection_expires() {
    use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};
    let state = fixture().await;
    *state.ws_rooms.drafts.presence_timing.write().await = Some((Duration::from_millis(100), Duration::from_millis(800)));
    let (address, _server) = server(&state).await;
    let (mut owner, owner_id) = connect(address, &state, "owner-a").await;
    subscribe_roster(&mut owner, "draft-a", "view", &owner_id).await;
    let (mut peer, peer_id) = connect(address, &state, "editor-a").await;
    subscribe_roster(&mut peer, "draft-a", "edit", &peer_id).await;
    roster_until(&mut owner, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().len() == 2).await;
    let (sender, mut frames) = tokio::sync::mpsc::unbounded_channel();
    let owner_answers = Arc::new(AtomicBool::new(true));
    let owner_pongs = Arc::new(AtomicUsize::new(0));
    let owner_task = tokio::spawn(presence_pump(owner, Some(sender), owner_answers, owner_pongs.clone()));
    let peer_answers = Arc::new(AtomicBool::new(true));
    let peer_pongs = Arc::new(AtomicUsize::new(0));
    let mut peer_task = tokio::spawn(presence_pump(peer, None, peer_answers.clone(), peer_pongs.clone()));
    let started = std::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let frame = frames.recv().await.unwrap();
            assert_ne!(frame["type"], "unavailable");
            if frame["type"] == "presence_roster" {
                assert_roster(&frame, "draft-a", &owner_id);
                assert!(frame["peers"].as_array().unwrap().iter().any(|p| p["connection_id"] == peer_id));
                if started.elapsed() > Duration::from_millis(1200) && peer_pongs.load(Ordering::SeqCst) >= 2 { break; }
            }
        }
    }).await.expect("Actual Ping/Pong must keep peer admitted beyond its TTL");
    assert!(owner_pongs.load(Ordering::SeqCst) >= 2);
    // Deliberately keep TCP open and read server frames, but stop replying to
    // Ping. This is different from the normal RFC-compatible helper above.
    peer_answers.store(false, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let frame = frames.recv().await.unwrap();
            assert_ne!(frame["type"], "unavailable");
            if frame["type"] == "presence_roster" {
                assert_roster(&frame, "draft-a", &owner_id);
                if frame["peers"].as_array().unwrap().iter().all(|p| p["connection_id"] != peer_id) { break; }
            }
        }
    }).await.expect("Open but silent peer must expire independently of body/source activity");
    tokio::time::timeout(Duration::from_secs(3), &mut peer_task).await.unwrap().unwrap();
    owner_task.abort(); let _ = owner_task.await;
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT content FROM document_drafts WHERE id='draft-a'").fetch_one(&state.db).await.unwrap(), "A😀BC");
}

#[tokio::test]
async fn roster_final_authority_and_name_pass_rechecks_after_projection_wait() {
    for change in ["revoke", "name"] {
        let state = fixture().await; let (address, _server) = server(&state).await;
        let (mut owner, owner_id) = connect(address, &state, "owner-a").await;
        subscribe_roster(&mut owner, "draft-a", "view", &owner_id).await;
        let (mut peer, peer_id) = connect(address, &state, "editor-a").await;
        subscribe_roster(&mut peer, "draft-a", "edit", &peer_id).await;
        roster_until(&mut owner, "draft-a", &owner_id, |r| r["peers"].as_array().unwrap().len() == 2).await;
        let gate = std::sync::Arc::new(super::ReceivePause { connection_id: owner_id.clone(), reached: tokio::sync::Notify::new(), release: tokio::sync::Notify::new() });
        *state.ws_rooms.drafts.roster_pause.write().await = Some(gate.clone());
        send(&mut peer, &json!({"type":"presence","key":key("draft-a"),"state":"idle"})).await;
        tokio::time::timeout(Duration::from_secs(8), gate.reached.notified()).await.expect("Actual roster projection reached final boundary");
        if change == "revoke" {
            sqlx::query("DELETE FROM document_draft_access WHERE draft_id='draft-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
        } else {
            sqlx::query("UPDATE users SET display_name='Fresh Final Name' WHERE id='editor-a'").execute(&state.db).await.unwrap();
        }
        gate.release.notify_one();
        let frame = tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let frame = presence_next(&mut owner).await;
                if frame["type"] != "presence_roster" { continue; }
                // Previously queued join rosters contain active, not this new
                // idle transition. Do not confuse them with the paused send.
                let selected = frame["peers"].as_array().unwrap().iter().find(|p| p["connection_id"] == peer_id);
                if selected.is_none_or(|peer| peer["state"] == "idle") { break frame; }
            }
        }).await.unwrap();
        assert_roster(&frame, "draft-a", &owner_id);
        if change == "revoke" { assert!(frame["peers"].as_array().unwrap().iter().all(|p| p["connection_id"] != peer_id)); }
        else { assert_eq!(roster_peer(&frame, &peer_id)["display_name"], "Fresh Final Name"); }
    }
}


#[tokio::test]
async fn roster_presence_only_pressure_cannot_lag_negotiated_source_channel_or_force_snapshot() {
    use super::{DraftKey, EventKind, PresenceState, ReceivePause, ROOM_CAPACITY};
    use std::sync::Arc;
    let state = fixture().await; let (address, _server) = server(&state).await;
    let (mut legacy, _) = connect(address, &state, "owner-a").await;
    subscribe(&mut legacy, "draft-a", "view").await;
    let (mut sender, sender_id) = connect(address, &state, "editor-a").await;
    subscribe(&mut sender, "draft-a", "edit").await;
    let (mut opted, opted_id) = connect(address, &state, "owner-a").await;
    let gate = Arc::new(ReceivePause { connection_id: opted_id.clone(), reached: tokio::sync::Notify::new(), release: tokio::sync::Notify::new() });
    *state.ws_rooms.drafts.receive_pause.write().await = Some(gate.clone());
    let (before, _) = subscribe_roster(&mut opted, "draft-a", "view", &opted_id).await;
    tokio::time::timeout(Duration::from_secs(8), gate.reached.notified()).await
        .expect("Actual negotiated socket is paused before reading its event channel");
    let typed: DraftKey = serde_json::from_value(key("draft-a")).unwrap();
    let room = state.ws_rooms.drafts.rooms.read().await.get(&typed).unwrap().clone();
    let mut tap = room.subscribe().unwrap();
    // Capture the real authenticated sender and its original liveness handle,
    // rather than forging peer credentials for this fanout-pressure fixture.
    send(&mut sender, &json!({"type":"presence","key":key("draft-a"),"state":"idle"})).await;
    let captured = tokio::time::timeout(Duration::from_secs(8), tap.recv()).await.unwrap().unwrap();
    assert!(matches!(captured.kind, EventKind::Presence(_)));
    let peer = captured.peer.unwrap();
    assert_eq!(peer.connection_id, sender_id);
    let original_presence = next(&mut legacy).await;
    assert_eq!(original_presence["type"], "presence");
    assert_eq!(original_presence["connection_id"], sender_id);
    assert_eq!(original_presence["sequence"], captured.sequence);
    let source_sequence = before["sequence"].as_u64().unwrap();
    assert_eq!(room.draft_sequence().unwrap(), source_sequence);
    let burst = 2 * ROOM_CAPACITY + 1;
    // Drain the real legacy socket after each emitted event, so it does not
    // itself legitimately lag. The opted-in socket remains paused throughout.
    for index in 0..burst {
        let state = if index % 2 == 0 { PresenceState::Active } else { PresenceState::Idle };
        room.emit(EventKind::Presence(state), Some(peer.clone())).unwrap();
        let frame = next(&mut legacy).await;
        assert_eq!(frame["type"], "presence");
        assert_eq!(frame["version"], 1); assert_eq!(frame["key"], key("draft-a"));
        assert_eq!(frame["connection_id"], sender_id); assert_eq!(frame["user_id"], "editor-a");
        assert_eq!(frame["state"], if index % 2 == 0 { "active" } else { "idle" });
        assert_eq!(frame["sequence"].as_u64().unwrap(), captured.sequence + index as u64 + 1);
        assert!(frame.get("peers").is_none());
    }
    assert_eq!(room.sequence().unwrap(), captured.sequence + burst as u64);
    assert_eq!(room.draft_sequence().unwrap(), source_sequence,
        "Presence-only pressure must not consume negotiated source order");
    // An actual authorized cursor is an ordered source-channel barrier after
    // the pressure. It legitimately advances that channel once; roster traffic
    // must not cause a resync gap before this barrier becomes visible.
    send(&mut sender, &json!({"type":"cursor","key":key("draft-a"),
        "source_digest":before["draft"]["source_digest"],"anchor":0,"head":0})).await;
    let legacy_cursor = next(&mut legacy).await;
    assert_eq!(legacy_cursor["type"], "cursor");
    assert_eq!(legacy_cursor["sequence"].as_u64().unwrap(), captured.sequence + burst as u64 + 1);
    assert_eq!(room.draft_sequence().unwrap(), source_sequence + 1);
    gate.release.notify_one();
    let cursor = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let frame = presence_next(&mut opted).await;
            match frame["type"].as_str() {
                Some("presence_roster") => assert_roster(&frame, "draft-a", &opted_id),
                Some("cursor") => break frame,
                _ => panic!("Presence-only pressure caused an unexpected source/lifecycle frame: {frame}"),
            }
        }
    }).await.expect("Negotiated socket must reach actual cursor barrier without forced resync");
    assert_eq!(cursor["sequence"].as_u64().unwrap(), source_sequence + 1);
    assert_eq!(cursor["connection_id"], sender_id);
    assert_eq!(cursor["source_digest"], before["draft"]["source_digest"]);
    assert_eq!(cursor["anchor"], 0); assert_eq!(cursor["head"], 0);
    assert!(cursor.get("operation_id").is_none());
    send(&mut opted, &json!({"type":"resync","key":key("draft-a")})).await;
    let current = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let frame = presence_next(&mut opted).await;
            match frame["type"].as_str() {
                Some("presence_roster") => assert_roster(&frame, "draft-a", &opted_id),
                Some("snapshot") => break frame,
                _ => panic!("Only the explicitly requested snapshot is allowed after the barrier: {frame}"),
            }
        }
    }).await.unwrap();
    assert_eq!(current["sequence"].as_u64().unwrap(), source_sequence + 1);
    assert_eq!(current["draft"], before["draft"]);
    assert_eq!(current["draft"]["content"], "A😀BC");
    assert!(current.get("operation_id").is_none());
}
