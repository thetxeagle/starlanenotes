//! Private broad-reference fence substrate, not an erasure authorization grant.
use sqlx::AnyConnection;
use super::{OwnedErasure,Step,census};
use crate::auth::routes::AppError;
pub(super) async fn lock_every_space(conn:&mut AnyConnection)->Result<Vec<String>,AppError> {
 crate::spaces::mutation_guards::lock_audience_for_mutation(conn).await?;
 let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM spaces ORDER BY id LIMIT 513").fetch_all(&mut *conn).await.map_err(AppError::internal)?;
 if ids.len()>512{return Err(AppError::Conflict("Global erasure writer fence exceeds supported bound".into()))}
 let borrowed:Vec<&str>=ids.iter().map(String::as_str).collect();
 crate::spaces::mutation_guards::lock_spaces_for_mutation(conn,&borrowed).await?;
 // Audience fence excludes adopted new-space creation while every existing scoped
 // guard excludes adopted reference writers. Coverage itself is a release gate.
 let fresh:Vec<String>=sqlx::query_scalar("SELECT id FROM spaces ORDER BY id LIMIT 513").fetch_all(conn).await.map_err(AppError::internal)?;
 if fresh!=ids{return Err(AppError::Conflict("Global erasure writer scope changed".into()))}Ok(ids)
}

// Private authority proof, NOT reference/custody permission and never a durable grant.
struct FencedTransaction{tx:sqlx::Transaction<'static,sqlx::Any>}
struct CurrentHuman<'a>{auth:&'a crate::auth::middleware::AuthUser,session_expiry:chrono::DateTime<chrono::FixedOffset>,
    _transaction:std::marker::PhantomData<&'a mut FencedTransaction>}
impl FencedTransaction{
 async fn begin(pool:&sqlx::AnyPool)->Result<Self,AppError>{let mut tx=pool.begin().await.map_err(AppError::internal)?;lock_every_space(&mut tx).await?;Ok(Self{tx})}
 async fn original_human<'a>(&'a mut self,auth:&'a crate::auth::middleware::AuthUser,actor:&str,space:&str)->Result<CurrentHuman<'a>,AppError>{
  use crate::spaces::access;
  if auth.api_key_scopes.is_some()||auth.claims.sub!=actor{return Err(AppError::Unauthorized("Original human cleanup credential required".into()))}
  // The audience/space guards do not cover logout or session revocation.
  // Hold the original user then original session rows across the final native
  // callback so independent DELETE/UPDATE cannot invalidate a cached proof.
  let user=sqlx::query("UPDATE users SET id=id WHERE id=$1").bind(actor).execute(&mut *self.tx).await.map_err(AppError::internal)?;
  let session=sqlx::query("UPDATE sessions SET id=id WHERE id=$1 AND user_id=$2").bind(&auth.claims.sid).bind(actor).execute(&mut *self.tx).await.map_err(AppError::internal)?;
  if user.rows_affected()!=1||session.rows_affected()!=1{return Err(AppError::Unauthorized("Original human rows unavailable".into()))}
  let current=access::resolve_space_access(&mut self.tx,access::principal_from_auth(auth)?,space,access::Purpose::Routine).await?;
  if !current.allows(access::Permission::DeleteContent){return Err(AppError::NotFound("Cleanup authority unavailable".into()))}
  let expiry:Option<String>=sqlx::query_scalar("SELECT expires_at FROM sessions WHERE id=$1 AND user_id=$2").bind(&auth.claims.sid).bind(actor).fetch_optional(&mut *self.tx).await.map_err(AppError::internal)?;
  let expiry=expiry.as_deref().and_then(|s|chrono::DateTime::parse_from_rfc3339(s).ok()).ok_or_else(||AppError::Unauthorized("Original session unavailable".into()))?;
  // Reprove original account/session after all blocking policy/expiry reads.
  if access::require_current_human_preference_write(&mut self.tx,auth).await?!=actor{return Err(AppError::Unauthorized("Original actor changed".into()))}
  let proof=CurrentHuman{auth,session_expiry:expiry,_transaction:std::marker::PhantomData};proof.check_now()?;Ok(proof)
 }
}
struct OneOwnedStep<'a>{human:CurrentHuman<'a>,owner:OwnedErasure,census:census::FamilyCensus,reference:references::ReferenceProof,step:Step}
impl FencedTransaction{
 async fn prepare_one<'a>(&'a mut self,auth:&'a crate::auth::middleware::AuthUser,operation:&str,receipt:&crate::storage::attachment_upload::Receipt)->Result<OneOwnedStep<'a>,AppError>{
  self.original_human(auth,&auth.claims.sub,&receipt.intent.request.space_id).await?.check_now()?;
  let reference=references::no_protected_references(self,auth,operation,receipt).await?;
  // Refresh original human after all reference and consistency waits, before a
  // durable native review intent. This remains one original deletion operation.
  self.original_human(auth,&reference.plan.actor_id,&reference.plan.space_id).await?.check_now()?;
  let hash=crate::storage::attachment_upload::digest(reference.plan_raw.as_bytes());
  let owner=OwnedErasure::open(&crate::storage::data_dir(),receipt,operation,&hash,&reference.plan.actor_id).map_err(AppError::internal)?;
  let reservation:(i64,String,String,String,Option<String>,Option<i64>)=sqlx::query_as("SELECT maximum_bytes,request_sha256,admission_claim_sha256,state,committed_asset_id,committed_bytes FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&receipt.intent.attempt_id).fetch_one(&mut *self.tx).await.map_err(AppError::internal)?;
  if reservation.0!=owner.lease.maximum_bytes() as i64||reservation.1!=owner.lease.producer_sha256()||reservation.2!=owner.lease.claim_sha256()||reservation.3!="committed"||reservation.4.as_deref()!=Some(receipt.intent.asset_id.as_str())||reservation.5!=Some(receipt.intent.byte_length){return Err(AppError::Conflict("Original committed admission differs".into()))}
  let qualified=crate::storage::restoration::qualified_erasure_source_in(&crate::storage::data_dir(),&mut self.tx).await.map_err(AppError::internal)?;let census=census::FamilyCensus::capture_qualified(&owner,reference.paths.clone(),reference.finalized_journals.clone(),qualified).map_err(AppError::internal)?;let step=owner.next().map_err(AppError::internal)?;
  census.fence_in(&mut self.tx).await.map_err(AppError::internal)?;
  let human=self.original_human(auth,&auth.claims.sub,&receipt.intent.request.space_id).await?;
  Ok(OneOwnedStep{human,owner,census,reference,step})
 }
}
impl FencedTransaction{
 // Private completed-only SQL binding. This consumes its transaction, preserving
 // native/family fences across insert/re-read/current-human proof and commit.
 async fn finalize_existing(mut self,auth:&crate::auth::middleware::AuthUser,operation:&str,receipt:&crate::storage::attachment_upload::Receipt)->Result<(),AppError>{
  use sqlx::Row;
  let one=self.prepare_one(auth,operation,receipt).await?;if one.step!=Step::Complete{return Err(AppError::Conflict("Owned paths are not all erased".into()))}
  one.human.check_now()?;
  let paths=one.owner.completed_path_material(&one.census).map_err(AppError::internal)?;
  let OneOwnedStep{human,owner,census,reference,step:_}=one;drop(human);
  let intent=String::from_utf8(owner.raw.clone()).map_err(AppError::internal)?;if intent.len()>16384{return Err(AppError::Conflict("Native completed evidence exceeds SQL bound".into()))}
  let completed=chrono::Utc::now().to_rfc3339();let intent_sha=crate::storage::attachment_upload::digest(intent.as_bytes());let paths_sha=crate::storage::attachment_upload::digest(paths.as_bytes());
  let fields=[("deletion_operation_id",operation.to_owned()),("asset_id",receipt.intent.asset_id.clone()),("actor_id",reference.plan.actor_id.clone()),("space_id",reference.plan.space_id.clone()),("document_id",reference.plan.document_id.clone()),("producer_kind","original_direct_v1".into()),("original_attempt_id",receipt.intent.attempt_id.clone()),("plan_sha256",owner.intent.deletion_plan_sha256.clone()),("original_receipt_sha256",owner.intent.receipt_sha256.clone()),("admission_claim_sha256",owner.lease.claim_sha256().to_owned()),("native_intent_json",intent.clone()),("native_intent_sha256",intent_sha.clone()),("native_tombstone_json",intent), ("native_tombstone_sha256",intent_sha),("owned_path_set_json",paths), ("owned_path_set_sha256",paths_sha),("disposition","owned_paths_erased_v1".into()),("completed_at",completed)];
  let columns=fields.iter().map(|(name,_)|*name).collect::<Vec<_>>().join(",");let parameters=(1..=fields.len()).map(|n|format!("${n}")).collect::<Vec<_>>().join(",");
  let sql=format!("INSERT INTO attachment_erasure_dispositions({columns}) VALUES({parameters}) ON CONFLICT(asset_id) DO NOTHING");let mut insert=sqlx::query(&sql);for(_,value)in &fields{insert=insert.bind(value);}insert.execute(&mut *self.tx).await.map_err(AppError::internal)?;
  let row=sqlx::query("SELECT * FROM attachment_erasure_dispositions WHERE asset_id=$1").bind(&receipt.intent.asset_id).fetch_one(&mut *self.tx).await.map_err(AppError::internal)?;
  for(name,expected)in &fields{let actual:String=row.try_get(*name).map_err(AppError::internal)?;match *name{
   "completed_at"=>{let time=chrono::DateTime::parse_from_rfc3339(&actual).map_err(AppError::internal)?;let original=chrono::DateTime::parse_from_rfc3339(&reference.plan.created_at).map_err(AppError::internal)?;if time<original||time>chrono::Utc::now(){return Err(AppError::Conflict("Completed disposition time differs".into()))}},
   "owned_path_set_json"=>{let mut old:serde_json::Value=serde_json::from_str(&actual).map_err(AppError::internal)?;let mut fresh:serde_json::Value=serde_json::from_str(expected).map_err(AppError::internal)?;let observed=old["family_observation_sha256"].as_str().ok_or_else(||AppError::Conflict("Completed census hash absent".into()))?;if observed.len()!=64||!observed.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err(AppError::Conflict("Completed census hash malformed".into()))}old.as_object_mut().ok_or_else(||AppError::Conflict("Completed material is not an object".into()))?.remove("family_observation_sha256");fresh.as_object_mut().unwrap().remove("family_observation_sha256");if old!=fresh{return Err(AppError::Conflict("Original completed path set differs".into()))}},
   "owned_path_set_sha256"=>{let raw:String=row.try_get("owned_path_set_json").map_err(AppError::internal)?;if actual!=crate::storage::attachment_upload::digest(raw.as_bytes()){return Err(AppError::Conflict("Original completed path set checksum differs".into()))}},
   _ if &actual!=expected=>return Err(AppError::Conflict("Original completed disposition differs".into())),_=>()}}
  owner.completed_path_material(&census).map_err(AppError::internal)?;
  self.original_human(auth,&reference.plan.actor_id,&reference.plan.space_id).await?.check_now()?;
  // Hashes/full native family work precede the last credential expiry check.
  owner.completed_path_material(&census).map_err(AppError::internal)?;
  self.original_human(auth,&reference.plan.actor_id,&reference.plan.space_id).await?.check_now()?;
  self.tx.commit().await.map_err(AppError::internal)?;drop(census);drop(owner);Ok(())
 }
}
impl OneOwnedStep<'_>{fn execute(self)->Result<bool,AppError>{
 if self.reference.receipt.intent.attempt_id!=self.owner.lease.attempt_id(){return Err(AppError::Conflict("Original cleanup proof changed".into()))}
 self.owner.apply_one_censused(self.step,&self.census,||self.human.check_now().map_err(|e|std::io::Error::other(format!("{e:?}")))).map_err(AppError::internal)?;
 Ok(self.step==Step::Complete)
}}
impl CurrentHuman<'_>{
 fn check_now(&self)->Result<(),AppError>{crate::spaces::access::principal_from_auth(self.auth)?;
  if self.session_expiry<=chrono::Utc::now(){return Err(AppError::Unauthorized("Original session expired before owned mutation".into()))}Ok(())}
}

#[cfg(test)]
mod tests {
 use super::*;use crate::storage::attachment_upload::admission::{WriterLease,Producer};use axum::{body::Body,extract::FromRequestParts,http::Request};use crate::{AppState,storage};use serde_json::{json,Value};use sqlx::any::AnyPoolOptions;
 #[tokio::test(flavor="current_thread")]
 #[ignore="requires private disposable dual-engine fixture and owns actual TCP"]
 async fn actual_foreign_space_draft_writer_waits_across_guarded_scan(){
 sqlx::any::install_default_drivers();let root=storage::data_dir();std::fs::create_dir_all(&root).unwrap();let pg=std::env::var("REPLAY_CAPACITY_TEST_PG_URL").ok();let schema=format!("erase_fence_{}",uuid::Uuid::new_v4().simple());
 let admin=if let Some(url)=&pg{let p=AnyPoolOptions::new().max_connections(2).connect(url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&p).await.unwrap();Some(p)}else{None};
 let db=if let Some(url)=&pg{let owned=schema.clone();AnyPoolOptions::new().max_connections(5).after_connect(move|conn,_|{let sql=format!("SET search_path TO {owned}");Box::pin(async move{sqlx::query(&sql).execute(conn).await?;Ok(())})}).connect(url).await.unwrap()}else{AnyPoolOptions::new().max_connections(5).after_connect(|conn,_|Box::pin(async move{sqlx::query("PRAGMA busy_timeout=10000").execute(&mut *conn).await?;sqlx::query("PRAGMA journal_mode=WAL").execute(conn).await?;Ok(())})).connect(&format!("sqlite://{}?mode=rwc",root.join("fixture.sqlite").display())).await.unwrap()};
 if pg.is_some(){sqlx::migrate!("./migrations-postgres").run(&db).await.unwrap()}else{sqlx::migrate!("./migrations").run(&db).await.unwrap()}
 for sql in ["INSERT INTO users(id,email,display_name,role) VALUES('owner','owner@test.invalid','Owner','member'),('viewer','viewer@test.invalid','Viewer','member')","INSERT INTO spaces(id,name,slug,created_by) VALUES('space','Outbox','outbox','owner')","INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner-member','space','owner','owner'),('viewer-member','space','viewer','viewer')","INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('owner-session','owner','owner-refresh','2999-01-01T00:00:00Z'),('viewer-session','viewer','viewer-refresh','2999-01-01T00:00:00Z')"]{sqlx::query(sql).execute(&db).await.unwrap();}
 for sql in ["INSERT INTO spaces(id,name,slug,created_by) VALUES('foreign-space','Foreign','foreign','owner')","INSERT INTO space_members(id,space_id,user_id,role) VALUES('foreign-owner','foreign-space','owner','owner')"]{sqlx::query(sql).execute(&db).await.unwrap();}
 let state=AppState{db,jwt_secret:"owned-erasure-fixture".into(),base_url:"http://localhost".into(),search_index:crate::search::SearchIndex::new(root.join("index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
 let token=crate::auth::jwt::create_token_pair_for_session("owner","owner@test.invalid","member",&state.jwt_secret,Some("owner-session"),30).unwrap().access_token;let viewer=crate::auth::jwt::create_token_pair_for_session("viewer","viewer@test.invalid","member",&state.jwt_secret,Some("viewer-session"),30).unwrap().access_token;
 sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES('status-admin','status-admin@test.invalid','Status admin','admin')").execute(&state.db).await.unwrap();
 sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('status-session','status-admin','status-refresh','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
 let status_token=crate::auth::jwt::create_token_pair_for_session("status-admin","status-admin@test.invalid","admin",&state.jwt_secret,Some("status-session"),30).unwrap().access_token;
 let app=crate::spaces::documents::router().merge(crate::spaces::drafts::router()).merge(crate::spaces::attachments::router()).merge(crate::storage::attachment_versions::router()).merge(crate::auth::routes::router()).merge(crate::audit::router()).with_state(state.clone());let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap()});let base=format!("http://{address}");let client=reqwest::Client::new();std::env::remove_var("ATTACHMENT_STORAGE_QUOTA_BYTES");std::env::set_var("MAX_UPLOAD_SIZE_MB","1");

 let request=Request::builder().uri("/private-proof").header("Authorization",format!("Bearer {token}")).body(Body::empty()).unwrap();let(mut parts,_)=request.into_parts();let owner_auth=crate::auth::middleware::AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
 let request=Request::builder().uri("/private-proof").header("Authorization",format!("Bearer {viewer}")).body(Body::empty()).unwrap();let(mut parts,_)=request.into_parts();let viewer_auth=crate::auth::middleware::AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();fenced.original_human(&owner_auth,"owner","space").await.unwrap().check_now().unwrap();
  assert!(fenced.original_human(&owner_auth,"foreign-actor","space").await.is_err());assert!(fenced.original_human(&viewer_auth,"viewer","space").await.is_err());fenced.tx.rollback().await.unwrap();}
 // Original session expires while no topology/session writer can change the held
 // transaction; time itself must be checked again at the native mutation boundary.
 sqlx::query("UPDATE sessions SET expires_at=$1 WHERE id='owner-session'").bind((chrono::Utc::now()+chrono::Duration::milliseconds(300)).to_rfc3339()).execute(&state.db).await.unwrap();
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();let proof=fenced.original_human(&owner_auth,"owner","space").await.unwrap();tokio::time::sleep(std::time::Duration::from_millis(350)).await;assert!(proof.check_now().is_err());drop(proof);fenced.tx.rollback().await.unwrap();}
 sqlx::query("UPDATE sessions SET expires_at='2999-01-01T00:00:00Z' WHERE id='owner-session'").execute(&state.db).await.unwrap();
 sqlx::query("UPDATE users SET is_active=0 WHERE id='owner'").execute(&state.db).await.unwrap();
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();assert!(fenced.original_human(&owner_auth,"owner","space").await.is_err());fenced.tx.rollback().await.unwrap();}
 sqlx::query("UPDATE users SET is_active=1 WHERE id='owner'").execute(&state.db).await.unwrap();
 // A real logout runs on a separate HTTP worker/connection. It cannot delete
 // the original session while the private current-human transaction holds it.
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();let proof=fenced.original_human(&owner_auth,"owner","space").await.unwrap();
  let client2=client.clone();let base2=base.clone();let token2=token.clone();let logout=tokio::spawn(async move{client2.post(format!("{base2}/auth/logout")).bearer_auth(token2).send().await.unwrap()});
  tokio::time::sleep(std::time::Duration::from_millis(150)).await;assert!(!logout.is_finished(),"Original-session logout crossed held current-human row fence");proof.check_now().unwrap();drop(proof);fenced.tx.rollback().await.unwrap();assert_eq!(logout.await.unwrap().status(),204);}
 // Logout-before authority admission must refuse the old extracted credential,
 // even though its original signed JWT has not yet expired.
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();assert!(fenced.original_human(&owner_auth,"owner","space").await.is_err());fenced.tx.rollback().await.unwrap();}
 sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('owner-session','owner','owner-refresh','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
 let response=client.post(format!("{base}/spaces/space/documents")).bearer_auth(&token).json(&json!({"source_id":"winner","title":"Winner","content":"ORIGINAL WINNER","property_values":[]})).send().await.unwrap();let status=response.status();let page:Value=response.json().await.unwrap();assert_eq!(status,201,"{page}");
 let response=client.post(format!("{base}/spaces/space/documents/winner/drafts/{}/publish",page["draft_id"].as_str().unwrap())).bearer_auth(&token).json(&json!({})).send().await.unwrap();assert!(response.status().is_success());
 let upload_operation=uuid::Uuid::new_v4().to_string();let boundary="original-erasure-winner-boundary";
 let body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Original.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nORIGINAL NATIVE WINNER\r\n--{boundary}--\r\n");
 let response=client.post(format!("{base}/spaces/space/documents/winner/attachments")).bearer_auth(&token).header("x-starlane-upload-operation",&upload_operation).header("content-type",format!("multipart/form-data; boundary={boundary}")).body(body).send().await.unwrap();let status=response.status();let uploaded:Value=response.json().await.unwrap();assert_eq!(status,201,"{uploaded}");let asset=uploaded["id"].as_str().unwrap().to_string();
 let mut conn=state.db.acquire().await.unwrap();let original=crate::storage::attachment_upload::receipt(&mut conn,&upload_operation).await.unwrap().unwrap();drop(conn);
 let raw_before:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&upload_operation).fetch_one(&state.db).await.unwrap();
 assert!(crate::storage::attachment_upload::admission::dispose_direct(&state.db,&root,&original.intent.attempt_id).await.is_err(),"Reserved disposer accepted real production committed winner");
 crate::storage::attachment_upload::verify(&root,&original,false).unwrap();
 let raw_after:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&upload_operation).fetch_one(&state.db).await.unwrap();assert_eq!(raw_after,raw_before);
 let reservation:String=sqlx::query_scalar("SELECT state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&original.intent.attempt_id).fetch_one(&state.db).await.unwrap();assert_eq!(reservation,"committed");
 assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_version_records WHERE version_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap(),1);
 let selected=client.get(format!("{base}/spaces/space/documents/winner/attachment-versions/{asset}")).bearer_auth(&token).send().await.unwrap();assert_eq!(selected.status(),200);
 let response=client.post(format!("{base}/spaces/foreign-space/documents")).bearer_auth(&token).json(&json!({"source_id":"foreign-reference","title":"Foreign","content":"","property_values":[]})).send().await.unwrap();
 let status=response.status();let page:Value=response.json().await.unwrap();assert_eq!(status,201,"{page}");let draft=page["draft_id"].as_str().unwrap();
 let endpoint=format!("{base}/spaces/foreign-space/documents/foreign-reference/drafts/{draft}");
 let current:Value=client.get(&endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();
 let protected=format!("[file:Original.bin](starlane-attachment://{asset})");
 let mut held=state.db.begin().await.unwrap();let scopes=lock_every_space(&mut held).await.unwrap();assert_eq!(scopes,vec!["foreign-space","space"]);
 let before:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE id=$1").bind(draft).fetch_one(&mut *held).await.unwrap();assert!(!before.contains(&asset));
 let client2=client.clone();let endpoint2=endpoint.clone();let token2=token.clone();let content2=protected.clone();
 let writer=tokio::spawn(async move{client2.put(endpoint2).bearer_auth(token2).json(&json!({"source_digest":current["source_digest"],"content":content2})).send().await.unwrap()});
 tokio::time::sleep(std::time::Duration::from_millis(150)).await;assert!(!writer.is_finished(),"Foreign-space draft writer crossed held global fence");
 let during:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE id=$1").bind(draft).fetch_one(&mut *held).await.unwrap();assert_eq!(during,before);
 // This proves the actual writer exclusion seam only. No physical mutation or
 // current-human erasure grant is implied by this generated possible reference.
 held.rollback().await.unwrap();let response=writer.await.unwrap();let status=response.status();let result:Value=response.json().await.unwrap();assert_eq!(status,200,"{result}");
 let after:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE id=$1").bind(draft).fetch_one(&state.db).await.unwrap();assert_eq!(after,protected);
 // Publication is a distinct real writer: it must also remain behind the
 // same global fence, rather than infer coverage from successful draft saving.
 let mut held=state.db.begin().await.unwrap();lock_every_space(&mut held).await.unwrap();
 let client2=client.clone();let endpoint2=format!("{endpoint}/publish");let token2=token.clone();
 let publisher=tokio::spawn(async move{client2.post(endpoint2).bearer_auth(token2).json(&json!({})).send().await.unwrap()});
 tokio::time::sleep(std::time::Duration::from_millis(150)).await;assert!(!publisher.is_finished(),"Foreign-space publication crossed held global fence");
 held.rollback().await.unwrap();let response=publisher.await.unwrap();let status=response.status();let result:Value=response.json().await.unwrap();assert_eq!(status,200,"{result}");
 let deletion=uuid::Uuid::new_v4().to_string();
 assert!(client.delete(format!("{base}/spaces/space/documents/winner")).bearer_auth(&token).send().await.unwrap().status().is_success());
 let witness:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id='winner'").fetch_one(&state.db).await.unwrap();
 let response=client.delete(format!("{base}/spaces/space/documents/winner/trash")).bearer_auth(&token).header("x-starlane-delete-operation",&deletion).json(&json!({"original_target":{"created_at":witness.0,"deleted_at":witness.1}})).send().await.unwrap();assert_eq!(response.status(),204);
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();assert!(fenced.prepare_one(&owner_auth,&deletion,&original).await.is_err(),"Protected foreign published pin allowed erasure");fenced.tx.rollback().await.unwrap();}
 assert!(root.join(&original.intent.storage_path).exists());
 assert!(client.delete(format!("{base}/spaces/foreign-space/documents/foreign-reference")).bearer_auth(&token).send().await.unwrap().status().is_success());
 let foreign_witness:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id='foreign-reference'").fetch_one(&state.db).await.unwrap();
 assert_eq!(client.delete(format!("{base}/spaces/foreign-space/documents/foreign-reference/trash")).bearer_auth(&token).header("x-starlane-delete-operation",uuid::Uuid::new_v4().to_string()).json(&json!({"original_target":{"created_at":foreign_witness.0,"deleted_at":foreign_witness.1}})).send().await.unwrap().status(),204);
 // Legacy format1 custody cannot stand in for the format2 original incarnation/trash witness.
 let plan_original:String=sqlx::query_scalar("SELECT plan_json FROM attachment_erasure_outbox WHERE operation_id=$1").bind(&deletion).fetch_one(&state.db).await.unwrap();
 let mut legacy:Value=serde_json::from_str(&plan_original).unwrap();legacy["format_version"]=json!(1);legacy.as_object_mut().unwrap().remove("original_target");let legacy_raw=serde_json::to_string(&legacy).unwrap();
 sqlx::query("UPDATE attachment_erasure_outbox SET plan_json=$1,plan_sha256=$2 WHERE operation_id=$3").bind(&legacy_raw).bind(crate::storage::attachment_upload::digest(legacy_raw.as_bytes())).bind(&deletion).execute(&state.db).await.unwrap();
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();assert!(fenced.prepare_one(&owner_auth,&deletion,&original).await.is_err(),"Legacy witnessless plan authorized physical erasure");fenced.tx.rollback().await.unwrap();}
 sqlx::query("UPDATE attachment_erasure_outbox SET plan_json=$1,plan_sha256=$2 WHERE operation_id=$3").bind(&plan_original).bind(crate::storage::attachment_upload::digest(plan_original.as_bytes())).bind(&deletion).execute(&state.db).await.unwrap();
 assert!(FencedTransaction::begin(&state.db).await.unwrap().finalize_existing(&owner_auth,&deletion,&original).await.is_err(),"Incomplete physical paths created disposition");
 let count:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_erasure_dispositions").fetch_one(&state.db).await.unwrap();assert_eq!(count,0);
 let mut completed=false;for _ in 0..3 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();let one=fenced.prepare_one(&owner_auth,&deletion,&original).await.unwrap();completed=one.execute().unwrap();fenced.tx.rollback().await.unwrap();if completed{break}}
 assert!(completed);assert!(!root.join(&original.intent.storage_path).exists());
 let raw_after_erasure:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&upload_operation).fetch_one(&state.db).await.unwrap();assert_eq!(raw_after_erasure,raw_before);
 FencedTransaction::begin(&state.db).await.unwrap().finalize_existing(&owner_auth,&deletion,&original).await.unwrap();
 let disposition_before:String=sqlx::query_scalar("SELECT owned_path_set_json FROM attachment_erasure_dispositions WHERE asset_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap();
 FencedTransaction::begin(&state.db).await.unwrap().finalize_existing(&owner_auth,&deletion,&original).await.unwrap();
 let disposition_after:String=sqlx::query_scalar("SELECT owned_path_set_json FROM attachment_erasure_dispositions WHERE asset_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap();assert_eq!(disposition_before,disposition_after,"Exact retry rewrote immutable completion material");
 assert!(FencedTransaction::begin(&state.db).await.unwrap().finalize_existing(&viewer_auth,&deletion,&original).await.is_err());
 let retained:i64=sqlx::query_scalar("SELECT byte_length FROM attachment_retained_byte_ledger WHERE asset_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap();assert_eq!(retained,original.intent.byte_length,"Private native cleanup silently removed ledger charge");

 // Completed immutable SQL is insufficient without current native absence.
 let usage=storage::attachment_capacity::usage_report(&state.db).await.unwrap();assert_eq!(usage.retained_bytes,0);assert_eq!(usage.pending_bytes,0);
 let lock_revision:i64=sqlx::query_scalar("SELECT revision FROM attachment_capacity_lock WHERE id=1").fetch_one(&state.db).await.unwrap();
 let status:Value=client.get(format!("{base}/admin/storage-health")).bearer_auth(&status_token).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
 assert_eq!(status["attachment_capacity"]["quota_status"],"unlimited");assert_eq!(status["attachment_capacity"]["accounting_status"],"available");assert_eq!(status["attachment_capacity"]["charged_total_bytes"],"0");
 assert_eq!(sqlx::query_scalar::<_,i64>("SELECT revision FROM attachment_capacity_lock WHERE id=1").fetch_one(&state.db).await.unwrap(),lock_revision,"Read-only health committed accounting lock changes");
 assert_eq!(client.get(format!("{base}/admin/storage-health")).bearer_auth(&viewer).send().await.unwrap().status(),401);
 for (id,actor,service) in [("owned-status-human-key",Some("status-admin"),0i64),("owned-status-service-key",None,1i64)] {
  sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,'Owned status key',$3,'owned',$4,'[\"admin\"]')").bind(id).bind(actor).bind(crate::auth::api_keys::hash_api_key(id)).bind(service).execute(&state.db).await.unwrap();
  let response=client.get(format!("{base}/admin/storage-health")).header("X-API-Key",id).send().await.unwrap();assert_eq!(response.status(),200);let keyed:Value=response.json().await.unwrap();assert_eq!(keyed["attachment_capacity"]["charged_total_bytes"],"0");
  sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id=$1").bind(id).execute(&state.db).await.unwrap();assert!(!client.get(format!("{base}/admin/storage-health")).header("X-API-Key",id).send().await.unwrap().status().is_success());
 }


 std::fs::write(root.join(&original.intent.storage_path),b"FOREIGN RESURRECTED OBJECT").unwrap();assert!(storage::attachment_capacity::usage_report(&state.db).await.is_err());
 let unavailable:Value=client.get(format!("{base}/admin/storage-health")).bearer_auth(&status_token).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();assert_eq!(unavailable["attachment_capacity"]["accounting_status"],"unavailable");assert!(unavailable["attachment_capacity"]["charged_total_bytes"].is_null());
 assert!(root.join(&original.intent.storage_path).exists());std::fs::remove_file(root.join(&original.intent.storage_path)).unwrap();
 let claim_before:String=sqlx::query_scalar("SELECT admission_claim_sha256 FROM attachment_erasure_dispositions WHERE asset_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap();sqlx::query("UPDATE attachment_erasure_dispositions SET admission_claim_sha256=$1 WHERE asset_id=$2").bind("0".repeat(64)).bind(&asset).execute(&state.db).await.unwrap();assert!(storage::attachment_capacity::usage_report(&state.db).await.is_err());sqlx::query("UPDATE attachment_erasure_dispositions SET admission_claim_sha256=$1 WHERE asset_id=$2").bind(&claim_before).bind(&asset).execute(&state.db).await.unwrap();
 let response=client.post(format!("{base}/spaces/space/documents")).bearer_auth(&token).json(&json!({"source_id":"accounting-next","title":"Next","content":"NEXT ORIGINAL","property_values":[]})).send().await.unwrap();let status=response.status();let next:Value=response.json().await.unwrap();assert_eq!(status,201,"{next}");
 let next_draft=next["draft_id"].as_str().unwrap();let next_endpoint=format!("{base}/spaces/space/documents/accounting-next/drafts/{next_draft}");
 let current:Value=client.get(&next_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();let response=client.put(&next_endpoint).bearer_auth(&token).json(&json!({"content":protected,"source_digest":current["source_digest"]})).send().await.unwrap();assert!(response.status().is_success());assert!(storage::attachment_capacity::usage_report(&state.db).await.is_err(),"Fresh protected reference allowed charge subtraction");
 let current:Value=client.get(&next_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();let response=client.put(&next_endpoint).bearer_auth(&token).json(&json!({"content":"NEXT ORIGINAL","source_digest":current["source_digest"]})).send().await.unwrap();assert!(response.status().is_success());
 let request=super::super::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"actual-fixture".into(),actor_id:"owner".into(),space_id:"space".into(),document_id:"accounting-next".into(),original_name:"proof.bin".into(),mime_type:"application/octet-stream".into()};
 let caller=WriterLease::begin(&root,&uuid::Uuid::new_v4().to_string(),Producer::Direct{request:request.clone()},64).unwrap();
 let key_text="owned-accounting-personal-key";let key_hash=crate::auth::api_keys::hash_api_key(key_text);sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('accounting-key','owner','Owned key',$1,'owned',0,'[\"read\",\"write\"]')").bind(&key_hash).execute(&state.db).await.unwrap();
 let request_http=Request::builder().method("POST").uri("/private-proof").header("X-API-Key",key_text).body(Body::empty()).unwrap();let(mut parts,_)=request_http.into_parts();let key_auth=crate::auth::middleware::AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
 sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='accounting-key'").execute(&state.db).await.unwrap();assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&key_auth).await.is_err());sqlx::query("UPDATE api_keys SET scopes='[\"read\",\"write\"]' WHERE id='accounting-key'").execute(&state.db).await.unwrap();
 sqlx::query("UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z' WHERE id='accounting-key'").execute(&state.db).await.unwrap();assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&key_auth).await.is_err());sqlx::query("UPDATE api_keys SET expires_at=NULL WHERE id='accounting-key'").execute(&state.db).await.unwrap();
 sqlx::query("DELETE FROM api_keys WHERE id='accounting-key'").execute(&state.db).await.unwrap();assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&key_auth).await.is_err());sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('accounting-key','owner','Owned key',$1,'owned',0,'[\"read\",\"write\"]')").bind(&key_hash).execute(&state.db).await.unwrap();


 assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&viewer_auth).await.is_err());
 sqlx::query("UPDATE users SET is_active=0 WHERE id='owner'").execute(&state.db).await.unwrap();assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&owner_auth).await.is_err());sqlx::query("UPDATE users SET is_active=1 WHERE id='owner'").execute(&state.db).await.unwrap();
 sqlx::query("UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE id='owner-session'").execute(&state.db).await.unwrap();assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&owner_auth).await.is_err());sqlx::query("UPDATE sessions SET expires_at='2999-01-01T00:00:00Z' WHERE id='owner-session'").execute(&state.db).await.unwrap();
 let mut foreign_request=request.clone();foreign_request.operation_id=uuid::Uuid::new_v4().to_string();let foreign=WriterLease::begin(&root,&uuid::Uuid::new_v4().to_string(),Producer::Direct{request:foreign_request},64).unwrap();assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&owner_auth).await.is_err(),"Foreign active capsule was arbitrarily excluded");assert!(storage::attachment_capacity::usage_report(&state.db).await.is_err());
 let unavailable:Value=client.get(format!("{base}/admin/storage-health")).bearer_auth(&status_token).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();assert_eq!(unavailable["attachment_capacity"]["accounting_status"],"unavailable");assert!(unavailable["attachment_capacity"]["charged_total_bytes"].is_null());
drop(foreign);drop(caller);
 std::env::set_var("ATTACHMENT_STORAGE_QUOTA_BYTES","1048576");
 let next_operation=uuid::Uuid::new_v4().to_string();let body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Next.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nSECOND DIFFERENT BODY\r\n--{boundary}--\r\n");
 let response=client.post(format!("{base}/spaces/space/documents/accounting-next/attachments")).header("X-API-Key",key_text).header("x-starlane-upload-operation",&next_operation).header("content-type",format!("multipart/form-data; boundary={boundary}")).body(body).send().await.unwrap();let status=response.status();let next_uploaded:Value=response.json().await.unwrap();assert_eq!(status,201,"{next_uploaded}");
 let next_original={let mut conn=state.db.acquire().await.unwrap();storage::attachment_upload::receipt(&mut conn,&next_operation).await.unwrap().unwrap()};let next_raw=serde_json::to_string(&next_original).unwrap();
 let usage=storage::attachment_capacity::usage_report(&state.db).await.unwrap();assert_eq!(usage.retained_bytes,next_original.intent.byte_length);assert_eq!(usage.pending_bytes,0);
  let status:Value=client.get(format!("{base}/admin/storage-health")).bearer_auth(&status_token).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();assert_eq!(status["attachment_capacity"]["quota_status"],"configured");assert_eq!(status["attachment_capacity"]["quota_bytes"],"1048576");assert_eq!(status["attachment_capacity"]["charged_retained_bytes"],next_original.intent.byte_length.to_string());assert_eq!(status["attachment_capacity"]["logical_retained_bytes"],next_original.intent.byte_length.to_string());assert_eq!(status["attachment_capacity"]["reserved_maximum_bytes"],"0");
 std::env::set_var("ATTACHMENT_STORAGE_QUOTA_BYTES","2097152");let replacement_operation=uuid::Uuid::new_v4().to_string();let replacement_body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Next.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nTHIRD REPLACEMENT OWNED BODY\r\n--{boundary}--\r\n");
 let response=client.post(format!("{base}/spaces/space/documents/accounting-next/attachment-families/{}/versions",next_original.intent.asset_id)).bearer_auth(&token).header("x-starlane-upload-operation",&replacement_operation).header("x-starlane-attachment-version",&next_original.intent.asset_id).header("x-starlane-attachment-revision","1").header("content-type",format!("multipart/form-data; boundary={boundary}")).body(replacement_body).send().await.unwrap();let status=response.status();let replaced:Value=response.json().await.unwrap();assert_eq!(status,201,"{replaced}");
 let replacement={let mut conn=state.db.acquire().await.unwrap();storage::attachment_upload::receipt(&mut conn,&replacement_operation).await.unwrap().unwrap()};assert!(client.delete(format!("{base}/spaces/space/documents/accounting-next/attachments/{}",replacement.intent.asset_id)).bearer_auth(&token).send().await.unwrap().status().is_success());

 assert!(client.delete(format!("{base}/spaces/space/documents/accounting-next")).bearer_auth(&token).send().await.unwrap().status().is_success());let target:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id='accounting-next'").fetch_one(&state.db).await.unwrap();let next_deletion=uuid::Uuid::new_v4().to_string();assert_eq!(client.delete(format!("{base}/spaces/space/documents/accounting-next/trash")).bearer_auth(&token).header("x-starlane-delete-operation",&next_deletion).json(&json!({"original_target":{"created_at":target.0,"deleted_at":target.1}})).send().await.unwrap().status(),204);
 let cleanup_endpoint=format!("{base}/spaces/space/documents/accounting-next/permanent-deletions/{next_deletion}/cleanup");let cleanup_request=json!({"original_target":{"created_at":target.0,"deleted_at":target.1}});
 assert!(!client.post(&cleanup_endpoint).bearer_auth(&viewer).json(&cleanup_request).send().await.unwrap().status().is_success());let mut changed=cleanup_request.clone();changed["original_target"]["created_at"]=json!("changed");assert_eq!(client.post(&cleanup_endpoint).bearer_auth(&token).json(&changed).send().await.unwrap().status(),409);
 let mut held_request=request.clone();held_request.operation_id=uuid::Uuid::new_v4().to_string();let active=WriterLease::begin(&root,&uuid::Uuid::new_v4().to_string(),Producer::Direct{request:held_request},64).unwrap();let review:Value=client.post(&cleanup_endpoint).bearer_auth(&token).json(&cleanup_request).send().await.unwrap().json().await.unwrap();assert_eq!(review["outcome"],"retained_review","{review}");assert!(root.join(&next_original.intent.storage_path).exists());assert!(root.join(&replacement.intent.storage_path).exists());drop(active);
 let target_path=root.join(&next_original.intent.storage_path);let retained_original=root.join("owned-fixture-held-canonical");std::fs::rename(&target_path,&retained_original).unwrap();std::fs::write(&target_path,b"FOREIGN BORROWED OBJECT").unwrap();let review:Value=client.post(&cleanup_endpoint).bearer_auth(&token).json(&cleanup_request).send().await.unwrap().json().await.unwrap();assert_eq!(review["outcome"],"retained_review","{review}");assert_eq!(std::fs::read(&target_path).unwrap(),b"FOREIGN BORROWED OBJECT");std::fs::remove_file(&target_path).unwrap();std::fs::rename(&retained_original,&target_path).unwrap();
 let response=client.post(&cleanup_endpoint).bearer_auth(&token).json(&cleanup_request).send().await.unwrap();let status=response.status();let cleaned:Value=response.json().await.unwrap();assert_eq!(status,200,"{cleaned}");assert_eq!(cleaned["outcome"],"physically_completed","{cleaned}");assert_eq!(cleaned["assets"].as_array().unwrap().len(),2);assert!(!root.join(&next_original.intent.storage_path).exists());assert!(!root.join(&replacement.intent.storage_path).exists());
 let repeated:Value=client.post(&cleanup_endpoint).bearer_auth(&token).json(&cleanup_request).send().await.unwrap().json().await.unwrap();assert_eq!(repeated["outcome"],"physically_completed","{repeated}");
 let response=client.post(format!("{base}/spaces/space/documents")).bearer_auth(&token).json(&json!({"source_id":"cleanup-empty","title":"Empty","content":"EMPTY ORIGINAL","property_values":[]})).send().await.unwrap();assert_eq!(response.status(),201);assert!(client.delete(format!("{base}/spaces/space/documents/cleanup-empty")).bearer_auth(&token).send().await.unwrap().status().is_success());let empty_target:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id='cleanup-empty'").fetch_one(&state.db).await.unwrap();let empty_operation=uuid::Uuid::new_v4().to_string();let empty_request=json!({"original_target":{"created_at":empty_target.0,"deleted_at":empty_target.1}});assert_eq!(client.delete(format!("{base}/spaces/space/documents/cleanup-empty/trash")).bearer_auth(&token).header("x-starlane-delete-operation",&empty_operation).json(&empty_request).send().await.unwrap().status(),204);let empty_result:Value=client.post(format!("{base}/spaces/space/documents/cleanup-empty/permanent-deletions/{empty_operation}/cleanup")).bearer_auth(&token).json(&empty_request).send().await.unwrap().json().await.unwrap();assert_eq!(empty_result["outcome"],"retained_review");assert_eq!(empty_result["reason"],"no_supported_owned_original_assets");
 // Pause after the complete asset loop, before the final status authority fence.
 // Both an already-completed roster and an empty roster must reject delayed logout.
 for (endpoint,request,operation) in [(&cleanup_endpoint,&cleanup_request,&next_deletion),(&format!("{base}/spaces/space/documents/cleanup-empty/permanent-deletions/{empty_operation}/cleanup"),&empty_request,&empty_operation)] {
  let(arrived,release)=cleanup_consumer::pause_final_observation(operation);let c=client.clone();let e=endpoint.clone();let r=request.clone();let t=token.clone();
  let pending=tokio::spawn(async move{c.post(e).bearer_auth(t).json(&r).send().await.unwrap()});
  tokio::time::timeout(std::time::Duration::from_secs(10),arrived).await.unwrap().unwrap();
  assert_eq!(client.post(format!("{base}/auth/logout")).bearer_auth(&token).send().await.unwrap().status(),204);
  release.send(()).unwrap();assert_eq!(pending.await.unwrap().status(),401,"Delayed logout published a cleanup outcome");
  sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('owner-session','owner','owner-refresh','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
 }
 let(arrived,release)=cleanup_consumer::pause_final_observation(&next_deletion);let c=client.clone();let e=cleanup_endpoint.clone();let r=cleanup_request.clone();let t=token.clone();
 let pending=tokio::spawn(async move{c.post(e).bearer_auth(t).json(&r).send().await.unwrap()});
 tokio::time::timeout(std::time::Duration::from_secs(10),arrived).await.unwrap().unwrap();
 sqlx::query("UPDATE sessions SET expires_at=$1 WHERE id='owner-session'").bind((chrono::Utc::now()+chrono::Duration::milliseconds(100)).to_rfc3339()).execute(&state.db).await.unwrap();
 tokio::time::sleep(std::time::Duration::from_millis(150)).await;release.send(()).unwrap();assert_eq!(pending.await.unwrap().status(),401,"Expired session published a cleanup outcome");
 sqlx::query("UPDATE sessions SET expires_at='2999-01-01T00:00:00Z' WHERE id='owner-session'").execute(&state.db).await.unwrap();
 println!("ACTUAL_CLEANUP_FINAL_OBSERVATION_PASS completed_logout_denied=true empty_logout_denied=true delayed_expiry_denied=true");
 // Actual HTTP body interruption leaves an original reserved Direct producer.
 std::env::set_var("ATTACHMENT_STORAGE_QUOTA_BYTES","1073741824");
 assert_eq!(client.post(format!("{base}/spaces/space/documents")).bearer_auth(&token).json(&json!({"source_id":"pending-live","title":"Pending","content":"PENDING SOURCE","property_values":[]})).send().await.unwrap().status(),201);
 let interrupted_operation=uuid::Uuid::new_v4().to_string();let boundary="pending-original-owned-boundary";
 let prefix=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Interrupted.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n");
 let mut socket=tokio::net::TcpStream::connect(address).await.unwrap();use tokio::io::{AsyncWriteExt,AsyncReadExt};
 socket.write_all(format!("POST /spaces/space/documents/pending-live/attachments HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nx-starlane-upload-operation: {interrupted_operation}\r\nx-starlane-upload-length: 128\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{prefix}",prefix.len()+8192).as_bytes()).await.unwrap();socket.write_all(&[7u8;64]).await.unwrap();
 let mut attempt=None;
 for _ in 0..100{for entry in std::fs::read_dir(root.join(".attachment-upload-admission")).unwrap(){let entry=entry.unwrap();let raw=std::fs::read(entry.path().join("claim.json")).unwrap();let c:Value=serde_json::from_slice(&raw).unwrap();if c["producer"]["request"]["operation_id"]==interrupted_operation&&std::fs::metadata(root.join(".attachment-upload-journal").join(entry.file_name()).join("payload.part")).is_ok_and(|m|m.len()>0){attempt=Some(entry.file_name().into_string().unwrap());break}}
 if attempt.is_some(){break}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
 let attempt=attempt.expect("Actual interrupted Direct stage did not become durable");socket.shutdown().await.unwrap();let mut raw_response=vec![];let _=tokio::time::timeout(std::time::Duration::from_secs(5),socket.read_to_end(&mut raw_response)).await;drop(socket);
 let active=WriterLease::open_idle(&root,&attempt).unwrap();let original_claim=active.original_claim().to_vec();let pending_request=json!({"producer_sha256":active.producer_sha256(),"claim_sha256":active.claim_sha256()});let pending_endpoint=format!("{base}/spaces/space/documents/pending-live/attachment-upload-attempts/{attempt}/cleanup");let pending_payload=root.join(".attachment-upload-journal").join(&attempt).join("payload.part");let original_bytes=std::fs::read(&pending_payload).unwrap();
 let discovery_endpoint=format!("{base}/spaces/space/documents/pending-live/attachment-upload-attempts");let active_listing:Value=client.get(&discovery_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();assert_eq!(active_listing["availability"],"not_available");assert_eq!(active_listing["reason"],"active_or_unproved_native_custody");assert!(active_listing["attempts"].as_array().unwrap().is_empty());

 let result:Value=client.post(&pending_endpoint).bearer_auth(&token).json(&pending_request).send().await.unwrap().json().await.unwrap();assert_eq!(result["outcome"],"retained_review","{result}");assert_eq!(std::fs::read(&pending_payload).unwrap(),original_bytes);drop(active);
 assert!(!client.get(&discovery_endpoint).bearer_auth(&viewer).send().await.unwrap().status().is_success());assert!(!client.get(&discovery_endpoint).header("X-API-Key",key_text).send().await.unwrap().status().is_success());assert_eq!(client.get(&discovery_endpoint).send().await.unwrap().status(),401);assert!(!client.get(format!("{base}/spaces/space/documents/absent-original/attachment-upload-attempts")).bearer_auth(&token).send().await.unwrap().status().is_success());
 let before_revision:i64=sqlx::query_scalar("SELECT revision FROM attachment_capacity_lock WHERE id=1").fetch_one(&state.db).await.unwrap();let discovered:Value=client.get(&discovery_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();assert_eq!(discovered["availability"],"available","{discovered}");let selected=discovered["attempts"].as_array().unwrap().iter().find(|x|x["attempt_id"]==attempt).unwrap();assert_eq!(selected["original_operation_id"],interrupted_operation);assert_eq!(selected["producer_kind"],"original_direct");assert!(selected.get("filename").is_none());let discovered_request=json!({"producer_sha256":selected["producer_sha256"],"claim_sha256":selected["claim_sha256"]});assert_eq!(discovered_request,pending_request);let after_revision:i64=sqlx::query_scalar("SELECT revision FROM attachment_capacity_lock WHERE id=1").fetch_one(&state.db).await.unwrap();assert_eq!(before_revision,after_revision,"Discovery persisted capacity observation mutation");assert_eq!(std::fs::read(&pending_payload).unwrap(),original_bytes);
 sqlx::query("UPDATE attachment_capacity_reservations SET admission_claim_sha256=$1 WHERE operation_id=$2").bind("f".repeat(64)).bind(&attempt).execute(&state.db).await.unwrap();let changed_listing:Value=client.get(&discovery_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();assert_eq!(changed_listing["availability"],"not_available");assert!(changed_listing["attempts"].as_array().unwrap().is_empty());sqlx::query("UPDATE attachment_capacity_reservations SET admission_claim_sha256=$1 WHERE operation_id=$2").bind(pending_request["claim_sha256"].as_str().unwrap()).bind(&attempt).execute(&state.db).await.unwrap();
 let bound_ids:Vec<String>=(0..129).map(|_|uuid::Uuid::new_v4().to_string()).collect();for id in &bound_ids{sqlx::query("INSERT INTO attachment_capacity_reservations(operation_id,request_sha256,maximum_bytes,state,admission_claim_sha256) VALUES($1,$2,1,'reserved',$2)").bind(id).bind("b".repeat(64)).execute(&state.db).await.unwrap();}let bounded_listing:Value=client.get(&discovery_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();assert_eq!(bounded_listing["availability"],"not_available");assert_eq!(bounded_listing["reason"],"reserved_attempt_inventory_exceeds_bound");for id in bound_ids{sqlx::query("DELETE FROM attachment_capacity_reservations WHERE operation_id=$1").bind(id).execute(&state.db).await.unwrap();}

 assert!(!client.post(&pending_endpoint).bearer_auth(&viewer).json(&pending_request).send().await.unwrap().status().is_success());let mut wrong=pending_request.clone();wrong["claim_sha256"]=json!("a".repeat(64));let wrong_result:Value=client.post(&pending_endpoint).bearer_auth(&token).json(&wrong).send().await.unwrap().json().await.unwrap();assert_eq!(wrong_result["outcome"],"retained_review");
 let original_held=pending_payload.with_extension("fixture-original-held");std::fs::rename(&pending_payload,&original_held).unwrap();std::fs::write(&pending_payload,b"FOREIGN BORROWED FIXTURE BYTES").unwrap();let result:Value=client.post(&pending_endpoint).bearer_auth(&token).json(&pending_request).send().await.unwrap().json().await.unwrap();assert_eq!(result["outcome"],"retained_review");assert_eq!(std::fs::read(&pending_payload).unwrap(),b"FOREIGN BORROWED FIXTURE BYTES");std::fs::remove_file(&pending_payload).unwrap();std::fs::rename(&original_held,&pending_payload).unwrap();
 let response=client.post(format!("{base}/spaces/foreign-space/documents")).bearer_auth(&token).json(&json!({"source_id":"pending-reference-writer","title":"Pending reference writer","content":"","property_values":[]})).send().await.unwrap();let new_foreign:Value=response.json().await.unwrap();let foreign_draft=new_foreign["draft_id"].as_str().unwrap();let foreign_endpoint=format!("{base}/spaces/foreign-space/documents/pending-reference-writer/drafts/{foreign_draft}");let foreign_current:Value=client.get(&foreign_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();
 let(arrived,release)=pending_consumer::pause_before_step(&attempt);let db=state.db.clone();let auth_request=Request::builder().uri("/pending-proof").header("Authorization",format!("Bearer {token}")).body(Body::empty()).unwrap();let(mut auth_parts,_)=auth_request.into_parts();let actor=crate::auth::middleware::AuthUser::from_request_parts(&mut auth_parts,&state).await.unwrap();let original_attempt=attempt.clone();let req=serde_json::from_value(pending_request.clone()).unwrap();
 let interrupted=tokio::spawn(async move{cleanup_pending(&db,&actor,"space","pending-live",&original_attempt,&req).await});tokio::time::timeout(std::time::Duration::from_secs(10),arrived).await.unwrap().unwrap();
 let c=client.clone();let e=foreign_endpoint.clone();let t=token.clone();let independent_writer=tokio::spawn(async move{c.put(e).bearer_auth(t).json(&json!({"source_digest":foreign_current["source_digest"],"content":"A fresh unrelated reference source"})).send().await.unwrap()});tokio::time::sleep(std::time::Duration::from_millis(150)).await;assert!(!independent_writer.is_finished(),"Actual foreign-space writer crossed pending native proof/unlink fence");
 interrupted.abort();assert!(interrupted.await.unwrap_err().is_cancelled());drop(release);assert_eq!(independent_writer.await.unwrap().status(),200);
 assert_eq!(std::fs::read(&pending_payload).unwrap(),original_bytes);let state_before:String=sqlx::query_scalar("SELECT state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&state.db).await.unwrap();assert_eq!(state_before,"reserved");
 let response=client.post(&pending_endpoint).bearer_auth(&token).json(&discovered_request).send().await.unwrap();let status=response.status();let result:Value=response.json().await.unwrap();assert_eq!(status,200,"{result}");assert_eq!(result["outcome"],"physically_completed","{result}");assert!(!pending_payload.exists());let repeated:Value=client.post(&pending_endpoint).bearer_auth(&token).json(&pending_request).send().await.unwrap().json().await.unwrap();assert_eq!(repeated,result);let idle=WriterLease::open_idle(&root,&attempt).unwrap();assert_eq!(idle.original_claim(),original_claim);drop(idle);let after_listing:Value=client.get(&discovery_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();assert_eq!(after_listing["availability"],"available");assert!(!after_listing["attempts"].as_array().unwrap().iter().any(|x|x["attempt_id"]==attempt));
 // Expiry is proved immediately before a real one-path unlink, while user
 // and session mutations remain excluded by the held original-human rows.
 let clock_attempt=uuid::Uuid::new_v4().to_string();let clock_request=storage::attachment_upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:state.base_url.clone(),actor_id:"owner".into(),space_id:"space".into(),document_id:"pending-live".into(),original_name:"Clock.bin".into(),mime_type:"application/octet-stream".into()};let clock_lease=WriterLease::begin(&root,&clock_attempt,Producer::Direct{request:clock_request.clone()},64).unwrap();storage::attachment_capacity::reserve_authorized(&state.db,&clock_lease,Some(2048),&owner_auth).await.unwrap();let clock_body=json!({"producer_sha256":clock_lease.producer_sha256(),"claim_sha256":clock_lease.claim_sha256()});let mut clock_ingest=storage::attachment_upload::Ingest::reserve_with_lease(&root,clock_request,64,clock_lease.clone()).unwrap();clock_ingest.append(b"CLOCK OWNED BYTES").unwrap();drop(clock_ingest);drop(clock_lease);let clock_path=root.join(".attachment-upload-journal").join(&clock_attempt).join("payload.part");
 let auth_request=Request::builder().uri("/pending-clock-proof").header("Authorization",format!("Bearer {token}")).body(Body::empty()).unwrap();let(mut parts,_)=auth_request.into_parts();let clock_actor=crate::auth::middleware::AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
 let expires=chrono::Utc::now()+chrono::Duration::seconds(2);sqlx::query("UPDATE sessions SET expires_at=$1 WHERE id='owner-session'").bind(expires.to_rfc3339()).execute(&state.db).await.unwrap();
 let(arrived,release)=pending_consumer::pause_before_step(&clock_attempt);let db=state.db.clone();let r=serde_json::from_value(clock_body.clone()).unwrap();let original=clock_attempt.clone();let blocked=tokio::spawn(async move{cleanup_pending(&db,&clock_actor,"space","pending-live",&original,&r).await});tokio::time::timeout(std::time::Duration::from_secs(10),arrived).await.unwrap().unwrap();let remaining=(expires-chrono::Utc::now()).to_std().unwrap_or_default();tokio::time::sleep(remaining+std::time::Duration::from_millis(20)).await;assert!(chrono::Utc::now()>=expires);release.send(()).unwrap();assert!(matches!(blocked.await.unwrap(),Err(AppError::Unauthorized(_))));assert_eq!(std::fs::read(&clock_path).unwrap(),b"CLOCK OWNED BYTES");let original_state:String=sqlx::query_scalar("SELECT state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&clock_attempt).fetch_one(&state.db).await.unwrap();assert_eq!(original_state,"reserved");sqlx::query("UPDATE sessions SET expires_at='2999-01-01T00:00:00Z' WHERE id='owner-session'").execute(&state.db).await.unwrap();let completed:Value=client.post(format!("{base}/spaces/space/documents/pending-live/attachment-upload-attempts/{clock_attempt}/cleanup")).bearer_auth(&token).json(&clock_body).send().await.unwrap().json().await.unwrap();assert_eq!(completed["outcome"],"physically_completed","{completed}");
 // Real cleanup route against normal original Replay native blob setup, not a
 // claim of interrupted provider HTTP acceptance or rewritten Replay support.
 let replay_bytes=b"R123";let replay_attempt=uuid::Uuid::new_v4().to_string();let replay_asset=uuid::Uuid::new_v4().to_string();let replay=WriterLease::begin(&root,&replay_attempt,Producer::Replay{draft_scope:None,provider:"web".into(),incoming_id:uuid::Uuid::new_v4().to_string(),actor_id:"owner".into(),space_id:"space".into(),document_id:"pending-live".into(),asset_id:replay_asset.clone(),payload_fingerprint:"a".repeat(64),binary_sha256:storage::attachment_upload::digest(replay_bytes),byte_length:4},4).unwrap();storage::attachment_capacity::reserve_authorized(&state.db,&replay,Some(2048),&owner_auth).await.unwrap();let replay_request=json!({"producer_sha256":replay.producer_sha256(),"claim_sha256":replay.claim_sha256()});let blob=storage::asset_blob::create_with_lease(&root,"outbox","pending-live","bin",replay_bytes,replay.clone()).unwrap();let replay_path=root.join(blob.path());drop(blob);drop(replay);let replay_listing:Value=client.get(&discovery_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();assert_eq!(replay_listing["availability"],"available");let replay_selected=replay_listing["attempts"].as_array().unwrap().iter().find(|x|x["attempt_id"]==replay_attempt).unwrap();assert_eq!(replay_selected["producer_kind"],"original_replay");assert_eq!(replay_selected["producer_sha256"],replay_request["producer_sha256"]);
 let foreign_attempt=uuid::Uuid::new_v4().to_string();let foreign=WriterLease::begin(&root,&foreign_attempt,Producer::Replay{draft_scope:None,provider:"other".into(),incoming_id:uuid::Uuid::new_v4().to_string(),actor_id:"owner".into(),space_id:"space".into(),document_id:"pending-live".into(),asset_id:replay_asset.clone(),payload_fingerprint:"b".repeat(64),binary_sha256:storage::attachment_upload::digest(replay_bytes),byte_length:4},4).unwrap();let foreign_claim=foreign.original_claim().to_vec();drop(foreign);
 let result:Value=client.post(format!("{base}/spaces/space/documents/pending-live/attachment-upload-attempts/{replay_attempt}/cleanup")).bearer_auth(&token).json(&replay_request).send().await.unwrap().json().await.unwrap();assert_eq!(result["outcome"],"retained_review","Selected idle Replay claim without payload was missed: {result}");assert_eq!(std::fs::read(&replay_path).unwrap(),replay_bytes);assert_eq!(std::fs::read(root.join(".attachment-upload-admission").join(&foreign_attempt).join("claim.json")).unwrap(),foreign_claim);
 // Remove only this disposable test-created unreserved conflicting producer to
 // qualify the independently supported one-original fixture. Product retains it.
 std::fs::remove_dir_all(root.join(".attachment-upload-admission").join(&foreign_attempt)).unwrap();
 let response=client.post(format!("{base}/spaces/space/documents/pending-live/attachment-upload-attempts/{replay_attempt}/cleanup")).bearer_auth(&token).json(&replay_request).send().await.unwrap();let status=response.status();let result:Value=response.json().await.unwrap();assert_eq!(status,200,"{result}");assert_eq!(result["outcome"],"physically_completed","{result}");assert!(!replay_path.exists());
 let states:Vec<String>=sqlx::query_scalar("SELECT state FROM attachment_capacity_reservations WHERE operation_id IN ($1,$2)").bind(&attempt).bind(&replay_attempt).fetch_all(&state.db).await.unwrap();assert_eq!(states,vec!["disposed","disposed"]);
 println!("ACTUAL_PENDING_DISCOVERY_HTTP_PASS exact_original_selection=true discovery_to_post=true active_unavailable=true native_sql_hash_mismatch_unavailable=true bound_exhaustion_unavailable=true viewer_key_anonymous_absent_denied=true observation_rolls_back=true disposed_excluded=true replay_native_setup_discovered=true");
 println!("ACTUAL_PENDING_CLEANUP_DIRECT_INTERRUPTED_HTTP_REPLAY_NATIVE_SETUP_PASS direct_body_interruption=true active_retained=true wrong_claim_retained=true foreign_inode_preserved=true helper_cancel_before_unlink_retry=true actual_foreign_space_writer_waits=true expiry_before_unlink_preserves_bytes=true exact_native_claim_preserved=true replay_original_blob_setup_http_cleanup=true");
 println!("ACTUAL_CLEANUP_API_PREVIOUS_RETIRED_DIRECT_PASS same_uuid_retry=true original_witness=true active_foreign_retained=true foreign_inode_preserved=true empty_roster_retained=true viewer_denied=true previous_version_and_retirement_completed=true");
 let usage=storage::attachment_capacity::usage_report(&state.db).await.unwrap();assert_eq!(usage.retained_bytes,0,"Multiple completed originals could not share one held native roster");assert_eq!(usage.pending_bytes,0);
 let next_raw_after:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&next_operation).fetch_one(&state.db).await.unwrap();assert_eq!(next_raw,next_raw_after);
 std::env::remove_var("ATTACHMENT_STORAGE_QUOTA_BYTES");
 println!("ACTUAL_COMPLETED_PERSONAL_KEY_ACCOUNTING_PASS current_write_key_upload=true scope_revocation_expiry_refused=true original_human_controls_preserved=true");
 println!("ACTUAL_ADMIN_CAPACITY_HTTP_PASS exact_charge=true no_committed_lock_mutation=true unavailable_not_zero=true current_admin_only=true configured_and_unlimited=true");
 println!("ACTUAL_COMPLETED_ACCOUNTING_BRIDGE_PASS engine={} two_completed_originals=true owned_active_writer_registered_upload=true native_resurrection_refused=true sql_tamper_refused=true fresh_reference_refused=true foreign_active_capsule_refused=true current_writer_authority_refused=true immutable_raw_receipts=true",if pg.is_some(){"postgresql"}else{"sqlite"});
 println!("ACTUAL_FOREIGN_SPACE_DRAFT_WRITER_FENCE_ONLY_PASS engine={} unchanged_while_held=true committed_after_release=true real_registered_winner=true reserved_disposer_refused_committed=true original_current_human_controls=true session_expiry_recheck=true actual_logout_waits=true foreign_publication_waits=true",if pg.is_some(){"postgresql"}else{"sqlite"});
 server.abort();let _=server.await;state.db.close().await;if let Some(admin)=admin{sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&admin).await.unwrap();admin.close().await;}
 }
}

#[path="attachment_erasure_references.rs"]
pub(super) mod references;

#[path="attachment_erasure_cleanup.rs"]
mod cleanup_consumer;
pub(crate) use cleanup_consumer::{cleanup,CleanupOutcome};

#[path="attachment_pending_cleanup.rs"]
mod pending_consumer;
pub(crate) use pending_consumer::{cleanup_pending,PendingRequest,PendingOutcome,discover_pending,PendingDiscovery};
// Purpose-separated original disposed source observation; no cleanup grant.
pub(super) use pending_consumer::{DisposedPendingSource,capture_disposed_pending_source_in};

#[cfg(test)]
#[path="attachment_erasure_completed_capture_tests.rs"]
mod completed_capture_tests;

#[cfg(test)]
#[path="attachment_erasure_pg_completed_capture_tests.rs"]
mod pg_completed_capture_tests;

#[cfg(test)]
#[path="attachment_erasure_sqlite_completed_families_tests.rs"]
mod sqlite_completed_families_tests;

#[cfg(test)]
#[path="attachment_erasure_restored_native_tests.rs"]
mod restored_native_tests;

pub(super) use pending_consumer::check_disposed_snapshot_references_in;

#[cfg(test)]
#[path="attachment_composite_cold_tests.rs"]
mod composite_cold_tests;
