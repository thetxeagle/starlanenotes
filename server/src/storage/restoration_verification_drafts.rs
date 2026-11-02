// Original draft source is the held canonical SQL snapshot. Mirror JSON/files
// cannot construct this plan, and this evidence grants no publication authority.
struct OriginalApplicationDraft {uri:String,expected:serde_json::Value}
struct OriginalApplicationDraftPlan<'a> {source:&'a Root,database:Identity,actor:String,session:String,drafts:Vec<OriginalApplicationDraft>}
impl<'a> StoppedVerificationSource<'a> {
 async fn application_draft_plan(&mut self)->anyhow::Result<OriginalApplicationDraftPlan<'a>> {
  use sqlx::Row;
  self.fence().await?;let actor=self.current_human().await?;
  let admin=self.admin.ok_or_else(||fail("Original draft human absent"))?;
  let auth=crate::auth::middleware::AuthUser{claims:admin.claims.clone(),api_key_scopes:None};
  let rows=sqlx::query("SELECT d.id,d.document_id,d.space_id,u.display_name AS owner_name,p.title AS document_title,p.published_at FROM document_drafts d JOIN documents p ON p.id=d.document_id AND p.space_id=d.space_id JOIN users u ON u.id=d.owner_id ORDER BY d.space_id,d.document_id,d.id LIMIT 257").fetch_all(&mut *self.conn).await?;
  anyhow::ensure!(rows.len()<=256,"Original draft plan exceeds256");
  let mut drafts=Vec::new();let mut total=0usize;
  for row in rows {
   let id:String=row.try_get("id")?;let doc:String=row.try_get("document_id")?;let space:String=row.try_get("space_id")?;
   let permission=crate::spaces::access::resolve_draft_access(self.conn,crate::spaces::access::principal_from_auth(&auth).map_err(|e|anyhow::anyhow!("Original draft principal refused: {e:?}"))?,&space,&doc,&id,crate::spaces::access::DraftOperation::View,crate::spaces::access::Purpose::Routine).await.map_err(|e|anyhow::anyhow!("Original draft audience cannot certify this bounded producer: {e:?}"))?;
   let access_role=match permission.participation {crate::spaces::access::DraftParticipation::Owner=>"owner",crate::spaces::access::DraftParticipation::SharedEditor=>"editor",crate::spaces::access::DraftParticipation::SharedViewer=>"viewer",crate::spaces::access::DraftParticipation::RecoveryAuditRequired=>anyhow::bail!("Recovery draft audience cannot certify routine read")};
   let raw=crate::spaces::draft_save::selected(self.conn,&space,&doc,&id).await.map_err(|e|anyhow::anyhow!("Original canonical draft selection unavailable: {e:?}"))?;
   let mut expected=serde_json::Map::new();
   for key in ["id","document_id","space_id","owner_id","title","icon","cover_image","content","base_updated_at","created_at","updated_at","previous_folder_id","previous_parent_id"] {
    expected.insert(key.into(),raw.get(key).ok_or_else(||fail("Original canonical draft field absent"))?.clone());
   }
   let source_digest=crate::spaces::draft_save::digest(&raw).map_err(|e|anyhow::anyhow!("Original raw draft digest unavailable: {e:?}"))?;
   expected.insert("source_digest".into(),source_digest.into());expected.insert("owner_name".into(),row.try_get::<String,_>("owner_name")?.into());
   expected.insert("document_title".into(),if row.try_get::<Option<String>,_>("published_at")?.is_some(){row.try_get::<String,_>("document_title")?.into()}else{raw["title"].clone()});
   expected.insert("access_role".into(),access_role.into());expected.insert("can_publish".into(),serde_json::Value::Bool(raw["owner_id"].as_str()==Some(actor.as_str())));
   let expected=serde_json::Value::Object(expected);total=total.checked_add(serde_json::to_vec(&expected)?.len()).ok_or_else(||fail("Draft plan size overflow"))?;anyhow::ensure!(total<=16*1024*1024,"Original draft plan exceeds16MiB");
   drafts.push(OriginalApplicationDraft{uri:format!("/spaces/{}/documents/{}/drafts/{}",urlencoding::encode(&space),urlencoding::encode(&doc),urlencoding::encode(&id)),expected});
  }
  self.fence().await?;Ok(OriginalApplicationDraftPlan{source:self.root,database:identity(self.database)?,actor,session:admin.claims.sid.clone(),drafts})
 }
}
impl OriginalApplicationDraftPlan<'_> {
 fn fence(&self,source:&StoppedVerificationSource<'_>)->anyhow::Result<()> {
  self.source.check()?;let admin=source.admin.ok_or_else(||fail("Original draft authority absent"))?;
  anyhow::ensure!(identity(self.source.fd())?==source.root_identity()?&&self.database==identity(source.database)?&&self.actor==admin.claims.sub&&self.session==admin.claims.sid,"Original draft plan context changed");Ok(())
 }
}
async fn verify_normal_tcp_drafts(source:&mut StoppedVerificationSource<'_>,pages:&OriginalApplicationPlan<'_>,plan:&OriginalApplicationDraftPlan<'_>,address:std::net::SocketAddr,token:&str,channel:&File,nonce:&str)->anyhow::Result<()> {
 let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
 for draft in &plan.drafts {
  source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;
  let(status,actual)=normal_source_fenced_observation(source,pages,channel,nonce,normal_tcp_json(&client,address,Some(token),&draft.uri)).await?;
  source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;
  anyhow::ensure!(status==reqwest::StatusCode::OK&&actual==draft.expected,"Normal TCP canonical draft differs from exact original raw SQL presentation/digest");
  let(status,denied)=normal_source_fenced_observation(source,pages,channel,nonce,normal_tcp_json(&client,address,None,&draft.uri)).await?;
  source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;
  anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Normal TCP anonymous private draft did not deny");
  if let Some(body)=draft.expected["content"].as_str().filter(|s|!s.is_empty()){anyhow::ensure!(!denied.to_string().contains(body),"Anonymous draft response exposed original content");}
 }
 Ok(())
}
