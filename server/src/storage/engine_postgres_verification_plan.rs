// Purpose-owned PG application plan; original typed source SQL/root/session constructs it.
// Never a fabricated SQLite File identity, copied receipt, API response or record grant.
struct PgCanonicalPagePlan {source:Identity,attestation_sha:String,actor:String,session:String,pages:Vec<OriginalApplicationPage>,content:Vec<serde_json::Value>}
impl<'a> StoppedPgVerificationSource<'a> {
    pub(crate) async fn canonical_page_plan(&mut self)->anyhow::Result<PgCanonicalPagePlan> {
        use anyhow::{ensure,Context};use sqlx::Row;
        self.fence().await?;let actor=self.current_human().await?;
        let admin=self.admin;
        let auth=crate::auth::middleware::AuthUser{claims:admin.claims.clone(),api_key_scopes:None};
        let rows=sqlx::query("SELECT id,space_id,title,content_path,created_at,updated_at,published_at FROM documents WHERE published_at IS NOT NULL AND deleted_at IS NULL AND archived_at IS NULL ORDER BY space_id,id LIMIT 257").fetch_all(&mut *self.conn).await?;
        ensure!(rows.len()<=256,"Initial application verification exceeds256 original published pages");
        let mut pages=Vec::new();let mut content=Vec::new();let mut total=0usize;
        for row in rows {
            let id:String=row.try_get("id")?;let space:String=row.try_get("space_id")?;
            crate::spaces::access::resolve_page_access(self.conn,crate::spaces::access::principal_from_auth(&auth).map_err(|e|anyhow::anyhow!("Original principal unavailable: {e:?}"))?,&space,&id,crate::spaces::access::PageOperation::View,crate::spaces::access::Purpose::Routine).await.map_err(|e|anyhow::anyhow!("Original page audience cannot certify this producer: {e:?}"))?;
            let path:String=row.try_get("content_path")?;ensure!(valid_relative(&path),"Unsafe original application path");
            let (owned,stamp)=self.files.get(&path).context("Original PG application body not inventoried")?;let sha=hash_file(owned,stamp.length)?;
            ensure!(stamp.length<=6_000_000,"Original application body exceeds route bounds");
            let held=owned.try_clone()?;let mut bytes=vec![0;stamp.length as usize];held.read_exact_at(&mut bytes,0)?;
            ensure!(physical_stamp(&held)?==*stamp&&digest(&bytes)==sha,"Original application body changed during plan");
            total=total.checked_add(bytes.len()).context("Original application byte overflow")?;ensure!(total<=16*1024*1024,"Original application plan exceeds16MiB");
            let body=String::from_utf8(bytes)?;
            let raw=crate::spaces::published_source::selected(self.conn,&space,&id).await.map_err(|e|anyhow::anyhow!("Original raw PG published source unavailable: {e:?}"))?;
            ensure!(raw["content_path"].as_str()==Some(path.as_str()),"Original PG raw published path differs");
            let source_digest=crate::spaces::published_source::digest(&raw,body.as_bytes()).map_err(|e|anyhow::anyhow!("Original PG published digest unavailable: {e:?}"))?;
            let expected_content=serde_json::json!({"id":id,"title":raw["title"],"content":body,"published_source_digest":source_digest,"published_metadata":{"title":raw["title"],"icon":raw["icon"],"cover_image":raw["cover_image"]}});total=pg_probe_account(total,&expected_content)?;content.push(expected_content);
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
        Ok(PgCanonicalPagePlan{source:identity(self.serving.proof.root.fd())?,attestation_sha:digest(&self.serving.proof.raw_attestation),actor,session:admin.claims.sid.clone(),pages,content})
    }
}
impl PgCanonicalPagePlan {
 fn fence(&self,original:&StoppedPgVerificationSource<'_>)->anyhow::Result<()> {
  ensure!(self.source==original.root_identity()?&&self.attestation_sha==digest(&original.serving.proof.raw_attestation)&&self.actor==original.admin.claims.sub&&self.session==original.admin.claims.sid,"Original canonical PG application plan context differs");original.physical()?;
  for page in &self.pages {let named=original.serving.proof.root.leaf(&page.path)?;ensure!(physical_stamp(&page.held)?==page.stamp&&physical_stamp(&named)?==page.stamp&&hash_file(&named,page.stamp.length)?==digest(page.body.as_bytes()),"Original canonical PG page replaced");}Ok(())
 }
 fn verify_content(&self,index:usize,actual:&serde_json::Value)->anyhow::Result<()> {let expected=self.content.get(index).context("Original canonical PG page absent")?;ensure!(actual==expected,"Exact original canonical PG row/body/digest/cover metadata differs");Ok(())}
}

fn pg_probe_account(total:usize,value:&serde_json::Value)->anyhow::Result<usize>{if let Some(metadata)=value.get("published_metadata"){ensure!(serde_json::to_vec(metadata)?.len()<=1024*1024,"Original PG published metadata exceeds1MiB");}let next=total.checked_add(serde_json::to_vec(value)?.len()).context("Original PG full probe-plan byte overflow")?;ensure!(next<=16*1024*1024,"Original PG full probe plan exceeds16MiB");Ok(next)}
#[cfg(test)]mod pg_probe_plan_pressure_controls {use super::*;#[test]fn pg_full_metadata_and_plan_pressure_refuses(){let ordinary=serde_json::json!({"published_metadata":{"title":"Original","icon":null,"cover_image":null},"content":"Original"});assert!(pg_probe_account(0,&ordinary).is_ok());assert!(pg_probe_account(16*1024*1024,&ordinary).is_err());let huge=serde_json::json!({"published_metadata":{"title":"Original","cover_image":"x".repeat(1024*1024)}});assert!(pg_probe_account(0,&huge).is_err());assert!(pg_probe_account(usize::MAX,&ordinary).is_err());}}
