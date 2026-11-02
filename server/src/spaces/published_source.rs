//! Original published row+body evidence. Pure digests grant no authority.
use crate::auth::routes::AppError;
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use sqlx::AnyConnection;

pub(crate) fn conflict()->AppError {AppError::Conflict("source_conflict: Published source changed; retain the original queued edit for review".into())}
pub(crate) fn required(value:Option<&Value>)->Result<String,AppError> {
    match value {Some(Value::String(s)) if s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))=>Ok(s.clone()),_=>Err(AppError::Conflict("source_required: Published replay requires the original published_source_digest; retain queued edits without rebasing from a latest GET".into()))}
}
pub(crate) fn digest(row:&Value,bytes:&[u8])->Result<String,AppError> {
    fn ordered(v:&Value)->Value {match v {Value::Object(m)=>{let mut keys:Vec<_>=m.keys().collect();keys.sort();let mut out=serde_json::Map::new();for k in keys {out.insert(k.clone(),ordered(&m[k]));}Value::Object(out)},Value::Array(a)=>Value::Array(a.iter().map(ordered).collect()),v=>v.clone()}}
    let raw=serde_json::to_vec(&ordered(row)).map_err(AppError::internal)?;
    let mut hash=Sha256::new();hash.update(b"starlane-published-source-v1\0");hash.update((raw.len() as u64).to_be_bytes());hash.update(raw);hash.update((bytes.len() as u64).to_be_bytes());hash.update(bytes);Ok(hex::encode(hash.finalize()))
}
pub(crate) async fn selected(conn:&mut AnyConnection,space:&str,id:&str)->Result<Value,AppError> {
    let rows=super::document_deletion::rows(conn,"SELECT * FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL AND published_at IS NOT NULL",&[id,space]).await.map_err(AppError::internal)?;
    if rows.len()!=1 {return Err(AppError::NotFound("Published source unavailable".into()));}Ok(rows[0].clone())
}
pub(crate) fn expected(before:&Value,path:&str,actor:&str,revision:&str)->Value {
    let mut row=before.clone();row["content_path"]=json!(path);row["last_updated_by"]=json!(actor);row["updated_at"]=json!(revision);row
}

use crate::auth::middleware::AuthUser;
#[derive(Clone)]
pub(crate) struct SourceProof {pub query:String,pub parameters:Vec<String>,pub rows:Vec<Value>}
async fn rows(conn:&mut AnyConnection,q:&str,args:&[&str])->Result<Vec<Value>,AppError> {
    super::document_deletion::rows(conn,q,args).await.map_err(AppError::internal)
}
pub(crate) async fn verify_source(conn:&mut AnyConnection,proofs:&[SourceProof])->Result<(),AppError> {
    for p in proofs {let a:Vec<&str>=p.parameters.iter().map(String::as_str).collect();if rows(conn,&p.query,&a).await?!=p.rows {return Err(conflict());}}
    Ok(())
}
pub(crate) async fn capture_source(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    ancestors: &[String],
) -> Result<Vec<SourceProof>, AppError> {
    let mut specs = vec![
        (
            "SELECT * FROM documents WHERE space_id=$1".into(),
            vec![space.into()],
        ),
        ("SELECT id,space_id,parent_id FROM documents".into(), vec![]),
        (
            "SELECT id,role,is_active,account_kind FROM users WHERE id=$1".into(),
            vec![auth.claims.sub.clone()],
        ),
    ];
    for table in [
        "spaces",
        "space_members",
        "space_group_permissions",
        "space_user_blocks",
        "space_roles",
    ] {
        specs.push((
            format!(
                "SELECT * FROM {table} WHERE {}=$1",
                if table == "spaces" { "id" } else { "space_id" }
            ),
            vec![space.into()],
        ));
    }
    for table in [
        "groups",
        "group_members",
        "group_global_roles",
        "global_roles",
    ] {
        specs.push((format!("SELECT * FROM {table}"), vec![]));
    }
    for table in [
        "attachments",
        "comments",
        "doc_subscriptions",
        "doc_tags",
        "doc_versions",
        "document_access_allowlist",
        "document_access_policies",
        "document_drafts",
        "document_workflow_events",
        "document_workflows",
        "excalidraw_files",
        "property_values",
        "reactions",
        "starred_documents",
    ] {
        specs.push((
            format!("SELECT * FROM {table} WHERE document_id=$1"),
            vec![id.into()],
        ));
    }
    specs.push(("SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1".into(),vec![id.into()]));
    for parent in ancestors {
        for table in [
            "documents",
            "document_drafts",
            "document_access_policies",
            "document_access_allowlist",
        ] {
            specs.push((
                format!(
                    "SELECT * FROM {table} WHERE {}=$1",
                    if table == "documents" {
                        "id"
                    } else {
                        "document_id"
                    }
                ),
                vec![parent.clone()],
            ));
        }
        specs.push(("SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1".into(),vec![parent.clone()]));
    }
    let mut proof = Vec::new();
    for (query, parameters) in specs {
        let args: Vec<&str> = parameters.iter().map(String::as_str).collect();
        let current = rows(conn, &query, &args).await?;
        proof.push(SourceProof {
            query,
            parameters,
            rows: current,
        });
    }
    proof.extend(super::page_viewer_grants::capture_page_dependencies(conn,space,id).await?);
    Ok(proof)
}
pub(crate) fn transition(proofs:&mut [SourceProof],id:&str,next:&Value)->Result<(),AppError> {
    let pages=proofs.iter_mut().find(|p|p.query=="SELECT * FROM documents WHERE space_id=$1").ok_or_else(conflict)?;
    let row=pages.rows.iter_mut().find(|r|r["id"]==id).ok_or_else(conflict)?;*row=next.clone();pages.rows.sort_by_cached_key(Value::to_string);Ok(())
}
