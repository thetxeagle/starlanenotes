use anyhow::Context as _;
// Original-source attachment observation only. No copied receipt, path, hash or
// this plan grants target serving, upload, import or verification publication.
struct OriginalApplicationAsset {
 id:String,space:String,page:String,path:String,length:u64,sha:String,
 held:File,stamp:PhysicalStamp,binding:crate::storage::attachment_versions::CurrentVersionBinding,
 proofs:Vec<crate::spaces::published_source::SourceProof>,
}
struct OriginalApplicationAssets<'a> {root:&'a Root,database:Identity,actor:String,session:String,assets:Vec<OriginalApplicationAsset>,versions:Vec<crate::storage::attachment_versions::VersionRecord>,routes:Vec<(usize,String)>,retirements:Vec<serde_json::Value>,legacy:Vec<SqliteOriginalLegacyAsset>}
impl<'a> StoppedVerificationSource<'a>{
 async fn application_asset_plan(&mut self,pages:&OriginalApplicationPlan<'_>)->anyhow::Result<OriginalApplicationAssets<'a>> {
  use sqlx::Row;use anyhow::{Context,ensure};let drafts=self.application_draft_plan().await?;
  self.fence().await?;pages.fence(self)?;drafts.fence(self)?;let actor=self.current_human().await?;let admin=self.admin.context("Original asset human absent")?;
  let auth=crate::auth::middleware::AuthUser{claims:admin.claims.clone(),api_key_scopes:None};
  let rows=sqlx::query("SELECT a.id,a.space_id,a.document_id,a.storage_path,a.size_bytes FROM attachments a JOIN documents d ON d.id=a.document_id AND d.space_id=a.space_id WHERE d.archived_at IS NULL AND d.deleted_at IS NULL ORDER BY a.space_id,a.document_id,a.id LIMIT 257").fetch_all(&mut *self.conn).await?;
  anyhow::ensure!(rows.len()<=256,"Original attachment probe exceeds256 assets");let mut assets=Vec::new();let mut versions=Vec::new();let mut routes=Vec::new();let mut retirements=Vec::new();let mut legacy=Vec::new();let mut total=0u64;
  for row in rows {
   let id:String=row.try_get("id")?;let space:String=row.try_get("space_id")?;let page:String=row.try_get("document_id")?;let path:String=row.try_get("storage_path")?;
   crate::spaces::shared_pages::asset_view(self.conn,&auth,&space,&page).await.map_err(|e|anyhow::anyhow!("Original asset page cannot certify: {e:?}"))?;
   let logical=crate::storage::attachment_versions::logical_attachment_id(self.conn,&id).await.map_err(|e|anyhow::anyhow!(e))?;let Some(logical)=logical else{let selected=sqlite_capture_legacy(self,&auth,&id,&space,&page,&path,row.try_get::<i64,_>("size_bytes")?).await?;total=total.checked_add(selected.length).context("Legacy asset total overflow")?;anyhow::ensure!(total<=1024*1024*1024,"Original native+legacy byte total exceeds1GiB");legacy.push(selected);continue;};
   let (version,mut binding,registry)=crate::storage::attachment_versions::capture_historical_version(&self.root.absolute,self.conn,&logical,&id).await.map_err(|e|anyhow::anyhow!("Original exact historical version unavailable: {e}"))?;
   let retirement=crate::storage::attachment_retirement::read(self.conn,&id).await.map_err(|e|anyhow::anyhow!(e))?;let retired=retirement.is_some();retirements.push(serde_json::to_value(&retirement)?);
   anyhow::ensure!(binding.asset_id==id&&binding.space_id==space&&binding.document_id==page&&binding.storage_path==path&&binding.byte_length==row.try_get::<i64,_>("size_bytes")?,"Original asset/version association differs");
   let length=u64::try_from(binding.byte_length)?;total=total.checked_add(length).context("Asset byte overflow")?;anyhow::ensure!(length<=100*1024*1024&&total<=1024*1024*1024,"Original asset byte coverage exceeds bounds");
   let (file,stamp,inventory_sha)=self.inventory.files.get(&path).context("Original exact SQLite asset not inventoried")?;let sha=hash_file(file,stamp.length)?;anyhow::ensure!(&sha==inventory_sha,"Original held inventory hash differs");
   anyhow::ensure!(stamp.length==length&&sha==binding.binary_sha256,"Original inventoried binary/version hash differs");
   let mut proofs=vec![registry];binding.append_sql_proofs(&mut proofs);binding.fence_files().map_err(|e|anyhow::anyhow!(e))?;
   let index=assets.len();let exact=sqlite_version_file_route(&space,&page,&version.logical_attachment_id,&id);routes.push((index,exact));if !retired{routes.push((index,format!("/api/v1/files/{}",path.split('/').map(|v|urlencoding::encode(v).into_owned()).collect::<Vec<_>>().join("/"))));}versions.push(version);
   assets.push(OriginalApplicationAsset{id,space,page,path,length,sha:binding.binary_sha256.clone(),held:file.try_clone()?,stamp:stamp.clone(),binding,proofs});
  }
  let mut sources=Vec::new();for (index,page)in pages.pages.iter().enumerate(){sources.push((page.space.clone(),page.id.clone(),page.body.clone(),pages.content[index]["published_metadata"]["cover_image"].as_str().map(str::to_owned)));}
  for draft in &drafts.drafts{let value=&draft.expected;sources.push((value["space_id"].as_str().context("Original draft reference space absent")?.to_owned(),value["document_id"].as_str().context("Original draft reference page absent")?.to_owned(),value["content"].as_str().context("Original draft reference content absent")?.to_owned(),value["cover_image"].as_str().map(str::to_owned)));}
  for (space,page,body,cover)in sources{
   let mut references=original_asset_markdown_destinations(&body)?;if let Some(cover)=cover{anyhow::ensure!(cover.len()<=8192,"Original cover reference exceeds8KiB");references.push(cover);}
   for destination in references {anyhow::ensure!(routes.len()<2048,"Original reference route bound exceeded");match sqlite_internal_reference(&destination)?{
    Some(SqliteReference::Pin(id))=>{let selected=assets.iter().position(|a|a.id==id&&a.page==page&&a.space==space).context("Exact original pin lacks same-page immutable owner")?;routes.push((selected,sqlite_version_file_route(&space,&page,&versions[selected].logical_attachment_id,&id)));},
    Some(SqliteReference::File(path))=>{if legacy.iter().any(|a|a.page==page&&a.space==space&&a.path==path){continue;}let selected=assets.iter().position(|a|a.page==page&&a.space==space&&a.binding.matches_requested_path(&path)).context("Parsed original file destination lacks exact same-page ownership")?;anyhow::ensure!(crate::storage::attachment_retirement::read(self.conn,&assets[selected].id).await.map_err(|e|anyhow::anyhow!(e))?.is_none(),"Retired exact version requires explicit pinned history reference");routes.push((selected,destination));},None=>{}
   }}
  }
  anyhow::ensure!(routes.len()<=2048,"Original exact reference routes exceed2048");routes.sort();routes.dedup();
  self.fence().await?;Ok(OriginalApplicationAssets{root:self.root,database:identity(self.database)?,actor,session:admin.claims.sid.clone(),assets,versions,routes,retirements,legacy})
 }
}
impl OriginalApplicationAssets<'_> {
 async fn fence(&self,source:&mut StoppedVerificationSource<'_>)->anyhow::Result<()> {
  source.fence().await?;anyhow::ensure!(std::ptr::eq(self.root,source.root)&&identity(source.database)?==self.database&&source.current_human().await?==self.actor&&source.admin.map(|a|a.claims.sid.as_str())==Some(self.session.as_str()),"Original asset source/context differs");
  for a in &self.assets {anyhow::ensure!(physical_stamp(&a.held)?==a.stamp,"Original exact asset file changed");a.binding.fence_files().map_err(|e|anyhow::anyhow!(e))?;crate::spaces::published_source::verify_source(source.conn,&a.proofs).await.map_err(|e|anyhow::anyhow!("Original asset SQL proof changed: {e:?}"))?;}for a in &self.legacy{a.fence(source).await?;}Ok(())
 }
}

async fn verify_normal_tcp_exact_assets(source:&mut StoppedVerificationSource<'_>,pages:&OriginalApplicationPlan<'_>,assets:&OriginalApplicationAssets<'_>,address:std::net::SocketAddr,token:&str,channel:&File,nonce:&str)->anyhow::Result<()> {
 use sha2::{Digest,Sha256};
 let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
 for (index,route) in &assets.routes {
  let a=&assets.assets[*index];
  assets.fence(source).await?;
  let uri=format!("http://{address}{route}");
  let observation=async {
   let mut response=client.get(&uri).bearer_auth(token).send().await?;
   anyhow::ensure!(response.status()==reqwest::StatusCode::OK,"Actual exact asset route refused original version");
   let mut offset=0u64;let mut hash=Sha256::new();let mut scratch=[0u8;65536];
   while let Some(chunk)=response.chunk().await? {
    anyhow::ensure!(offset.checked_add(chunk.len()as u64).is_some_and(|n|n<=a.length),"Exact asset response exceeds original length");
    for part in chunk.chunks(scratch.len()) {
     anyhow::ensure!(physical_stamp(&a.held)?==a.stamp,"Original byte source changed during actual HTTP stream");a.binding.fence_files().map_err(|e|anyhow::anyhow!(e))?;
     a.held.read_exact_at(&mut scratch[..part.len()],offset)?;anyhow::ensure!(&scratch[..part.len()]==part,"Actual asset bytes differ from original held binary");
     hash.update(part);offset+=part.len()as u64;
    }
   }
   anyhow::ensure!(offset==a.length&&hex::encode(hash.finalize())==a.sha,"Actual exact binary stream length/hash differs");Ok(())
  };
  normal_source_fenced_observation(source,pages,channel,nonce,observation).await?;assets.fence(source).await?;
  let anonymous=async{let response=client.get(&uri).send().await?;anyhow::ensure!(matches!(response.status().as_u16(),401|403|404),"Anonymous original private asset audience was not denied");Ok(())};
  normal_source_fenced_observation(source,pages,channel,nonce,anonymous).await?;assets.fence(source).await?;
 }
 for (index,a) in assets.assets.iter().enumerate(){
  let uri=format!("/spaces/{}/documents/{}/attachment-versions/{}",urlencoding::encode(&a.space),urlencoding::encode(&a.page),urlencoding::encode(&a.id));
  let(status,value)=normal_source_fenced_observation(source,pages,channel,nonce,normal_tcp_json(&client,address,Some(token),&uri)).await?;
  anyhow::ensure!(status==reqwest::StatusCode::OK&&value["version"]==serde_json::to_value(&assets.versions[index])?&&value["current_space_id"]==a.space&&value["current_document_id"]==a.page&&value["current_storage_path"]==a.path&&value["retirement"]==assets.retirements[index],"Original exact historical metadata/retirement differs");assets.fence(source).await?;
  let(status,_)=normal_source_fenced_observation(source,pages,channel,nonce,normal_tcp_json(&client,address,None,&uri)).await?;anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Anonymous private immutable metadata was exposed");assets.fence(source).await?;
 }
 for a in &assets.legacy{normal_source_fenced_observation(source,pages,channel,nonce,sqlite_legacy_download(&client,address,Some(token),a,true)).await?;assets.fence(source).await?;normal_source_fenced_observation(source,pages,channel,nonce,sqlite_legacy_download(&client,address,None,a,false)).await?;assets.fence(source).await?;
  let uri=format!("/spaces/{}/documents/{}/attachment-versions/{}",urlencoding::encode(&a.space),urlencoding::encode(&a.page),urlencoding::encode(&a.id));
  for token in [Some(token),None]{let(status,_)=normal_source_fenced_observation(source,pages,channel,nonce,normal_tcp_json(&client,address,token,&uri)).await?;anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Unregistered legacy ID fabricated immutable metadata");assets.fence(source).await?;}
 }
 Ok(())
}

fn original_asset_markdown_destinations(body:&str)->anyhow::Result<Vec<String>> {
 use pulldown_cmark::{Event,Tag};
 anyhow::ensure!(body.len()<=6_000_000,"Original Markdown reference probe exceeds bound");let mut out=Vec::new();
 for event in pulldown_cmark::Parser::new_ext(body,pulldown_cmark::Options::all()) {match event {
  Event::Start(Tag::Link{dest_url,..}|Tag::Image{dest_url,..})=>{anyhow::ensure!(dest_url.len()<=8192&&out.len()<1024,"Original parsed destination bound exceeded");out.push(dest_url.into_string());},
  Event::Html(raw)|Event::InlineHtml(raw)=>anyhow::ensure!(original_asset_literal_html(&raw),"Opaque HTML cannot certify complete original internal references"),_=>{}
 }}Ok(out)
}
#[cfg(test)]mod verification_exact_asset_reference_tests {
 use super::*;
 #[test]fn parsed_destinations_preserve_exact_pins_ignore_code_prose_and_substrings(){
  let exact="/api/v1/files/spaces/a/attachments/p/asset.bin";let body=format!("![owned]({exact})\n\n`{exact}`\n\n{exact}suffix\n\n```md\n![code]({exact})\n```\n");
  assert_eq!(original_asset_markdown_destinations(&body).unwrap(),vec![exact]);
  assert_eq!(original_asset_markdown_destinations(&format!("[longer]({exact}suffix)")).unwrap(),vec![format!("{exact}suffix")]);
  assert!(original_asset_markdown_destinations(&format!("<img src='{exact}'>")).is_err());
 }
}
#[derive(Debug,PartialEq,Eq)]enum SqliteReference{Pin(String),File(String)}
fn sqlite_internal_reference(destination:&str)->anyhow::Result<Option<SqliteReference>>{
 anyhow::ensure!(destination.len()<=8192,"Original reference exceeds8KiB");
 if let Some(id)=destination.strip_prefix("starlane-attachment://"){anyhow::ensure!(uuid::Uuid::parse_str(id).ok().is_some_and(|v|v.to_string()==id),"Original pin requires exact canonical immutable UUID");return Ok(Some(SqliteReference::Pin(id.into())));}
 if let Some(path)=destination.strip_prefix("/api/v1/files/"){anyhow::ensure!(!path.contains(&['?','#'][..]),"Original exact file reference cannot contain query/fragment");let path=urlencoding::decode(path)?.into_owned();anyhow::ensure!(valid_relative(&path)&&!path.contains(&['%','?','#'][..]),"Original file reference path unsafe or double encoded");return Ok(Some(SqliteReference::File(path)));}
 anyhow::ensure!(!destination.to_ascii_lowercase().contains("starlane-attachment:")&&!destination.contains("/api/v1/files/")&&!destination.contains("/attachment-families/"),"Unsupported internal origin/route cannot certify complete references");Ok(None)
}
fn sqlite_version_file_route(space:&str,page:&str,logical:&str,version:&str)->String{format!("/api/v1/spaces/{}/documents/{}/attachment-families/{}/versions/{}/file",urlencoding::encode(space),urlencoding::encode(page),urlencoding::encode(logical),urlencoding::encode(version))}
#[cfg(test)]mod sqlite_exact_reference_controls{use super::*;#[test]fn exact_pins_and_scalar_cover_refuse_ambiguous_routes(){let id="11111111-2222-4333-8444-555555555555";let pin=format!("starlane-attachment://{id}");assert_eq!(sqlite_internal_reference(&pin).unwrap(),Some(SqliteReference::Pin(id.into())));for bad in [format!("{pin}suffix"),format!("{pin}?head=latest"),"starlane-attachment://filename.png".into(),"https://foreign.invalid/api/v1/files/spaces/a/b.bin".into(),"/api/v1/files/../foreign".into(),"/api/v1/files/spaces/a/b.bin?latest".into(),"/api/v1/files/%252e%252e/foreign".into()]{assert!(sqlite_internal_reference(&bad).is_err(),"{bad}");}let body=format!("![exact]({pin})\n\n`{pin}`\n\n{pin}suffix");assert_eq!(original_asset_markdown_destinations(&body).unwrap(),vec![pin]);}}

include!("restoration_verification_legacy_assets.rs");

// Notes escapes raw markup before formatting. A single inert bare tag contains
// no source/destination attribute or encoded reference and supplies no ownership.
// Resource/active/raw-content/attribute-bearing/encoded/multitag HTML still refuses.
fn original_asset_literal_html(raw:&str)->bool {
 let Some(tag)=raw.trim().strip_prefix('<').and_then(|v|v.strip_suffix('>'))else{return false};
 let tag=tag.strip_prefix('/').unwrap_or(tag);
 if tag.is_empty()||tag.len()>64||!tag.as_bytes()[0].is_ascii_alphabetic()||!tag.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'){return false;}
 !matches!(tag.to_ascii_lowercase().as_str(),"script"|"style"|"iframe"|"object"|"embed"|"svg"|"math"|"base"|"link"|"img"|"video"|"audio"|"source"|"track"|"input"|"textarea"|"template"|"form"|"button"|"select"|"option"|"meta"|"canvas")
}
#[cfg(test)]mod original_literal_html_reference_controls{
 use super::*;
 #[test]fn bare_literal_markup_is_text_without_an_attachment_grant(){
  assert_eq!(original_asset_markdown_destinations("Opaque original text <opaque> ☃").unwrap(),Vec::<String>::new());
  assert!(original_asset_literal_html("<opaque>")&&original_asset_literal_html("</opaque>")&&original_asset_literal_html("<strong>"));
  for raw in ["<img>","<script>","<style>","<svg>","<opaque src='/api/v1/files/spaces/private/file'>","<opaque data-id='starlane-attachment://11111111-2222-4333-8444-555555555555'>","<opaque>&#47;api/v1/files/x</opaque>","<opaque>![hidden](/api/v1/files/x)","<opaque><other>","<op%61que>"] {assert!(!original_asset_literal_html(raw),"{raw}");}
  assert!(original_asset_markdown_destinations("<img src='/api/v1/files/spaces/private/file'>").is_err());
 }
}
