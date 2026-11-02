//! Transaction-owned logical retirement. Original associations and custody evidence are retained.
use serde::{Deserialize, Serialize};
use sqlx::{AnyConnection, Row};

pub(crate) type Association = (String, String, String, i64, String, String, String);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Retirement {
    pub format_version: u32,
    pub attachment_id: String,
    pub state: String,
    pub actor_id: String,
    pub space_id: String,
    pub document_id: String,
    pub origin: String,
    pub operation_id: String,
    pub retired_at: String,
    pub association: Association,
}
pub(crate) async fn read(conn: &mut AnyConnection, id: &str) -> Result<Option<Retirement>, String> {
    let row = sqlx::query("SELECT * FROM attachment_retirements WHERE attachment_id=$1")
        .bind(id).fetch_optional(&mut *conn).await.map_err(|e| e.to_string())?;
    let Some(row) = row else { return Ok(None) };
    let raw: String = row.try_get("receipt_json").map_err(|e| e.to_string())?;
    let r: Retirement = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if r.format_version != 1 || r.attachment_id != id || r.state != "retained"
        || !matches!(r.origin.as_str(), "direct" | "replay") || r.association.3 < 0
        || chrono::DateTime::parse_from_rfc3339(&r.retired_at).is_err() {
        return Err("Attachment retirement metadata invalid".into());
    }
    for (name, expected) in [
        ("attachment_id", r.attachment_id.as_str()), ("state", r.state.as_str()),
        ("actor_id", r.actor_id.as_str()), ("space_id", r.space_id.as_str()),
        ("document_id", r.document_id.as_str()), ("origin", r.origin.as_str()),
        ("operation_id", r.operation_id.as_str()), ("retired_at", r.retired_at.as_str()),
    ] {
        let actual: String = row.try_get(name).map_err(|e| e.to_string())?;
        if actual != expected || actual.is_empty() { return Err("Attachment retirement scalar differs".into()); }
    }
    let association: String = row.try_get("association_json").map_err(|e| e.to_string())?;
    if serde_json::from_str::<Association>(&association).map_err(|e| e.to_string())? != r.association
        || serde_json::to_string(&r).map_err(|e| e.to_string())? != raw {
        return Err("Attachment retirement receipt differs".into());
    }
    Ok(Some(r))
}
/// Caller holds current page/association mutation locks and commits this with its receipt.
pub(crate) async fn retire(
    conn: &mut AnyConnection, id: &str, actor: &str, space: &str, doc: &str,
    origin: &str, operation: &str, expected: &Association,
) -> Result<Retirement, String> {
    if !matches!(origin,"direct"|"replay") || [id,actor,space,doc,operation].iter().any(|v|v.is_empty()) {
        return Err("Attachment retirement identity invalid".into());
    }
    if read(conn,id).await?.is_some() { return Err("Attachment already retired".into()); }
    let actual: Option<Association> = sqlx::query_as("SELECT filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at FROM attachments WHERE id=$1 AND document_id=$2 AND space_id=$3")
        .bind(id).bind(doc).bind(space).fetch_optional(&mut *conn).await.map_err(|e|e.to_string())?;
    if actual.as_ref() != Some(expected) { return Err("Retirement association changed".into()); }
    let r=Retirement {format_version:1,attachment_id:id.into(),state:"retained".into(),actor_id:actor.into(),space_id:space.into(),document_id:doc.into(),origin:origin.into(),operation_id:operation.into(),retired_at:chrono::Utc::now().to_rfc3339(),association:expected.clone()};
    sqlx::query("INSERT INTO attachment_retirements(attachment_id,state,actor_id,space_id,document_id,origin,operation_id,retired_at,association_json,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(id).bind(&r.state).bind(actor).bind(space).bind(doc).bind(origin).bind(operation).bind(&r.retired_at)
        .bind(serde_json::to_string(expected).map_err(|e|e.to_string())?).bind(serde_json::to_string(&r).map_err(|e|e.to_string())?)
        .execute(&mut *conn).await.map_err(|e|e.to_string())?;
    if !matches_staged_association(conn,&r).await? {return Err("Retirement publication differs".into());}
    Ok(r)
}

/// Precommit check only: immutable attribution must equal the association captured in this mutation.
/// Historical reads after relocation resolve current association separately and never rewrite this receipt.
pub(crate) async fn matches_staged_association(conn:&mut AnyConnection,r:&Retirement)->Result<bool,String> {
    let actual:Option<Association>=sqlx::query_as("SELECT filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at FROM attachments WHERE id=$1 AND document_id=$2 AND space_id=$3")
        .bind(&r.attachment_id).bind(&r.document_id).bind(&r.space_id).fetch_optional(&mut *conn).await.map_err(|e|e.to_string())?;
    Ok(actual.as_ref()==Some(&r.association) && read(conn,&r.attachment_id).await?.as_ref()==Some(r))
}
