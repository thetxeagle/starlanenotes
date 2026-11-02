// Supplied sessions are observational samples, never producer authority.
// Neither a serialized access label nor a copied target session constructs this plan.
struct VerificationSuppliedAudience {
    auth: crate::auth::middleware::AuthUser,
    bearer: String,
}
impl VerificationSuppliedAudience {
    fn supplied(token: &str, secret: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(secret.len() >= 32 && !token.is_empty() && token.len() <= 8192
            && !token.bytes().any(|b| b.is_ascii_whitespace()), "Invalid bounded audience credential");
        let claims = crate::auth::jwt::validate_token(token, secret, crate::auth::jwt::TokenType::Access)
            .map_err(|_| fail("Supplied audience signature invalid"))?;
        let auth = crate::auth::middleware::AuthUser { claims, api_key_scopes: None };
        crate::spaces::access::principal_from_auth(&auth)
            .map_err(|_| fail("Audience requires a genuine human access session"))?;
        Ok(Self { auth, bearer: token.into() })
    }
}

#[derive(PartialEq)]
struct VerificationAudienceDecisions {
    pages: Vec<bool>,
    published_pages: Vec<bool>,
    metadata: Vec<Option<serde_json::Value>>,
    searches: Vec<bool>,
    global_read: bool,
    drafts: Vec<Option<(&'static str, bool)>>,
}
struct VerificationAudienceSample {
    credential: VerificationSuppliedAudience,
    expected: VerificationAudienceDecisions,
}
struct OriginalApplicationAudiences<'a> {
    root: &'a Root,
    database: Identity,
    actor: String,
    session: String,
    samples: Vec<VerificationAudienceSample>,
}
fn verification_expected_access<T>(result: Result<T, crate::auth::routes::AppError>) -> anyhow::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(crate::auth::routes::AppError::Unauthorized(_)
            | crate::auth::routes::AppError::Forbidden(_)
            | crate::auth::routes::AppError::NotFound(_)) => Ok(None),
        Err(error) => anyhow::bail!("Original audience policy unavailable: {error:?}"),
    }
}

async fn verification_audience_decisions(
    conn: &mut sqlx::AnyConnection,
    credential: &VerificationSuppliedAudience,
    pages: &OriginalApplicationPlan<'_>,
    drafts: &OriginalApplicationDraftPlan<'_>,
) -> anyhow::Result<VerificationAudienceDecisions> {
    use crate::spaces::access as access;
    let auth = &credential.auth;
    let actor = access::require_current_human_identity(conn, auth).await
        .map_err(|_| fail("Original sampled session is no longer current"))?;
    let principal = || access::principal_from_auth(auth)
        .map_err(|_| fail("Original sampled human access session expired"));
    let global_read = verification_expected_access(access::require_current_human_read(conn, auth).await)?.is_some();
    let mut allowed = Vec::new();
    let mut published_pages = Vec::new();
    let mut metadata = Vec::new();
    let mut searches = Vec::new();
    for page in &pages.pages {
        allowed.push(verification_expected_access(access::resolve_page_access(
            conn, principal()?, &page.space, &page.id, access::PageOperation::View, access::Purpose::Routine,
        ).await)?.is_some());
        let space = verification_expected_access(access::resolve_space_access(
            conn, principal()?, &page.space, access::Purpose::Routine,
        ).await)?;
        searches.push(global_read && space.is_some_and(|s| s.allows(access::Permission::View)));
        let published = verification_expected_access(crate::spaces::shared_pages::published_view(
            conn, auth, &page.space, &page.id,
        ).await)?;
        published_pages.push(published.is_some());
        let expected = if let Some(published) = published {
            let direct = published.direct_viewer_grant.is_some();
            let inspect = direct || verification_expected_access(access::resolve_page_access(
                conn, principal()?, &page.space, &page.id,
                access::PageOperation::InspectMetadata, access::Purpose::Routine,
            ).await)?.is_some();
            if inspect {
                let row = crate::spaces::published_source::selected(conn, &page.space, &page.id).await
                    .map_err(|_| fail("Original sampled published metadata unavailable"))?;
                let mut value = serde_json::Map::new();
                for key in ["id", "space_id", "folder_id", "parent_id", "title", "icon", "cover_image",
                    "locked_by", "public_slug", "created_by", "last_updated_by", "created_at", "updated_at",
                    "published_at", "human_identifier", "maintenance_user_id", "maintenance_group_id"] {
                    value.insert(key.into(), row.get(key).ok_or_else(|| fail("Original metadata column absent"))?.clone());
                }
                for key in ["public", "locked"] {
                    let raw = row.get(key).ok_or_else(|| fail("Original metadata flag absent"))?;
                    let flag = if let Some(v) = raw.as_bool() { v } else {
                        match raw.as_i64() { Some(0) => false, Some(1) => true,
                            _ => anyhow::bail!("Original metadata flag is not canonical") }
                    };
                    value.insert(key.into(), flag.into());
                }
                if direct {
                    for key in ["folder_id", "parent_id", "maintenance_user_id", "maintenance_group_id", "locked_by"] {
                        value.insert(key.into(), serde_json::Value::Null);
                    }
                    for key in ["created_by", "last_updated_by"] { value.insert(key.into(), "".into()); }
                    value.insert("page_access_source".into(), "direct_page_grant".into());
                }
                let identifier = if direct { false } else {
                    crate::spaces::page_metadata::edit_allowed(conn, auth, &page.space, &page.id).await
                        .map_err(|_| fail("Original sampled identifier capability unavailable"))?
                };
                let maintenance = if direct { false } else {
                    crate::spaces::maintenance::edit_allowed(conn, auth, &page.space, &page.id).await
                        .map_err(|_| fail("Original sampled maintenance capability unavailable"))?
                };
                value.insert("identifier_edit_allowed".into(), identifier.into());
                value.insert("maintenance_edit_allowed".into(), maintenance.into());
                Some(serde_json::Value::Object(value))
            } else { None }
        } else { None };
        metadata.push(expected);
    }
    let mut selected_drafts = Vec::new();
    for draft in &drafts.drafts {
        let raw = &draft.expected;
        let space = raw["space_id"].as_str().ok_or_else(|| fail("Original draft space absent"))?;
        let page = raw["document_id"].as_str().ok_or_else(|| fail("Original draft page absent"))?;
        let id = raw["id"].as_str().ok_or_else(|| fail("Original draft identity absent"))?;
        let access = verification_expected_access(access::resolve_draft_access(
            conn, principal()?, space, page, id, access::DraftOperation::View, access::Purpose::Routine,
        ).await)?;
        selected_drafts.push(match access {
            None => None,
            Some(access) => Some((match access.participation {
                access::DraftParticipation::Owner => "owner",
                access::DraftParticipation::SharedEditor => "editor",
                access::DraftParticipation::SharedViewer => "viewer",
                access::DraftParticipation::RecoveryAuditRequired => anyhow::bail!("Recovery-only draft access cannot certify a routine audience"),
            }, raw["owner_id"].as_str() == Some(actor.as_str()))),
        });
    }
    Ok(VerificationAudienceDecisions { pages: allowed, published_pages, metadata, searches, global_read, drafts: selected_drafts })
}

impl<'a> StoppedVerificationSource<'a> {
    async fn application_audience_plan(
        &mut self, pages: &OriginalApplicationPlan<'_>, drafts: &OriginalApplicationDraftPlan<'_>,
        supplied: Vec<VerificationSuppliedAudience>,
    ) -> anyhow::Result<OriginalApplicationAudiences<'a>> {
        self.fence().await?; pages.fence(self)?; drafts.fence(self)?;
        anyhow::ensure!(!supplied.is_empty() && supplied.len() <= 8, "Supply1..8 distinct original audience samples");
        let actor = self.current_human().await?;
        let session = self.admin.ok_or_else(|| fail("Original producer absent"))?.claims.sid.clone();
        let mut seen = std::collections::BTreeSet::new();
        let mut samples = Vec::new();
        for credential in supplied {
            let id = crate::spaces::access::require_current_human_identity(self.conn, &credential.auth).await
                .map_err(|_| fail("Original sampled session unavailable"))?;
            anyhow::ensure!(id != actor && seen.insert(id), "Audience samples must be distinct from producer and each other");
            let expected = verification_audience_decisions(self.conn, &credential, pages, drafts).await?;
            samples.push(VerificationAudienceSample { credential, expected });
        }
        let plan = OriginalApplicationAudiences { root: self.root, database: identity(self.database)?, actor, session, samples };
        plan.fence(self, pages, drafts).await?; Ok(plan)
    }
}
impl OriginalApplicationAudiences<'_> {
    async fn fence(&self, source: &mut StoppedVerificationSource<'_>, pages: &OriginalApplicationPlan<'_>, drafts: &OriginalApplicationDraftPlan<'_>) -> anyhow::Result<()> {
        source.fence().await?; pages.fence(source)?; drafts.fence(source)?;
        let admin = source.admin.ok_or_else(|| fail("Original producer absent"))?;
        anyhow::ensure!(identity(self.root.fd())? == source.root_identity()? && self.database == identity(source.database)?
            && self.actor == admin.claims.sub && self.session == admin.claims.sid, "Audience plan belongs to another source or producer");
        for sample in &self.samples {
            anyhow::ensure!(verification_audience_decisions(source.conn, &sample.credential, pages, drafts).await? == sample.expected,
                "Original sampled permissions or metadata changed");
        }
        source.fence().await?; pages.fence(source)?; drafts.fence(source)?; Ok(())
    }
}

async fn normal_audience_observation<T>(
    source: &mut StoppedVerificationSource<'_>, pages: &OriginalApplicationPlan<'_>, drafts: &OriginalApplicationDraftPlan<'_>,
    audiences: &OriginalApplicationAudiences<'_>, assets: &OriginalApplicationAssets<'_>, process: &mut std::process::Child,
    channel: &File, nonce: &str, work: impl std::future::Future<Output = anyhow::Result<T>>,
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
                normal_parent_progress(channel, nonce)?; return result;
            },
            _ = &mut until => anyhow::bail!("Original sampled audience observation exceeded120s"),
            _ = pulse.tick() => {
                #[cfg(test)] normal_pending_audience_change(source, audiences).await?;
                audiences.fence(source, pages, drafts).await?; assets.fence(source).await?;
                anyhow::ensure!(process.try_wait()?.is_none(), "Actual normal process exited while sampled audience observation pending");
                normal_parent_progress(channel, nonce)?;
            }
        }
    }
}

fn verification_sample_search(actual: &serde_json::Value, sample: &VerificationAudienceSample,
    pages: &OriginalApplicationPlan<'_>, index: usize, word: &str, scoped: bool) -> anyhow::Result<()> {
    let rows = actual.as_array().ok_or_else(|| fail("Actual sampled search response is not an array"))?;
    anyhow::ensure!(rows.iter().all(|row| pages.pages.iter().enumerate().any(|(i, page)|
        sample.expected.published_pages[i] && row["document_id"] == page.id && row["space_id"] == page.space)),
        "Sampled search returned an inaccessible or noncanonical page");
    anyhow::ensure!(rows.iter().all(|row| row["is_draft"] == false && row["draft_id"].is_null()),
        "Default published search exposed draft identity");
    for row in rows {
        let i = pages.pages.iter().position(|page| row["document_id"] == page.id && row["space_id"] == page.space)
            .ok_or_else(|| fail("Original sampled search page absent"))?;
        if sample.expected.metadata[i].as_ref().is_some_and(|value| value["page_access_source"] == "direct_page_grant") {
            for key in ["space_name", "author", "author_id"] {
                anyhow::ensure!(row[key] == "", "Grant-only search exposed hidden labels or actors");
            }
            for key in ["maintenance_user_id", "maintenance_group_id", "folder_id", "folder_name", "draft_id"] {
                anyhow::ensure!(row[key].is_null(), "Grant-only search exposed private placement or ownership");
            }
        }
    }
    if scoped {
        anyhow::ensure!(rows.iter().all(|row| row["space_id"] == pages.pages[index].space),
            "Sampled scoped search returned a different original space");
    }
    if sample.expected.published_pages[index] {
        let page = &pages.pages[index];
        anyhow::ensure!(rows.iter().any(|row| row["document_id"] == page.id && row["space_id"] == page.space
            && row["excerpt"].as_str().is_some_and(|s| s.contains(word))), "Sampled rebuilt search lost an original permitted page");
    }
    Ok(())
}

async fn verify_normal_tcp_audiences(
    source: &mut StoppedVerificationSource<'_>, pages: &OriginalApplicationPlan<'_>, drafts: &OriginalApplicationDraftPlan<'_>,
    assets: &OriginalApplicationAssets<'_>, audiences: &OriginalApplicationAudiences<'_>, process: &mut std::process::Child,
    address: std::net::SocketAddr, channel: &File, nonce: &str,
) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
    let private_words = normal_original_private_words(pages, drafts)?;
    for sample in &audiences.samples {
        let token = &sample.credential.bearer;
        let (status, me) = normal_audience_observation(source, pages, drafts, audiences, assets, process, channel, nonce,
            normal_tcp_json(&client, address, Some(token), "/auth/me")).await?;
        anyhow::ensure!(status == reqwest::StatusCode::OK && me["id"] == sample.credential.auth.claims.sub,
            "Actual restored sampled authentication identity differs");
        for (index, page) in pages.pages.iter().enumerate() {
            let prefix = format!("/spaces/{}/documents/{}", urlencoding::encode(&page.space), urlencoding::encode(&page.id));
            let (status, metadata) = normal_audience_observation(source, pages, drafts, audiences, assets, process, channel, nonce,
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
                let (status, actual) = normal_audience_observation(source, pages, drafts, audiences, assets, process, channel, nonce,
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
                let (status, actual) = normal_audience_observation(source, pages, drafts, audiences, assets, process, channel, nonce,
                    normal_tcp_json(&client, address, Some(token), &uri)).await?;
                if if scoped { sample.expected.searches[index] } else { sample.expected.global_read } {
                    anyhow::ensure!(status == reqwest::StatusCode::OK, "Actual permitted sampled search refused");
                    verification_sample_search(&actual, sample, pages, index, word, scoped)?;
                } else { anyhow::ensure!(matches!(status.as_u16(), 401 | 403 | 404), "Source-denied sampled search exposed results"); }
            }
        }
        for (index, draft) in drafts.drafts.iter().enumerate() {
            let (status, actual) = normal_audience_observation(source, pages, drafts, audiences, assets, process, channel, nonce,
                normal_tcp_json(&client, address, Some(token), &draft.uri)).await?;
            if let Some((role, publish)) = sample.expected.drafts[index] {
                let mut expected = draft.expected.clone(); expected["access_role"] = role.into(); expected["can_publish"] = publish.into();
                anyhow::ensure!(status == reqwest::StatusCode::OK && actual == expected, "Sampled exact private draft participation differs");
            } else { anyhow::ensure!(matches!(status.as_u16(), 401 | 403 | 404), "Sampled user accessed an original private draft"); }
        }
        for word in &private_words {
            normal_audience_observation(source, pages, drafts, audiences, assets, process, channel, nonce,
                normal_private_default_search(&client, address, token, word, sample.expected.global_read)).await?;
        }
        for (asset_index,asset) in assets.assets.iter().enumerate() {
            let permitted=verification_expected_access(crate::spaces::shared_pages::asset_view(source.conn,&sample.credential.auth,&asset.space,&asset.page).await)?.is_some();
            let version=&assets.versions[asset_index];
            let metadata_uri=format!("/spaces/{}/documents/{}/attachment-versions/{}",urlencoding::encode(&asset.space),urlencoding::encode(&asset.page),urlencoding::encode(&asset.id));
            let(status,metadata)=normal_audience_observation(source,pages,drafts,audiences,assets,process,channel,nonce,normal_tcp_json(&client,address,Some(token),&metadata_uri)).await?;
            if permitted{anyhow::ensure!(status==reqwest::StatusCode::OK&&metadata["version"]==serde_json::to_value(version)?&&metadata["current_space_id"]==asset.space&&metadata["current_document_id"]==asset.page&&metadata["current_storage_path"]==asset.path&&metadata["retirement"]==assets.retirements[asset_index],"Sampled exact historical metadata/retirement differs");}else{anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Sampled nonmember obtained immutable metadata");}
            let uri=format!("http://{address}{}",sqlite_version_file_route(&asset.space,&asset.page,&version.logical_attachment_id,&asset.id));
            normal_audience_observation(source, pages, drafts, audiences, assets, process, channel, nonce, async {
                let mut response = client.get(&uri).bearer_auth(token).send().await?;
                if !permitted {
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
            let permitted=verification_expected_access(crate::spaces::shared_pages::asset_view(source.conn,&sample.credential.auth,&asset.space,&asset.page).await)?.is_some();
            normal_audience_observation(source,pages,drafts,audiences,assets,process,channel,nonce,sqlite_legacy_download(&client,address,Some(token),asset,permitted)).await?;
            let uri=format!("/spaces/{}/documents/{}/attachment-versions/{}",urlencoding::encode(&asset.space),urlencoding::encode(&asset.page),urlencoding::encode(&asset.id));
            let(status,_)=normal_audience_observation(source,pages,drafts,audiences,assets,process,channel,nonce,normal_tcp_json(&client,address,Some(token),&uri)).await?;anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Sampled legacy ID fabricated immutable metadata");
        }
    }
    audiences.fence(source, pages, drafts).await?; assets.fence(source).await?; Ok(())
}

#[cfg(test)] tokio::task_local! { static NORMAL_PENDING_AUDIENCE_CHANGE: std::cell::Cell<u8>; }
#[cfg(test)] async fn normal_pending_audience_change(source: &mut StoppedVerificationSource<'_>, plan: &OriginalApplicationAudiences<'_>) -> anyhow::Result<()> {
    let mode = NORMAL_PENDING_AUDIENCE_CHANGE.try_with(|v| v.replace(0)).unwrap_or(0);
    if mode != 0 {
        let sample = plan.samples.first().ok_or_else(|| fail("Original pending sampled actor absent"))?;
        if mode == 1 {
            sqlx::query("DELETE FROM sessions WHERE id=$1").bind(&sample.credential.auth.claims.sid).execute(&mut *source.conn).await?;
        } else if mode == 2 {
            sqlx::query("DELETE FROM space_members WHERE user_id=$1").bind(&sample.credential.auth.claims.sub).execute(&mut *source.conn).await?;
        } else { anyhow::bail!("Unknown test-only sampled source mutation"); }
    }
    Ok(())
}

#[cfg(test)] mod verification_audience_policy_controls {
    use super::*;
    #[test] fn internal_policy_failure_is_not_a_permission_denial() {
        assert!(verification_expected_access::<()>(Err(crate::auth::routes::AppError::Forbidden("expected".into()))).unwrap().is_none());
        assert!(verification_expected_access::<()>(Err(crate::auth::routes::AppError::Internal("unavailable".into()))).is_err());
        assert!(VerificationSuppliedAudience::supplied("forged bearer", "a-genuine-configured-disposable-secret-key").is_err());
    }
}

// These bounded literals come only from held original draft SQL and never
// become a record or an authority label. No target result chooses the query.
fn normal_private_words(published: &str, private: &[String]) -> Vec<String> {
    let public = published.to_ascii_lowercase();
    let mut words = std::collections::BTreeSet::new();
    for body in private {
        if let Some(word) = body.split(|c: char| !c.is_ascii_alphanumeric())
            .find(|word| (3..=40).contains(&word.len()) && !public.contains(&word.to_ascii_lowercase())) {
            words.insert(word.to_ascii_lowercase());
        }
    }
    words.into_iter().collect()
}
fn normal_original_private_words(pages: &OriginalApplicationPlan<'_>, drafts: &OriginalApplicationDraftPlan<'_>) -> anyhow::Result<Vec<String>> {
    let mut published = String::new();
    for page in &pages.pages {
        published.push_str(&page.body); published.push(' ');
        published.push_str(page.metadata["title"].as_str().ok_or_else(|| fail("Original published title absent"))?);
        published.push(' ');
    }
    let private = drafts.drafts.iter().map(|draft| {
        Ok(format!("{} {}", draft.expected["content"].as_str().ok_or_else(|| fail("Original private content absent"))?,
            draft.expected["title"].as_str().ok_or_else(|| fail("Original private title absent"))?))
    }).collect::<anyhow::Result<Vec<_>>>()?;
    let words = normal_private_words(&published, &private);
    anyhow::ensure!(words.len() <= 256, "Original private-only query plan exceeds256");
    Ok(words)
}
async fn normal_private_default_search(client: &reqwest::Client, address: std::net::SocketAddr,
    token: &str, word: &str, permitted: bool) -> anyhow::Result<()> {
    let uri = format!("/search?q={}&limit=100", urlencoding::encode(word));
    let (status, result) = normal_tcp_json(client, address, Some(token), &uri).await
        .map_err(|_| fail("Original private default-search transport refused"))?;
    if permitted {
        anyhow::ensure!(status == reqwest::StatusCode::OK,
            "Original private default-search request failed: status={status}; bounded_error={}",result["error"].as_str().unwrap_or("non-error JSON response").chars().take(256).collect::<String>());
        let rows=result.as_array().ok_or_else(||fail("Original private default-search response was not an array"))?;
        anyhow::ensure!(rows.is_empty(),
            "Default published search returned {} rows for original private-only query sha256={}; draft_or_identity_rows={}",rows.len(),digest(word.as_bytes()),rows.iter().filter(|row|row["is_draft"]==true||!row["draft_id"].is_null()).count());
    } else {
        anyhow::ensure!(matches!(status.as_u16(), 401 | 403 | 404), "Source-denied actor accessed default search");
    }
    Ok(())
}
async fn verify_normal_owner_private_search(source: &mut StoppedVerificationSource<'_>,
    pages: &OriginalApplicationPlan<'_>, drafts: &OriginalApplicationDraftPlan<'_>, address: std::net::SocketAddr,
    token: &str, channel: &File, nonce: &str) -> anyhow::Result<usize> {
    source.fence().await?; pages.fence(source)?; drafts.fence(source)?;
    let admin = source.admin.ok_or_else(|| fail("Original private-search human absent"))?;
    let auth = crate::auth::middleware::AuthUser { claims: admin.claims.clone(), api_key_scopes: None };
    let permitted = verification_expected_access(crate::spaces::access::require_current_human_read(source.conn, &auth).await)?.is_some();
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
    let words = normal_original_private_words(pages, drafts)?;
    for word in &words {
        normal_source_fenced_observation(source, pages, channel, nonce,
            normal_private_default_search(&client, address, token, word, permitted)).await?;
        drafts.fence(source)?;
    }
    source.fence().await?; Ok(words.len())
}
