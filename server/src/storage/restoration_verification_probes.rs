// Actual application probe evidence. Caller flags, logs or JSON cannot construct
// either the original source plan or completed evidence. This grants no runtime
// serving/import/cleanup/record-publication authority.
struct OriginalApplicationPage {
    space:String,id:String,path:String,held:File,stamp:PhysicalStamp,
    body:String,metadata:serde_json::Value,history:Vec<serde_json::Value>,properties:Vec<serde_json::Value>,
}
pub(crate) struct OriginalApplicationPlan<'a> {
    source:&'a Root,source_database:Identity,actor:String,session:String,
    pages:Vec<OriginalApplicationPage>,content:Vec<serde_json::Value>,
}
pub(crate) struct ApplicationPageEvidence {
    actor:String,session:String,page_count:usize,body_bytes:usize,
    check_sha256:String,
}
impl ApplicationPageEvidence {
    pub(crate) fn page_count(&self)->usize {self.page_count}
    pub(crate) fn check_sha256(&self)->&str {&self.check_sha256}
}
impl<'a> StoppedVerificationSource<'a> {
    /// Every active published page must be genuinely readable by the supplied
    /// original human. An inaccessible page refuses this initial producer; it
    /// must not be silently skipped or impersonated as another audience.
    pub(crate) async fn application_page_plan(&mut self)->anyhow::Result<OriginalApplicationPlan<'a>> {
        use anyhow::{ensure,Context};use sqlx::Row;
        self.fence().await?;let actor=self.current_human().await?;
        let admin=self.admin.context("Original application probe human absent")?;
        let auth=crate::auth::middleware::AuthUser{claims:admin.claims.clone(),api_key_scopes:None};
        let rows=sqlx::query("SELECT id,space_id,title,content_path,created_at,updated_at,published_at FROM documents WHERE published_at IS NOT NULL AND deleted_at IS NULL AND archived_at IS NULL ORDER BY space_id,id LIMIT 257").fetch_all(&mut *self.conn).await?;
        ensure!(rows.len()<=256,"Initial application verification exceeds256 original published pages");
        let mut pages=Vec::new();let mut content=Vec::new();let mut total=0usize;
        for row in rows {
            let id:String=row.try_get("id")?;let space:String=row.try_get("space_id")?;
            crate::spaces::access::resolve_page_access(self.conn,crate::spaces::access::principal_from_auth(&auth).map_err(|e|anyhow::anyhow!("Original principal unavailable: {e:?}"))?,&space,&id,crate::spaces::access::PageOperation::View,crate::spaces::access::Purpose::Routine).await.map_err(|e|anyhow::anyhow!("Original page audience cannot certify this producer: {e:?}"))?;
            let path:String=row.try_get("content_path")?;ensure!(valid_relative(&path),"Unsafe original application path");
            let (owned,stamp,sha)=self.inventory.files.get(&path).context("Original application body not inventoried")?;
            ensure!(stamp.length<=6_000_000,"Original application body exceeds route bounds");
            let held=owned.try_clone()?;let mut bytes=vec![0;stamp.length as usize];held.read_exact_at(&mut bytes,0)?;
            ensure!(physical_stamp(&held)?==*stamp&&digest(&bytes)==*sha,"Original application body changed during plan");
            total=total.checked_add(bytes.len()).context("Original application byte overflow")?;ensure!(total<=16*1024*1024,"Original application plan exceeds16MiB");
            let body=String::from_utf8(bytes)?;
            let raw=crate::spaces::published_source::selected(self.conn,&space,&id).await.map_err(|e|anyhow::anyhow!("Original raw published source unavailable: {e:?}"))?;
            ensure!(raw["content_path"].as_str()==Some(path.as_str()),"Original raw published path differs");
            let source_digest=crate::spaces::published_source::digest(&raw,body.as_bytes()).map_err(|e|anyhow::anyhow!("Original published digest unavailable: {e:?}"))?;
            let expected=serde_json::json!({"id":id,"title":raw["title"],"content":body,"published_source_digest":source_digest,"published_metadata":{"title":raw["title"],"icon":raw["icon"],"cover_image":raw["cover_image"]}});
            total=total.checked_add(serde_json::to_vec(&expected)?.len()).context("Original full content plan overflow")?;ensure!(total<=16*1024*1024,"Original full content plan exceeds16MiB");content.push(expected);

            let metadata=serde_json::json!({"id":id,"space_id":space,"title":row.try_get::<String,_>("title")?,"created_at":row.try_get::<String,_>("created_at")?,"updated_at":row.try_get::<String,_>("updated_at")?,"published_at":row.try_get::<String,_>("published_at")?});
            let histories=sqlx::query("SELECT v.id,v.version_number,v.lines_added,v.lines_removed,v.created_by,u.display_name AS author_name,v.created_at FROM doc_versions v JOIN users u ON u.id=v.created_by WHERE v.document_id=$1 ORDER BY v.version_number DESC LIMIT 257").bind(&id).fetch_all(&mut *self.conn).await?;
            ensure!(histories.len()<=256,"Original page history exceeds256 versions");
            let mut history=Vec::new();for h in histories {history.push(serde_json::json!({"id":h.try_get::<String,_>("id")?,"version_number":h.try_get::<i64,_>("version_number")?,"lines_added":h.try_get::<i64,_>("lines_added")?,"lines_removed":h.try_get::<i64,_>("lines_removed")?,"created_by":h.try_get::<String,_>("created_by")?,"author_name":h.try_get::<String,_>("author_name")?,"created_at":h.try_get::<String,_>("created_at")?}));}
            let values=sqlx::query("SELECT ps.id,ps.name,ps.property_type,ps.options,ps.sort_order,pv.value FROM properties_schema ps LEFT JOIN property_values pv ON ps.id=pv.property_id AND pv.document_id=$1 WHERE ps.space_id=$2 ORDER BY ps.sort_order LIMIT 257").bind(&id).bind(&space).fetch_all(&mut *self.conn).await?;
            ensure!(values.len()<=256,"Original page properties exceed256");let mut properties=Vec::new();let mut order=std::collections::HashSet::new();
            for v in values {ensure!(order.insert(v.try_get::<i64,_>("sort_order")?),"Ambiguous original property ordering cannot certify application presentation");let options=v.try_get::<Option<String>,_>("options")?.map(|raw|serde_json::from_str::<serde_json::Value>(&raw)).transpose()?;properties.push(serde_json::json!({"property_id":v.try_get::<String,_>("id")?,"property_name":v.try_get::<String,_>("name")?,"property_type":v.try_get::<String,_>("property_type")?,"options":options,"value":v.try_get::<Option<String>,_>("value")?}));}
            total=total.checked_add(serde_json::to_vec(&(&history,&properties))?.len()).context("Original application metadata overflow")?;ensure!(total<=16*1024*1024,"Original application plan exceeds16MiB");
            pages.push(OriginalApplicationPage{space,id,path,held,stamp:stamp.clone(),body,metadata,history,properties});
        }
        self.fence().await?;
        Ok(OriginalApplicationPlan{source:self.root,source_database:identity(self.database)?,actor,session:admin.claims.sid.clone(),pages,content})
    }
}
impl OriginalApplicationPlan<'_> {
    fn fence(&self,original:&StoppedVerificationSource<'_>)->anyhow::Result<()> {
        anyhow::ensure!(identity(self.source.fd())?==original.root_identity()?&&self.source_database==identity(original.database)?,"Application plan belongs to another original source");
        let admin=original.admin.ok_or_else(||fail("Original application human absent"))?;
        anyhow::ensure!(admin.claims.sub==self.actor&&admin.claims.sid==self.session,"Application plan original actor/session changed");
        self.source.check()?;
        for p in &self.pages {
            let fresh=self.source.leaf(&p.path)?;
            anyhow::ensure!(physical_stamp(&p.held)?==p.stamp&&physical_stamp(&fresh)?==p.stamp&&hash_file(&fresh,p.stamp.length)?==digest(p.body.as_bytes()),"Original application page bytes exchanged");
        }Ok(())
    }
}
async fn application_route_json(state:&crate::AppState,token:Option<&str>,uri:&str)->anyhow::Result<(axum::http::StatusCode,serde_json::Value)> {
    use tower::ServiceExt;
    let mut request=axum::http::Request::builder().method("GET").uri(uri);
    if let Some(token)=token {request=request.header("authorization",format!("Bearer {token}"));}
    let response=tokio::time::timeout(std::time::Duration::from_secs(15),crate::api_routes().with_state(state.clone()).oneshot(request.body(axum::body::Body::empty())?)).await??;
    let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),8*1024*1024).await?;
    Ok((status,serde_json::from_slice(&bytes)?))
}
/// Execute real registered application routes on the original initialized target
/// pool while writer exclusion and all original source/target fences remain held.
/// This evidence explicitly describes in-process page checks, not fresh-worker or
/// other authenticated audiences, full recovery, search or asset checks. History means registered version metadata, not historical body reconstruction.
pub(crate) async fn verify_application_pages(
    original:&mut StoppedVerificationSource<'_>,plan:&OriginalApplicationPlan<'_>,
    target:&VerificationTarget<'_>,reservation:&mut VerificationReservation,
    state:&crate::AppState,supplied_token:&str,
)->anyhow::Result<ApplicationPageEvidence> {
    use anyhow::ensure;
    ensure!(Arc::ptr_eq(&state.db.connect_options(),&target.pool.connect_options()),"Original application target pool required");
    let supplied=VerificationHumanCredential::supplied(supplied_token,&state.jwt_secret)?;
    ensure!(supplied.admin().claims.sub==plan.actor&&supplied.admin().claims.sid==plan.session,"Application probe requires exact original supplied credential");
    let mut checks=Vec::new();let mut body_bytes=0usize;
    for (index,page) in plan.pages.iter().enumerate() {
        ensure!(crate::storage::data_dir()==target.path(),"Application data directory retargeted");
        original.fence().await?;plan.fence(original)?;target.fence(reservation).await?;
        let uri=format!("/spaces/{}/documents/{}",urlencoding::encode(&page.space),urlencoding::encode(&page.id));
        let (status,metadata)=application_route_json(state,Some(supplied_token),&uri).await?;
        original.fence().await?;plan.fence(original)?;target.fence(reservation).await?;
        ensure!(status==axum::http::StatusCode::OK,"Original application page metadata unavailable");
        for (key,value) in page.metadata.as_object().ok_or_else(||fail("Malformed internal original metadata"))?{ensure!(metadata.get(key)==Some(value),"Application metadata differs from exact original field: {key}");}
        let (status,content)=application_route_json(state,Some(supplied_token),&format!("{uri}/content")).await?;
        original.fence().await?;plan.fence(original)?;target.fence(reservation).await?;
        ensure!(status==axum::http::StatusCode::OK&&content==plan.content[index],"Actual canonical application body differs from original");
        let (status,denied)=application_route_json(state,None,&format!("{uri}/content")).await?;
        original.fence().await?;plan.fence(original)?;target.fence(reservation).await?;
        ensure!(matches!(status,axum::http::StatusCode::UNAUTHORIZED|axum::http::StatusCode::FORBIDDEN|axum::http::StatusCode::NOT_FOUND),"Anonymous private route did not deny");
        ensure!(!denied.to_string().contains(&page.body),"Denied private response exposed original body");
        ensure!(crate::storage::data_dir()==target.path(),"Application data root changed across route wait");
        let (status,history)=application_route_json(state,Some(supplied_token),&format!("{uri}/versions")).await?;
        original.fence().await?;plan.fence(original)?;target.fence(reservation).await?;
        ensure!(status==axum::http::StatusCode::OK&&history==serde_json::json!(page.history),"Registered application history differs from exact original SQL");
        for version in &page.history {let version_id=version["id"].as_str().ok_or_else(||fail("Malformed internal original history"))?;let (status,actual)=application_route_json(state,Some(supplied_token),&format!("{uri}/versions/{}",urlencoding::encode(version_id))).await?;original.fence().await?;plan.fence(original)?;target.fence(reservation).await?;ensure!(status==axum::http::StatusCode::OK&&actual==*version,"Registered exact historical version differs from original");}
        let (status,properties)=application_route_json(state,Some(supplied_token),&format!("{uri}/properties")).await?;
        original.fence().await?;plan.fence(original)?;target.fence(reservation).await?;
        ensure!(status==axum::http::StatusCode::OK&&properties==serde_json::json!(page.properties),"Registered application properties differ from exact original SQL");
        for suffix in ["versions","properties"] {let (status,_)=application_route_json(state,None,&format!("{uri}/{suffix}")).await?;original.fence().await?;plan.fence(original)?;target.fence(reservation).await?;ensure!(matches!(status,axum::http::StatusCode::UNAUTHORIZED|axum::http::StatusCode::FORBIDDEN|axum::http::StatusCode::NOT_FOUND),"Anonymous private metadata route did not deny");}
        ensure!(crate::storage::data_dir()==target.path(),"Application data root changed across metadata route wait");
        checks.push(serde_json::json!({"space":page.space,"page":page.id,"metadata":page.metadata,"body_sha256":digest(page.body.as_bytes()),"history":page.history,"properties":page.properties,"anonymous_content_and_metadata_denied":true}));body_bytes+=page.body.len();
    }
    original.fence().await?;plan.fence(original)?;target.fence(reservation).await?;
    Ok(ApplicationPageEvidence{actor:plan.actor.clone(),session:plan.session.clone(),page_count:plan.pages.len(),body_bytes,check_sha256:digest(&serde_json::to_vec(&checks)?)})
}
