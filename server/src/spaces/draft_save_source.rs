//! Exact selected draft source for explicit retained-save review.
//! This grants no published-body View and performs no write or receipt adoption.
use axum::{extract::{Path,State},response::IntoResponse,routing::get,Json,Router};
use serde_json::json;
use crate::{AppState,auth::{middleware::AuthUser,routes::AppError}};
use super::draft_save;

pub(crate) fn router()->Router<AppState> {
    Router::new().route("/spaces/{space_id}/documents/{document_id}/drafts/{draft_id}/save-source",get(review))
}
async fn review(State(state):State<AppState>,auth:AuthUser,Path((space,document,draft)):Path<(String,String,String)>)->Result<impl IntoResponse,AppError>{
    // mutation=true is deliberate: this is a human write review, despite GET.
    // inspect resolves ordinary Page Edit + selected Draft Edit, proves full
    // source/access inventories, then exact participation/source/current write.
    // A read-only API key, guest, recovery bypass or matching lock owner alone
    // cannot authorize this projection.
    let (mut source,page)=draft_save::inspect_edit_source(&state,&auth,&space,&document,&draft).await?;
    let digest=draft_save::digest(&source.row)?;
    let response=json!({"schema_version":1,"space_id":space,"document_id":document,"draft_id":draft,"draft_source_digest":digest,"locked":page["locked"].as_i64()==Some(1),"locked_by":draft_save::optional(&page,"locked_by")?,"page_edit_allowed":true,"page_published":!page["published_at"].is_null(),"page_updated_at":draft_save::string(&page,"updated_at")?,"draft":{
        "title":draft_save::string(&source.row,"title")?,
        "content":draft_save::string(&source.row,"content")?,
        "icon":draft_save::optional(&source.row,"icon")?,
        "cover_image":draft_save::optional(&source.row,"cover_image")?
    }});
    // Edit permits reading the selected draft needed for a save, not the
    // published canonical label. Existing response proof remains unchanged.
    source.document_title=draft_save::string(&source.row,"title")?;
    let selected=super::drafts::response_from_saved(source,&auth.claims.sub)?;
    let mut response=response;
    response["selected_draft"]=serde_json::to_value(selected).map_err(AppError::internal)?;
    Ok(([(axum::http::header::CACHE_CONTROL,"no-store")],Json(response)))
}
#[cfg(test)]
#[path="draft_save_source_tests.rs"]
mod tests;
