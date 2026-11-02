// Purpose-specific ordinary-main adapters of existing worker probes.
// No worker control frames are sent to the listener-only protocol.
async fn ordinary_sqlite_verify_normal_tcp_pages(original:&mut StoppedVerificationSource<'_>,plan:&OriginalApplicationPlan<'_>,address:std::net::SocketAddr,token:&str,process:&mut std::process::Child)->anyhow::Result<()> {
 let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
 for (index,page) in plan.pages.iter().enumerate() {
  let uri=format!("/spaces/{}/documents/{}",urlencoding::encode(&page.space),urlencoding::encode(&page.id));
  for(suffix,expected)in [("",page.metadata.clone()),("/content",plan.content[index].clone()),("/versions",serde_json::json!(page.history)),("/properties",serde_json::json!(page.properties))]{
   original.fence().await?;plan.fence(original)?;
   let(status,actual)=ordinary_sqlite_live(original,plan,process,normal_tcp_json(&client,address,Some(token),&format!("{uri}{suffix}"))).await?;
   original.fence().await?;plan.fence(original)?;anyhow::ensure!(status==reqwest::StatusCode::OK,"Registered normal TCP route unavailable");
   if suffix.is_empty(){for(k,v)in expected.as_object().ok_or_else(||fail("Malformed internal original metadata"))?{anyhow::ensure!(actual.get(k)==Some(v),"Normal TCP original metadata differs");}}
   else if suffix=="/content"{anyhow::ensure!(actual==expected,"Normal TCP exact body differs");}
   else{anyhow::ensure!(actual==expected,"Normal TCP original history/property presentation differs");}
   let(status,_) = ordinary_sqlite_live(original,plan,process,normal_tcp_json(&client,address,None,&format!("{uri}{suffix}"))).await?;
   original.fence().await?;plan.fence(original)?;anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Normal TCP anonymous private audience did not deny");
  }
 }
 Ok(())
}

async fn ordinary_sqlite_verify_normal_tcp_rebuilt_search(source:&mut StoppedVerificationSource<'_>,plan:&OriginalApplicationPlan<'_>,address:std::net::SocketAddr,token:&str,process:&mut std::process::Child)->anyhow::Result<()> {
 let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
 let mut queries=Vec::new();
 for page in &plan.pages {
  // Require a meaningful original body token; do not replace a content-index
  // probe with an unindexed SQL listing or filename/head resolution.
  let word=page.body.split(|c:char|!c.is_ascii_alphanumeric()).find(|w|w.len()>=8&&w.len()<=100).ok_or_else(||fail("Original page lacks a bounded literal search body sample"))?;
  let uri=format!("/search?q={}&space_id={}&limit=100",urlencoding::encode(word),urlencoding::encode(&page.space));
  source.fence().await?;plan.fence(source)?;
  let(status,empty)=ordinary_sqlite_live(source,plan,process,normal_tcp_json(&client,address,Some(token),&uri)).await?;
  source.fence().await?;plan.fence(source)?;
  anyhow::ensure!(status==reqwest::StatusCode::OK&&empty.as_array().is_some_and(|r|r.is_empty()),"Genuinely new derived index must be empty before registered rebuild");
  queries.push((uri,page.id.clone(),page.space.clone(),word.to_owned()));
 }
 source.fence().await?;plan.fence(source)?;
 let rebuilt=ordinary_sqlite_live(source,plan,process,async {
 let mut response=client.post(format!("http://{address}/api/v1/admin/search/reindex")).bearer_auth(token).send().await?;
 anyhow::ensure!(response.status()==reqwest::StatusCode::OK,"Actual registered normal search rebuild refused");
 let mut bytes=Vec::new();while let Some(chunk)=response.chunk().await?{anyhow::ensure!(bytes.len().checked_add(chunk.len()).is_some_and(|n|n<=65536),"Registered rebuild response exceeds bound");bytes.extend_from_slice(&chunk);}let rebuilt:serde_json::Value=serde_json::from_slice(&bytes)?;Ok(rebuilt)
 }).await?;
 source.fence().await?;plan.fence(source)?;
 anyhow::ensure!(rebuilt["status"]=="reindex_complete"&&rebuilt["documents_indexed"].as_u64()==Some(plan.pages.len()as u64),"Normal rebuilt index coverage differs from original canonical published pages");
 for(uri,id,space,word)in queries {
  let(status,found)=ordinary_sqlite_live(source,plan,process,normal_tcp_json(&client,address,Some(token),&uri)).await?;
  source.fence().await?;plan.fence(source)?;
  anyhow::ensure!(status==reqwest::StatusCode::OK&&found.as_array().is_some_and(|hits|hits.iter().any(|h|h["document_id"].as_str()==Some(id.as_str())&&h["space_id"].as_str()==Some(space.as_str())&&h["excerpt"].as_str().is_some_and(|s|s.contains(&word)))),"Rebuilt normal search lost original body/page binding");
  let(status,_)=ordinary_sqlite_live(source,plan,process,normal_tcp_json(&client,address,None,&uri)).await?;
  source.fence().await?;plan.fence(source)?;
  anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Normal anonymous private search audience did not deny");
 }
 Ok(())
}

async fn ordinary_sqlite_verify_normal_tcp_drafts(source:&mut StoppedVerificationSource<'_>,pages:&OriginalApplicationPlan<'_>,plan:&OriginalApplicationDraftPlan<'_>,address:std::net::SocketAddr,token:&str,process:&mut std::process::Child)->anyhow::Result<()> {
 let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
 for draft in &plan.drafts {
  source.fence().await?;plan.fence(source)?;
  let(status,actual)=ordinary_sqlite_live(source,pages,process,normal_tcp_json(&client,address,Some(token),&draft.uri)).await?;
  source.fence().await?;plan.fence(source)?;
  anyhow::ensure!(status==reqwest::StatusCode::OK&&actual==draft.expected,"Normal TCP canonical draft differs from exact original raw SQL presentation/digest");
  let(status,denied)=ordinary_sqlite_live(source,pages,process,normal_tcp_json(&client,address,None,&draft.uri)).await?;
  source.fence().await?;plan.fence(source)?;
  anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Normal TCP anonymous private draft did not deny");
  if let Some(body)=draft.expected["content"].as_str().filter(|s|!s.is_empty()){anyhow::ensure!(!denied.to_string().contains(body),"Anonymous draft response exposed original content");}
 }
 Ok(())
}

async fn ordinary_sqlite_verify_normal_tcp_exact_assets(source:&mut StoppedVerificationSource<'_>,pages:&OriginalApplicationPlan<'_>,assets:&OriginalApplicationAssets<'_>,address:std::net::SocketAddr,token:&str,process:&mut std::process::Child)->anyhow::Result<()> {
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
  ordinary_sqlite_live(source,pages,process,observation).await?;assets.fence(source).await?;
  let anonymous=async{let response=client.get(&uri).send().await?;anyhow::ensure!(matches!(response.status().as_u16(),401|403|404),"Anonymous original private asset audience was not denied");Ok(())};
  ordinary_sqlite_live(source,pages,process,anonymous).await?;assets.fence(source).await?;
 }
 for (index,a) in assets.assets.iter().enumerate(){
  let uri=format!("/spaces/{}/documents/{}/attachment-versions/{}",urlencoding::encode(&a.space),urlencoding::encode(&a.page),urlencoding::encode(&a.id));
  let(status,value)=ordinary_sqlite_live(source,pages,process,normal_tcp_json(&client,address,Some(token),&uri)).await?;
  anyhow::ensure!(status==reqwest::StatusCode::OK&&value["version"]==serde_json::to_value(&assets.versions[index])?&&value["current_space_id"]==a.space&&value["current_document_id"]==a.page&&value["current_storage_path"]==a.path&&value["retirement"]==assets.retirements[index],"Original exact historical metadata/retirement differs");assets.fence(source).await?;
  let(status,_)=ordinary_sqlite_live(source,pages,process,normal_tcp_json(&client,address,None,&uri)).await?;anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Anonymous private immutable metadata was exposed");assets.fence(source).await?;
 }
 for a in &assets.legacy{ordinary_sqlite_live(source,pages,process,sqlite_legacy_download(&client,address,Some(token),a,true)).await?;assets.fence(source).await?;ordinary_sqlite_live(source,pages,process,sqlite_legacy_download(&client,address,None,a,false)).await?;assets.fence(source).await?;
  let uri=format!("/spaces/{}/documents/{}/attachment-versions/{}",urlencoding::encode(&a.space),urlencoding::encode(&a.page),urlencoding::encode(&a.id));
  for token in [Some(token),None]{let(status,_)=ordinary_sqlite_live(source,pages,process,normal_tcp_json(&client,address,token,&uri)).await?;anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Unregistered legacy ID fabricated immutable metadata");assets.fence(source).await?;}
 }
 Ok(())
}

async fn ordinary_sqlite_normal_audience_observation<T>(
    source: &mut StoppedVerificationSource<'_>, pages: &OriginalApplicationPlan<'_>, drafts: &OriginalApplicationDraftPlan<'_>,
    audiences: &OriginalApplicationAudiences<'_>, assets: &OriginalApplicationAssets<'_>, process: &mut std::process::Child,
    work: impl std::future::Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    audiences.fence(source, pages, drafts).await?; assets.fence(source).await?;
    tokio::pin!(work);
    let until = tokio::time::sleep(std::time::Duration::from_secs(120)); tokio::pin!(until);
    let mut pulse = tokio::time::interval(std::time::Duration::from_secs(3)); pulse.tick().await;
    loop {
        tokio::select! {
            result = &mut work => {
                audiences.fence(source, pages, drafts).await?; assets.fence(source).await?;
                anyhow::ensure!(process.try_wait()?.is_none(), "Actual normal process exited during audience observation");
                 return result;
            },
            _ = &mut until => anyhow::bail!("Original sampled audience observation exceeded120s"),
            _ = pulse.tick() => {

                audiences.fence(source, pages, drafts).await?; assets.fence(source).await?;
                anyhow::ensure!(process.try_wait()?.is_none(), "Actual normal process exited while sampled audience observation pending");
                
            }
        }
    }
}

async fn ordinary_sqlite_verify_normal_tcp_audiences(
    source: &mut StoppedVerificationSource<'_>, pages: &OriginalApplicationPlan<'_>, drafts: &OriginalApplicationDraftPlan<'_>,
    assets: &OriginalApplicationAssets<'_>, audiences: &OriginalApplicationAudiences<'_>, process: &mut std::process::Child,
    address: std::net::SocketAddr,
) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
    let private_words = normal_original_private_words(pages, drafts)?;
    for sample in &audiences.samples {
        let token = &sample.credential.bearer;
        let (status, me) = ordinary_sqlite_normal_audience_observation(source, pages, drafts, audiences, assets, process,
            normal_tcp_json(&client, address, Some(token), "/auth/me")).await?;
        anyhow::ensure!(status == reqwest::StatusCode::OK && me["id"] == sample.credential.auth.claims.sub,
            "Actual restored sampled authentication identity differs");
        for (index, page) in pages.pages.iter().enumerate() {
            let prefix = format!("/spaces/{}/documents/{}", urlencoding::encode(&page.space), urlencoding::encode(&page.id));
            let (status, metadata) = ordinary_sqlite_normal_audience_observation(source, pages, drafts, audiences, assets, process,
                normal_tcp_json(&client, address, Some(token), &prefix)).await?;
            if let Some(expected) = &sample.expected.metadata[index] {
                anyhow::ensure!(status == reqwest::StatusCode::OK, "Actual permitted sampled page metadata refused");
                for (key, value) in expected.as_object().ok_or_else(|| fail("Original metadata plan malformed"))? {
                    anyhow::ensure!(metadata.get(key) == Some(value), "Actual sampled page metadata differs: {key}");
                }
                anyhow::ensure!(!["draft_id", "content", "source_digest"].iter().any(|key| metadata.get(*key).is_some()),
                    "Published sampled metadata exposed a private draft projection");
            } else { anyhow::ensure!(matches!(status.as_u16(), 401 | 403 | 404), "Source-denied sampled page metadata was exposed"); }
            for (suffix, expected) in [("content", pages.content[index].clone()),
                ("versions", serde_json::json!(page.history)), ("properties", serde_json::json!(page.properties))] {
                let (status, actual) = ordinary_sqlite_normal_audience_observation(source, pages, drafts, audiences, assets, process,
                    normal_tcp_json(&client, address, Some(token), &format!("{prefix}/{suffix}"))).await?;
                if if suffix == "content" { sample.expected.published_pages[index] } else { sample.expected.pages[index] } {
                    anyhow::ensure!(status == reqwest::StatusCode::OK, "Actual permitted sampled page route refused");
                    if suffix == "content" { anyhow::ensure!(actual == expected, "Sampled original page bytes differ"); }
                    else { anyhow::ensure!(actual == expected, "Sampled original history or properties differ"); }
                } else { anyhow::ensure!(matches!(status.as_u16(), 401 | 403 | 404), "Source-denied sampled private page was exposed"); }
            }
            let word = page.body.split(|c: char| !c.is_ascii_alphanumeric()).find(|w| w.len() >= 8 && w.len() <= 100)
                .ok_or_else(|| fail("Original sampled body search token absent"))?;
            for scoped in [true, false] {
                let uri = if scoped { format!("/search?q={}&space_id={}&limit=100", urlencoding::encode(word), urlencoding::encode(&page.space)) }
                    else { format!("/search?q={}&limit=100", urlencoding::encode(word)) };
                let (status, actual) = ordinary_sqlite_normal_audience_observation(source, pages, drafts, audiences, assets, process,
                    normal_tcp_json(&client, address, Some(token), &uri)).await?;
                if if scoped { sample.expected.searches[index] } else { sample.expected.global_read } {
                    anyhow::ensure!(status == reqwest::StatusCode::OK, "Actual permitted sampled search refused");
                    verification_sample_search(&actual, sample, pages, index, word, scoped)?;
                } else { anyhow::ensure!(matches!(status.as_u16(), 401 | 403 | 404), "Source-denied sampled search exposed results"); }
            }
        }
        for (index, draft) in drafts.drafts.iter().enumerate() {
            let (status, actual) = ordinary_sqlite_normal_audience_observation(source, pages, drafts, audiences, assets, process,
                normal_tcp_json(&client, address, Some(token), &draft.uri)).await?;
            if let Some((role, publish)) = sample.expected.drafts[index] {
                let mut expected = draft.expected.clone(); expected["access_role"] = role.into(); expected["can_publish"] = publish.into();
                anyhow::ensure!(status == reqwest::StatusCode::OK && actual == expected, "Sampled exact private draft participation differs");
            } else { anyhow::ensure!(matches!(status.as_u16(), 401 | 403 | 404), "Sampled user accessed an original private draft"); }
        }
        for word in &private_words {
            ordinary_sqlite_normal_audience_observation(source, pages, drafts, audiences, assets, process,
                normal_private_default_search(&client, address, token, word, sample.expected.global_read)).await?;
        }
        for (asset_index,asset) in assets.assets.iter().enumerate() {
            let index = pages.pages.iter().position(|page| page.id == asset.page && page.space == asset.space)
                .ok_or_else(|| fail("Sampled attachment page absent from original plan"))?;
            let version=&assets.versions[asset_index];
            let metadata_uri=format!("/spaces/{}/documents/{}/attachment-versions/{}",urlencoding::encode(&asset.space),urlencoding::encode(&asset.page),urlencoding::encode(&asset.id));
            let(status,metadata)=ordinary_sqlite_normal_audience_observation(source,pages,drafts,audiences,assets,process,normal_tcp_json(&client,address,Some(token),&metadata_uri)).await?;
            if sample.expected.published_pages[index]{anyhow::ensure!(status==reqwest::StatusCode::OK&&metadata["version"]==serde_json::to_value(version)?&&metadata["current_space_id"]==asset.space&&metadata["current_document_id"]==asset.page&&metadata["current_storage_path"]==asset.path&&metadata["retirement"]==assets.retirements[asset_index],"Sampled exact historical metadata/retirement differs");}else{anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Sampled nonmember obtained immutable metadata");}
            let uri=format!("http://{address}{}",sqlite_version_file_route(&asset.space,&asset.page,&version.logical_attachment_id,&asset.id));
            ordinary_sqlite_normal_audience_observation(source, pages, drafts, audiences, assets, process, async {
                let mut response = client.get(&uri).bearer_auth(token).send().await?;
                if !sample.expected.published_pages[index] {
                    anyhow::ensure!(matches!(response.status().as_u16(), 401 | 403 | 404), "Sampled nonmember retrieved a private attachment"); return Ok(());
                }
                anyhow::ensure!(response.status() == reqwest::StatusCode::OK, "Sampled permitted exact attachment refused");
                let mut offset = 0u64; let mut hash = Sha256::new(); let mut scratch = [0u8; 65536];
                while let Some(chunk) = response.chunk().await? {
                    anyhow::ensure!(offset.checked_add(chunk.len() as u64).is_some_and(|n| n <= asset.length), "Sampled attachment exceeds original length");
                    for part in chunk.chunks(scratch.len()) {
                        anyhow::ensure!(physical_stamp(&asset.held)? == asset.stamp, "Original sampled attachment changed");
                        asset.binding.fence_files().map_err(|e| anyhow::anyhow!(e))?;
                        asset.held.read_exact_at(&mut scratch[..part.len()], offset)?;
                        anyhow::ensure!(part == &scratch[..part.len()], "Sampled attachment bytes differ from held original");
                        hash.update(part); offset += part.len() as u64;
                    }
                }
                anyhow::ensure!(offset == asset.length && hex::encode(hash.finalize()) == asset.sha, "Sampled exact attachment SHA/length differs"); Ok(())
            }).await?;
        }
        for asset in &assets.legacy {
            let index=pages.pages.iter().position(|page|page.id==asset.page&&page.space==asset.space).ok_or_else(||fail("Legacy sampled page lacks original published plan; explicit draft-only policy qualification required"))?;
            ordinary_sqlite_normal_audience_observation(source,pages,drafts,audiences,assets,process,sqlite_legacy_download(&client,address,Some(token),asset,sample.expected.published_pages[index])).await?;
            let uri=format!("/spaces/{}/documents/{}/attachment-versions/{}",urlencoding::encode(&asset.space),urlencoding::encode(&asset.page),urlencoding::encode(&asset.id));
            let(status,_)=ordinary_sqlite_normal_audience_observation(source,pages,drafts,audiences,assets,process,normal_tcp_json(&client,address,Some(token),&uri)).await?;anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Sampled legacy ID fabricated immutable metadata");
        }
    }
    audiences.fence(source, pages, drafts).await?; assets.fence(source).await?; Ok(())
}

async fn ordinary_sqlite_verify_normal_owner_private_search(source: &mut StoppedVerificationSource<'_>,
    pages: &OriginalApplicationPlan<'_>, drafts: &OriginalApplicationDraftPlan<'_>, address: std::net::SocketAddr,
    token: &str, process: &mut std::process::Child) -> anyhow::Result<usize> {
    source.fence().await?; pages.fence(source)?; drafts.fence(source)?;
    let admin = source.admin.ok_or_else(|| fail("Original private-search human absent"))?;
    let auth = crate::auth::middleware::AuthUser { claims: admin.claims.clone(), api_key_scopes: None };
    let permitted = verification_expected_access(crate::spaces::access::require_current_human_read(source.conn, &auth).await)?.is_some();
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
    let words = normal_original_private_words(pages, drafts)?;
    for word in &words {
        ordinary_sqlite_live(source, pages, process,
            normal_private_default_search(&client, address, token, word, permitted)).await?;
        drafts.fence(source)?;
    }
    source.fence().await?; Ok(words.len())
}

async fn ordinary_sqlite_live<T>(source:&mut StoppedVerificationSource<'_>,pages:&OriginalApplicationPlan<'_>,process:&mut std::process::Child,work:impl std::future::Future<Output=anyhow::Result<T>>)->anyhow::Result<T>{
    source.fence().await?;pages.fence(source)?;anyhow::ensure!(process.try_wait()?.is_none(),"Actual ordinary child exited before observation");
    tokio::pin!(work);let deadline=tokio::time::sleep(std::time::Duration::from_secs(120));tokio::pin!(deadline);
    let mut pulse=tokio::time::interval(std::time::Duration::from_secs(3));pulse.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);pulse.tick().await;
    loop{tokio::select!{result=&mut work=>{source.fence().await?;pages.fence(source)?;anyhow::ensure!(process.try_wait()?.is_none(),"Actual ordinary child exited during observation");return result;},_=&mut deadline=>anyhow::bail!("Ordinary source-fenced observation exceeded120s"),_=pulse.tick()=>{source.fence().await?;pages.fence(source)?;anyhow::ensure!(process.try_wait()?.is_none(),"Actual ordinary child exited while observation pending");}}}
}
struct OrdinarySqliteSockets {root:Identity,database:Identity,actor:String,session:String,pages:Vec<(String,Vec<u8>)>,owner:bool,samples:Vec<Vec<bool>>}
impl OrdinarySqliteSockets {
    async fn capture(source:&mut StoppedVerificationSource<'_>,pages:&OriginalApplicationPlan<'_>,audiences:&OriginalApplicationAudiences<'_>)->anyhow::Result<Self>{
        source.fence().await?;pages.fence(source)?;
        let admin=source.admin.ok_or_else(||fail("Original socket human absent"))?;
        let auth=crate::auth::middleware::AuthUser{claims:admin.claims.clone(),api_key_scopes:None};
        let owner=verification_expected_access(crate::spaces::access::require_current_human_member_read(source.conn,&auth).await)?.is_some();
        let mut selected=Vec::new();let mut total=0usize;
        for page in &pages.pages {
            let row:Option<(String,Option<Vec<u8>>)>=sqlx::query_as("SELECT space_id,automerge_state FROM documents WHERE id=$1 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL").bind(&page.id).fetch_optional(&mut *source.conn).await?;
            let(space,bytes)=row.ok_or_else(||fail("Original socket page unavailable"))?;anyhow::ensure!(space==page.space,"Original socket source space differs");let bytes=bytes.unwrap_or_default();total=total.checked_add(bytes.len()).ok_or_else(||fail("Original socket aggregate overflow"))?;anyhow::ensure!(bytes.len()<=8*1024*1024&&total<=16*1024*1024,"Original socket body exceeds bounds");selected.push((page.id.clone(),bytes));
        }
        let mut samples=Vec::new();
        for sample in &audiences.samples {anyhow::ensure!(sample.expected.pages.len()==selected.len(),"Original socket audience coverage differs");let member=verification_expected_access(crate::spaces::access::require_current_human_member_read(source.conn,&sample.credential.auth).await)?.is_some();samples.push(sample.expected.pages.iter().map(|p|member&&*p).collect());}
        let result=Self{root:source.root_identity()?,database:identity(source.database)?,actor:admin.claims.sub.clone(),session:admin.claims.sid.clone(),pages:selected,owner,samples};source.fence().await?;Ok(result)
    }
    async fn verify(&self,source:&mut StoppedVerificationSource<'_>,pages:&OriginalApplicationPlan<'_>,drafts:&OriginalApplicationDraftPlan<'_>,assets:&OriginalApplicationAssets<'_>,audiences:&OriginalApplicationAudiences<'_>,process:&mut std::process::Child,address:std::net::SocketAddr,token:&str)->anyhow::Result<()>{
        anyhow::ensure!(source.root_identity()?==self.root&&identity(source.database)?==self.database&&source.admin.is_some_and(|a|a.claims.sub==self.actor&&a.claims.sid==self.session)&&self.samples.len()==audiences.samples.len(),"Original socket observation context changed");
        for(index,(id,expected))in self.pages.iter().enumerate(){
            let actual=ordinary_sqlite_normal_audience_observation(source,pages,drafts,audiences,assets,process,engine_transfer::sqlite_ordinary_socket(address,Some(token),Some(&self.actor),id,self.owner)).await?;
            if self.owner{anyhow::ensure!(actual.as_deref()==Some(expected.as_slice()),"Ordinary socket owner opaque bytes differ");}
            for(sample,allowed)in audiences.samples.iter().zip(&self.samples){let actual=ordinary_sqlite_normal_audience_observation(source,pages,drafts,audiences,assets,process,engine_transfer::sqlite_ordinary_socket(address,Some(&sample.credential.bearer),Some(&sample.credential.auth.claims.sub),id,allowed[index])).await?;if allowed[index]{anyhow::ensure!(actual.as_deref()==Some(expected.as_slice()),"Ordinary socket sample opaque bytes differ");}}
            ordinary_sqlite_normal_audience_observation(source,pages,drafts,audiences,assets,process,engine_transfer::sqlite_ordinary_socket(address,None,None,id,false)).await?;
        }
        audiences.fence(source,pages,drafts).await?;source.fence().await?;Ok(())
    }
}
