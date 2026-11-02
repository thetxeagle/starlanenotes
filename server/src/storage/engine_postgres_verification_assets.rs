// Opaque original PG binary/registry/reference plan, not a copied receipt grant.
struct PgCanonicalAssets{source:Identity,attestation_sha:String,actor:String,session:String,assets:Vec<OriginalApplicationAsset>,versions:Vec<crate::storage::attachment_versions::VersionRecord>,routes:Vec<(usize,String)>,retirements:Vec<serde_json::Value>,legacy:Vec<PgOriginalLegacyAsset>}
impl<'a> StoppedPgVerificationSource<'a>{
 async fn canonical_asset_plan(&mut self,pages:&PgCanonicalPagePlan,drafts:&PgCanonicalDraftPlan)->anyhow::Result<PgCanonicalAssets> {
  use sqlx::Row;use anyhow::{Context,ensure};
  self.fence().await?;pages.fence(self)?;drafts.fence(self)?;let actor=self.current_human().await?;let admin=self.admin;
  let auth=crate::auth::middleware::AuthUser{claims:admin.claims.clone(),api_key_scopes:None};
  let rows=sqlx::query("SELECT a.id,a.space_id,a.document_id,a.storage_path,a.size_bytes FROM attachments a JOIN documents d ON d.id=a.document_id AND d.space_id=a.space_id WHERE d.archived_at IS NULL AND d.deleted_at IS NULL ORDER BY a.space_id,a.document_id,a.id LIMIT 257").fetch_all(&mut *self.conn).await?;
  ensure!(rows.len()<=256,"Original attachment probe exceeds256 assets");let mut assets=Vec::new();let mut versions=Vec::new();let mut routes=Vec::new();let mut retirements=Vec::new();let mut legacy=Vec::new();let mut total=0u64;
  for row in rows {
   let id:String=row.try_get("id")?;let space:String=row.try_get("space_id")?;let page:String=row.try_get("document_id")?;let path:String=row.try_get("storage_path")?;
   crate::spaces::shared_pages::asset_view(self.conn,&auth,&space,&page).await.map_err(|e|anyhow::anyhow!("Original asset page cannot certify: {e:?}"))?;
   let logical=crate::storage::attachment_versions::logical_attachment_id(self.conn,&id).await.map_err(|e|anyhow::anyhow!(e))?;let Some(logical)=logical else{let selected=pg_capture_legacy(self,&auth,&id,&space,&page,&path,row.try_get::<i64,_>("size_bytes")?).await?;total=total.checked_add(selected.length).context("Legacy asset total overflow")?;ensure!(total<=1024*1024*1024,"Original native+legacy byte total exceeds1GiB");legacy.push(selected);continue;};
   let (version,mut binding,registry)=crate::storage::attachment_versions::capture_historical_version(&self.serving.proof.root.absolute,self.conn,&logical,&id).await.map_err(|e|anyhow::anyhow!("Original exact historical version unavailable: {e}"))?;
   let retirement=crate::storage::attachment_retirement::read(self.conn,&id).await.map_err(|e|anyhow::anyhow!(e))?;let retired=retirement.is_some();retirements.push(serde_json::to_value(&retirement)?);
   ensure!(binding.asset_id==id&&binding.space_id==space&&binding.document_id==page&&binding.storage_path==path&&binding.byte_length==row.try_get::<i64,_>("size_bytes")?,"Original asset/version association differs");
   let length=u64::try_from(binding.byte_length)?;total=total.checked_add(length).context("Asset byte overflow")?;ensure!(length<=100*1024*1024&&total<=1024*1024*1024,"Original asset byte coverage exceeds bounds");
   let (file,stamp)=self.files.get(&path).context("Original exact PG asset not inventoried")?;let sha=hash_file(file,stamp.length)?;
   ensure!(stamp.length==length&&sha==binding.binary_sha256,"Original inventoried binary/version hash differs");
   let mut proofs=vec![registry];binding.append_sql_proofs(&mut proofs);binding.fence_files().map_err(|e|anyhow::anyhow!(e))?;
   let index=assets.len();let exact=pg_version_file_route(&space,&page,&version.logical_attachment_id,&id);routes.push((index,exact));if !retired{routes.push((index,format!("/api/v1/files/{}",path.split('/').map(|v|urlencoding::encode(v).into_owned()).collect::<Vec<_>>().join("/"))));}versions.push(version);
   assets.push(OriginalApplicationAsset{id,space,page,path,length,sha:binding.binary_sha256.clone(),held:file.try_clone()?,stamp:stamp.clone(),binding,proofs});
  }
  let mut sources=Vec::new();for (index,page)in pages.pages.iter().enumerate(){sources.push((page.space.clone(),page.id.clone(),page.body.clone(),pages.content[index]["published_metadata"]["cover_image"].as_str().map(str::to_owned)));}
  for draft in &drafts.drafts{let value=&draft.expected;sources.push((value["space_id"].as_str().context("Original draft reference space absent")?.to_owned(),value["document_id"].as_str().context("Original draft reference page absent")?.to_owned(),value["content"].as_str().context("Original draft reference content absent")?.to_owned(),value["cover_image"].as_str().map(str::to_owned)));}
  for (space,page,body,cover)in sources{
   let mut references=original_asset_markdown_destinations(&body)?;if let Some(cover)=cover{ensure!(cover.len()<=8192,"Original cover reference exceeds8KiB");references.push(cover);}
   for destination in references {ensure!(routes.len()<2048,"Original reference route bound exceeded");match pg_internal_reference(&destination)?{
    Some(PgReference::Pin(id))=>{let selected=assets.iter().position(|a|a.id==id&&a.page==page&&a.space==space).context("Exact original pin lacks same-page immutable owner")?;routes.push((selected,pg_version_file_route(&space,&page,&versions[selected].logical_attachment_id,&id)));},
    Some(PgReference::File(path))=>{if legacy.iter().any(|a|a.page==page&&a.space==space&&a.path==path){continue;}let selected=assets.iter().position(|a|a.page==page&&a.space==space&&a.binding.matches_requested_path(&path)).context("Parsed original file destination lacks exact same-page ownership")?;ensure!(crate::storage::attachment_retirement::read(self.conn,&assets[selected].id).await.map_err(|e|anyhow::anyhow!(e))?.is_none(),"Retired exact version requires explicit pinned history reference");routes.push((selected,destination));},None=>{}
   }}
  }
  ensure!(routes.len()<=2048,"Original exact reference routes exceed2048");routes.sort();routes.dedup();
  self.fence().await?;Ok(PgCanonicalAssets{source:identity(self.serving.proof.root.fd())?,attestation_sha:digest(&self.serving.proof.raw_attestation),actor,session:admin.claims.sid.clone(),assets,versions,routes,retirements,legacy})
 }
}
impl PgCanonicalAssets{async fn fence(&self,original:&mut StoppedPgVerificationSource<'_>)->anyhow::Result<()>{original.fence().await?;ensure!(self.source==original.root_identity()?&&self.attestation_sha==digest(&original.serving.proof.raw_attestation)&&self.actor==original.current_human().await?&&self.session==original.admin.claims.sid,"Original PG binary source context differs");for asset in &self.assets{ensure!(physical_stamp(&asset.held)?==asset.stamp,"Original PG binary exchanged");asset.binding.fence_files().map_err(|e|anyhow::anyhow!(e))?;crate::spaces::published_source::verify_source(original.conn,&asset.proofs).await.map_err(|e|anyhow::anyhow!("Original PG exact binary SQL differs: {e:?}"))?;}for asset in &self.legacy{asset.fence(original).await?;}original.fence().await?;Ok(())}}

#[derive(Debug,PartialEq,Eq)]enum PgReference{Pin(String),File(String)}
fn pg_internal_reference(destination:&str)->anyhow::Result<Option<PgReference>>{
 ensure!(destination.len()<=8192,"Original reference exceeds8KiB");
 if let Some(id)=destination.strip_prefix("starlane-attachment://"){ensure!(uuid::Uuid::parse_str(id).ok().is_some_and(|v|v.to_string()==id),"Original pin requires exact canonical immutable UUID");return Ok(Some(PgReference::Pin(id.into())));}
 if let Some(path)=destination.strip_prefix("/api/v1/files/"){ensure!(!path.contains(&['?','#'][..]),"Original exact file reference cannot contain query/fragment");let path=urlencoding::decode(path)?.into_owned();ensure!(valid_relative(&path)&&!path.contains(&['%','?','#'][..]),"Original file reference path unsafe or double encoded");return Ok(Some(PgReference::File(path)));}
 ensure!(!destination.to_ascii_lowercase().contains("starlane-attachment:")&&!destination.contains("/api/v1/files/")&&!destination.contains("/attachment-families/"),"Unsupported internal origin/route cannot certify complete references");Ok(None)
}
fn pg_version_file_route(space:&str,page:&str,logical:&str,version:&str)->String{format!("/api/v1/spaces/{}/documents/{}/attachment-families/{}/versions/{}/file",urlencoding::encode(space),urlencoding::encode(page),urlencoding::encode(logical),urlencoding::encode(version))}
#[cfg(test)]mod pg_exact_reference_controls{use super::*;#[test]fn exact_pins_and_scalar_cover_refuse_ambiguous_routes(){let id="11111111-2222-4333-8444-555555555555";let pin=format!("starlane-attachment://{id}");assert_eq!(pg_internal_reference(&pin).unwrap(),Some(PgReference::Pin(id.into())));for bad in [format!("{pin}suffix"),format!("{pin}?head=latest"),"starlane-attachment://filename.png".into(),"https://foreign.invalid/api/v1/files/spaces/a/b.bin".into(),"/api/v1/files/../foreign".into(),"/api/v1/files/spaces/a/b.bin?latest".into(),"/api/v1/files/%252e%252e/foreign".into()]{assert!(pg_internal_reference(&bad).is_err(),"{bad}");}let body=format!("![exact]({pin})\n\n`{pin}`\n\n{pin}suffix");assert_eq!(original_asset_markdown_destinations(&body).unwrap(),vec![pin]);}}
