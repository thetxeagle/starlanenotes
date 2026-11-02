//! Durable authorized retention plan. No unlink, quota release or erasure capability.
use crate::{auth::{middleware::AuthUser,routes::AppError},spaces::{access,document_deletion}};
use serde::{Deserialize,Serialize};
use serde_json::Value;
use sha2::{Digest,Sha256};
use sqlx::{AnyConnection,Row};
use std::collections::BTreeMap;
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct OriginalDeleteTarget {
 pub created_at:String,
 #[serde(deserialize_with="required_nullable_string")]
 pub deleted_at:Option<String>,
}
fn required_nullable_string<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<String>,D::Error>{Option::<String>::deserialize(d)}
impl OriginalDeleteTarget {
 pub(crate) fn validate(&self,trashed:bool)->Result<(),AppError>{
  let valid=|s:&str|!s.trim().is_empty()&&s.len()<=4096;
  if !valid(&self.created_at)||self.deleted_at.as_deref().is_some_and(|v|!valid(v))||(trashed&&self.deleted_at.is_none()){return Err(target_review())}Ok(())
 }
 pub(crate) fn matches_source(&self,source:&Value,document:&str)->bool {
  source["domain"]["documents"].as_array().is_some_and(|rows|rows.iter().any(|row|row["id"]==document&&row["created_at"]==self.created_at&&row["deleted_at"]==serde_json::to_value(&self.deleted_at).unwrap_or(Value::Null)))
 }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OriginalDeleteRequest {pub original_target:OriginalDeleteTarget}
pub(crate) fn target_review()->AppError{AppError::Conflict("Original page or Trash identity unavailable or changed; retain this attempt and review the original target".into())}
pub(crate) async fn verify_original_target(conn:&mut AnyConnection,space:&str,document:&str,target:&OriginalDeleteTarget)->Result<(),AppError>{
 target.validate(false)?;
 let row:Option<(String,Option<String>)>=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE space_id=$1 AND id=$2").bind(space).bind(document).fetch_optional(conn).await.map_err(AppError::internal)?;
 if row.as_ref()!=Some(&(target.created_at.clone(),target.deleted_at.clone())){return Err(target_review())}Ok(())
}
const ROWS:i64=512;
const BYTES:i64=8*1024*1024;
const TABLES:&[(&str,&str)]=&[
 ("documents","length(title)+length(content_path)"),
 ("attachments","length(storage_path)+length(original_name)"),
 ("document_drafts","length(content)+length(title)"),
 ("doc_versions","COALESCE(length(change_set),0)"),
 ("attachment_retained_byte_ledger","COALESCE(length(original_receipt_json),0)"),
 ("attachment_families","length(id)"),("attachment_version_records","length(original_storage_path)"),
 ("attachment_version_changes","length(receipt_json)"),("attachment_retirements","length(receipt_json)"),
 ("attachment_upload_receipts","length(receipt_json)"),("asset_replay_receipts","length(receipt_json)"),
 ("document_delete_replay_receipts","length(receipt_json)"),
 ("move_asset_path_aliases","length(old_path)"),
 ("move_commit_receipts","length(manifest_json)+length(expected_state_json)"),
 ("move_metadata_escrow","length(original_json)+length(expected_json)"),
 ("storage_journal_receipts","length(content_path)"),
 ("publication_receipts","length(manifest_json)"),("canonical_revision_receipts","length(manifest_json)"),
];
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Plan{
 pub format_version:u8,pub operation_id:String,pub actor_id:String,pub space_id:String,
 pub document_id:String,pub source_fingerprint:String,pub source:Value,
 pub references:BTreeMap<String,Vec<Value>>,pub native_review:Vec<Value>,
 pub disposition:String,pub created_at:String,pub original_target:OriginalDeleteTarget,
}
fn hash(s:&str)->String{hex::encode(Sha256::digest(s.as_bytes()))}
fn conflict()->AppError{AppError::Conflict("Original deletion retention plan unavailable or changed; retained bytes require review".into())}
pub(crate) fn operation(headers:&axum::http::HeaderMap)->Result<Option<String>,AppError>{
 let mut values=headers.get_all("x-starlane-delete-operation").iter();let Some(value)=values.next() else{return Ok(None)};
 if values.next().is_some(){return Err(conflict())}let value=value.to_str().map_err(|_|conflict())?;
 let id=uuid::Uuid::parse_str(value).map_err(|_|conflict())?;if id.to_string()!=value{return Err(conflict())}Ok(Some(value.into()))
}
async fn capture(conn:&mut AnyConnection)->Result<BTreeMap<String,Vec<Value>>,AppError>{
 let mut result=BTreeMap::new();let mut bytes=0i64;
 for(table,size)in TABLES{
  let(count,length):(i64,i64)=sqlx::query_as(&format!("SELECT count(*),COALESCE(SUM({size}),0) FROM {table}")).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
  bytes=bytes.checked_add(length).ok_or_else(conflict)?;if count>ROWS||bytes>BYTES{return Err(conflict())}
  let rows=document_deletion::rows(conn,&format!("SELECT * FROM {table} LIMIT {}",ROWS+1),&[]).await.map_err(|_|conflict())?;
  if rows.len()>ROWS as usize{return Err(conflict())}result.insert((*table).into(),rows);
 }
 if serde_json::to_vec(&result).map_err(AppError::internal)?.len()>BYTES as usize{return Err(conflict())}Ok(result)
}
pub(crate) async fn prepare(conn:&mut AnyConnection,auth:&AuthUser,prepared:&document_deletion::PreparedDeletion,operation_id:String,original_target:&OriginalDeleteTarget)->Result<Plan,AppError>{
 let source=prepared.retention_snapshot();if source["actor_id"]!=auth.claims.sub{return Err(conflict())}
 original_target.validate(false)?;if !original_target.matches_source(&source,source["document_id"].as_str().ok_or_else(conflict)?){return Err(target_review())}
 let references=capture(conn).await?;let mut native_review=vec![];
 // Read-only proof observations cannot become persisted filesystem handles or erase grants.
 #[cfg(target_os="linux")]
 for row in &references["attachment_upload_receipts"]{
  let Some(raw)=row["receipt_json"].as_str() else{return Err(conflict())};
  let receipt:Result<super::attachment_upload::Receipt,_>=serde_json::from_str(raw);
  if let Ok(r)=receipt {if r.intent.request.document_id==source["document_id"].as_str().unwrap_or(""){
   let current=super::attachment_upload::receipt(conn,&r.intent.request.operation_id).await;
   let proved=current.ok().flatten().as_ref()==Some(&r)&&super::attachment_upload::verify(&super::data_dir(),&r,true).is_ok();
   native_review.push(serde_json::json!({"asset_id":r.intent.asset_id,"original_receipt_sha256":hash(raw),"original_native_read_only_verified":proved,"erasure_authorized":false}));
  }}
 }
 let plan=Plan{format_version:2,original_target:original_target.clone(),operation_id,actor_id:auth.claims.sub.clone(),space_id:source["space_id"].as_str().ok_or_else(conflict)?.into(),document_id:source["document_id"].as_str().ok_or_else(conflict)?.into(),source_fingerprint:prepared.fingerprint().into(),source,references,native_review,disposition:"retained_review".into(),created_at:chrono::Utc::now().to_rfc3339()};
 let raw=serde_json::to_string(&plan).map_err(AppError::internal)?;if raw.len()>BYTES as usize{return Err(conflict())}
 sqlx::query("INSERT INTO attachment_erasure_outbox(operation_id,actor_id,space_id,document_id,source_fingerprint,plan_json,plan_sha256,state,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,'retained_review',$8)").bind(&plan.operation_id).bind(&plan.actor_id).bind(&plan.space_id).bind(&plan.document_id).bind(&plan.source_fingerprint).bind(&raw).bind(hash(&raw)).bind(&plan.created_at).execute(conn).await.map_err(AppError::internal)?;Ok(plan)
}
async fn read(conn:&mut AnyConnection,operation:&str)->Result<Option<Plan>,AppError>{
 let row=sqlx::query("SELECT * FROM attachment_erasure_outbox WHERE operation_id=$1").bind(operation).fetch_optional(conn).await.map_err(AppError::internal)?;let Some(row)=row else{return Ok(None)};
 let raw:String=row.try_get("plan_json").map_err(AppError::internal)?;if raw.len()>BYTES as usize{return Err(conflict())}let plan:Plan=serde_json::from_str(&raw).map_err(|_|conflict())?;
 if uuid::Uuid::parse_str(operation).map(|id|id.to_string()).ok().as_deref()!=Some(operation)||hash(&serde_json::to_string(&plan.source["domain"]).map_err(AppError::internal)?)!=plan.source_fingerprint||plan.format_version!=2||plan.operation_id!=operation||plan.disposition!="retained_review"||serde_json::to_string(&plan).map_err(AppError::internal)?!=raw||row.try_get::<String,_>("plan_sha256").map_err(AppError::internal)?!=hash(&raw){return Err(conflict())}
 plan.original_target.validate(false)?;if !plan.original_target.matches_source(&plan.source,&plan.document_id){return Err(target_review())}
 for(k,v)in [("actor_id",&plan.actor_id),("space_id",&plan.space_id),("document_id",&plan.document_id),("source_fingerprint",&plan.source_fingerprint),("state",&plan.disposition),("created_at",&plan.created_at)]{if row.try_get::<String,_>(k).map_err(AppError::internal)?!=*v{return Err(conflict())}}
 if plan.source["actor_id"]!=plan.actor_id||plan.source["space_id"]!=plan.space_id||plan.source["document_id"]!=plan.document_id||plan.source["source_fingerprint"]!=plan.source_fingerprint{return Err(conflict())}Ok(Some(plan))
}
pub(crate) async fn verify(conn:&mut AnyConnection,plan:&Plan)->Result<(),AppError>{
 if read(conn,&plan.operation_id).await?.as_ref()!=Some(plan){return Err(conflict())}
 // Non-cascading originals must survive the deletion unchanged. Live/draft/history rows
 // are preserved in the plan; the guarded SQL delete domain verifies their exact cascade.
 for(table,_)in TABLES.iter().filter(|(t,_)|!matches!(*t,"documents"|"attachments"|"document_drafts"|"doc_versions")){
  if document_deletion::rows(conn,&format!("SELECT * FROM {table} LIMIT {}",ROWS+1),&[]).await.map_err(|_|conflict())?!=plan.references[*table]{return Err(conflict())}
 }Ok(())
}
pub(crate) async fn retry(conn:&mut AnyConnection,auth:&AuthUser,space:&str,document:&str,operation:&str,original_target:&OriginalDeleteTarget)->Result<bool,AppError>{
 let Some(plan)=read(conn,operation).await? else{return Ok(false)};
 if plan.original_target!=*original_target||plan.actor_id!=auth.claims.sub||plan.space_id!=space||plan.document_id!=document{return Err(conflict())}
 let current=access::resolve_space_access(conn,access::principal_from_auth(auth)?,space,access::Purpose::Routine).await?;
 if !current.allows(access::Permission::DeleteContent){return Err(conflict())}
 let exists:Option<i64>=sqlx::query_scalar("SELECT 1 FROM documents WHERE id=$1").bind(document).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;if exists.is_some(){return Err(conflict())}
 // A retry reports the committed deletion/retention outcome only. It neither renews
 // old custody nor reads, erases or releases any byte based on page absence.
 Ok(true)
}

// Private guarded review observation; no constructor, serving or erase authority.
pub(crate) async fn load_for_physical_review(conn:&mut AnyConnection,operation:&str)->Result<Option<Plan>,AppError>{read(conn,operation).await}

// Historical original rows must persist byteexact, but later independent winners
// are allowed. This is provenance observation, not live reference or file authority.
pub(crate) async fn verify_historical_subset(conn:&mut AnyConnection,plan:&Plan)->Result<(),AppError>{
 if read(conn,&plan.operation_id).await?.as_ref()!=Some(plan){return Err(conflict())}
 for(table,_)in TABLES.iter().filter(|(t,_)|!matches!(*t,"documents"|"attachments"|"document_drafts"|"doc_versions")){
  let current=document_deletion::rows(conn,&format!("SELECT * FROM {table} LIMIT {}",ROWS+1),&[]).await.map_err(|_|conflict())?;if current.len()>ROWS as usize{return Err(conflict())}
  for original in plan.references.get(*table).ok_or_else(conflict)?{if !current.contains(original){return Err(conflict())}}
 }Ok(())
}
