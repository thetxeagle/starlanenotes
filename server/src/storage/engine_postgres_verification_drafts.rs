// Private canonical PG draft plan, derived only from original held SQL/current audience.
struct PgCanonicalDraftPlan{source:Identity,attestation_sha:String,actor:String,session:String,drafts:Vec<OriginalApplicationDraft>}
impl<'a> StoppedPgVerificationSource<'a>{
 async fn canonical_draft_plan(&mut self)->anyhow::Result<PgCanonicalDraftPlan> {
  use sqlx::Row;
  self.fence().await?;let actor=self.current_human().await?;
  let admin=self.admin;
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
  self.fence().await?;Ok(PgCanonicalDraftPlan{source:identity(self.serving.proof.root.fd())?,attestation_sha:digest(&self.serving.proof.raw_attestation),actor,session:admin.claims.sid.clone(),drafts})
 }
}
impl PgCanonicalDraftPlan{fn fence(&self,original:&StoppedPgVerificationSource<'_>)->anyhow::Result<()>{ensure!(self.source==original.root_identity()?&&self.attestation_sha==digest(&original.serving.proof.raw_attestation)&&self.actor==original.admin.claims.sub&&self.session==original.admin.claims.sid,"Original canonical PG draft context differs");original.physical()?;Ok(())}fn verify(&self,uri:&str,value:&serde_json::Value)->anyhow::Result<()>{let expected=self.drafts.iter().find(|v|v.uri==uri).context("Original canonical PG draft absent")?;ensure!(expected.expected==*value,"Original canonical PG draft/body/digest/access presentation differs");Ok(())}}
