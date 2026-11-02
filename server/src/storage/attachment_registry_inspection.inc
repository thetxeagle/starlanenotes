/// Read-only SQL provenance closure. This token never certifies physical ownership or serving.
pub(crate) struct ValidatedRegistrySnapshot {
    completed_absences:std::collections::BTreeSet<String>,
    families: Vec<FamilyHead>,
    versions: Vec<VersionRecord>,
    changes: Vec<(String, ReplacementChange)>,
    retirements: Vec<super::attachment_retirement::Retirement>,
    current_associations: std::collections::BTreeMap<String,CurrentRegistryAssociation>,
    raw_rows: std::collections::BTreeMap<String,Vec<serde_json::Value>>,
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub(crate) struct CurrentRegistryAssociation {
    pub space_id:String,pub document_id:String,pub association:super::attachment_retirement::Association,
}
impl ValidatedRegistrySnapshot {
    pub(crate) fn completed_absences(&self)->&std::collections::BTreeSet<String>{&self.completed_absences}
    pub(crate) fn current_associations(&self)->&std::collections::BTreeMap<String,CurrentRegistryAssociation>{&self.current_associations}
    pub(crate) fn families(&self)->&[FamilyHead]{&self.families}
    pub(crate) fn versions(&self)->&[VersionRecord]{&self.versions}
    pub(crate) fn changes(&self)->&[(String,ReplacementChange)]{&self.changes}
    pub(crate) fn retirements(&self)->&[super::attachment_retirement::Retirement]{&self.retirements}
    pub(crate) fn raw_rows(&self)->&std::collections::BTreeMap<String,Vec<serde_json::Value>>{&self.raw_rows}
}
/// Caller must hold the independently verified bundle's read-only database snapshot.
/// Current moved paths require the existing upload/relocation owner inspection separately.
pub(crate) async fn inspect_registry_snapshot(conn:&mut AnyConnection)->Result<ValidatedRegistrySnapshot,String>{
 inspect_registry_snapshot_core(conn,None).await
}
pub(crate) async fn inspect_completed_registry_snapshot(conn:&mut AnyConnection,proof:&crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedRegistryAbsences)->Result<ValidatedRegistrySnapshot,String>{inspect_registry_snapshot_core(conn,Some(proof)).await}
async fn inspect_registry_snapshot_core(conn:&mut AnyConnection,completed:Option<&crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedRegistryAbsences>)->Result<ValidatedRegistrySnapshot,String>{
 let mut completed_absences=std::collections::BTreeSet::new();
    let names:[&str;4]=["attachment_families","attachment_version_records","attachment_version_changes","attachment_retirements"];
    let mut raw_rows=std::collections::BTreeMap::new();
    for name in names {
        let count:i64=sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {name}")).fetch_one(&mut *conn).await.map_err(|e|e.to_string())?;
        if !(0..=100000).contains(&count){return Err("Registry inspection row bound exceeded".into())}
        raw_rows.insert(name.to_owned(),crate::spaces::document_deletion::rows(conn,&format!("SELECT * FROM {name} ORDER BY 1"),&[]).await.map_err(|e|format!("{e:?}"))?);
    }
    let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM attachment_families ORDER BY id").fetch_all(&mut *conn).await.map_err(|e|e.to_string())?;
    let mut current_associations=std::collections::BTreeMap::new();let mut families=Vec::new();let mut versions=Vec::new();let mut changes=Vec::new();
    for id in ids {
        let first=history_page(conn,&id,0,100).await?;let head=first.head.clone();
        let mut family_versions=first.versions;
        while family_versions.len()<(head.revision as usize) {
            let after=family_versions.last().ok_or("Registry page made no progress")?.version_number;
            let page=history_page(conn,&id,after,100).await?;
            if page.head!=head||page.versions.is_empty(){return Err("Registry changed during inspection".into())}
            family_versions.extend(page.versions);
        }
        if family_versions.len()!=head.revision as usize{return Err("Registry sequence exceeds head".into())}
        for (index,v) in family_versions.iter().enumerate(){
            if v.version_number!=index as i64+1{return Err("Registry ordered sequence differs".into())}
            let association:Option<(String,String,String,i64,String,String,String)>=sqlx::query_as("SELECT filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at FROM attachments WHERE id=$1").bind(&v.version_id).fetch_optional(&mut *conn).await.map_err(|e|e.to_string())?;
            match association {
             Some(a)=>{
              if (a.0.as_str(),a.1.as_str(),a.2.as_str(),a.3,a.5.as_str(),a.6.as_str())!=(v.original_filename.as_str(),v.original_name.as_str(),v.mime_type.as_str(),v.byte_length,v.uploader_id.as_str(),v.original_created_at.as_str()) {return Err("Version original association differs".into())}
              let (space_id,document_id):(String,String)=sqlx::query_as("SELECT space_id,document_id FROM attachments WHERE id=$1").bind(&v.version_id).fetch_one(&mut *conn).await.map_err(|e|e.to_string())?;
              current_associations.insert(v.version_id.clone(),CurrentRegistryAssociation{space_id,document_id,association:a});
             },
             None=>{
              let receipt=completed.and_then(|p|p.original(&v.version_id)).ok_or("Retained version association missing")?;
              let raw=receipt_raw(conn,receipt).await?;
              // Full original receipt plus the ordinary immutable predecessor chain,
              // never a virtual current attachment/path or physical ownership grant.
              if *v!=replacement_version(receipt,&raw,&head.id,index as i64+1)||head.space_id!=v.original_space_id||head.document_id!=v.original_document_id{return Err("Completed registry original differs".into())}
              validated_version(conn,v).await?;completed_absences.insert(v.version_id.clone());
             }
            }
            if index>0 {
                let c=replacement_change(conn,&v.upload_operation_id).await?.ok_or("Replacement change missing")?;
                let old=&family_versions[index-1];
                if c.version!=*v||c.expected.id!=head.id||c.expected.space_id!=head.space_id||c.expected.document_id!=head.document_id||c.expected.created_at!=head.created_at||c.expected.state!="active"||c.expected.current_version_id!=old.version_id||c.expected.revision!=old.version_number {return Err("Replacement predecessor sequence differs".into())}
                changes.push((v.upload_operation_id.clone(),c));
            }
        }
        versions.extend(family_versions);families.push(head);
    }
    if versions.len()!=raw_rows["attachment_version_records"].len()||changes.len()!=raw_rows["attachment_version_changes"].len(){return Err("Orphan or extra registry evidence".into())}
    let ids:Vec<String>=sqlx::query_scalar("SELECT attachment_id FROM attachment_retirements ORDER BY attachment_id").fetch_all(&mut *conn).await.map_err(|e|e.to_string())?;
    let mut retirements=Vec::new();
    for id in ids {
        let r=super::attachment_retirement::read(conn,&id).await?.ok_or("Retirement disappeared")?;
        let a:Option<super::attachment_retirement::Association>=sqlx::query_as("SELECT filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at FROM attachments WHERE id=$1").bind(&id).fetch_optional(&mut *conn).await.map_err(|e|e.to_string())?;
        match a {
         Some(a)=>{
          if (a.0.as_str(),a.1.as_str(),a.2.as_str(),a.3,a.5.as_str(),a.6.as_str())!=(r.association.0.as_str(),r.association.1.as_str(),r.association.2.as_str(),r.association.3,r.association.5.as_str(),r.association.6.as_str()){return Err("Retirement original association differs".into())}
          let(space_id,document_id):(String,String)=sqlx::query_as("SELECT space_id,document_id FROM attachments WHERE id=$1").bind(&id).fetch_one(&mut *conn).await.map_err(|e|e.to_string())?;
          current_associations.insert(id.clone(),CurrentRegistryAssociation{space_id,document_id,association:a});
         },
         None=>{
          let proof=completed.ok_or("Retirement association missing")?;let receipt=proof.original(&id).ok_or("Retirement association missing")?;let i=&receipt.intent;let q=&i.request;
          let original=(i.filename.clone(),q.original_name.clone(),q.mime_type.clone(),i.byte_length,i.storage_path.clone(),q.actor_id.clone(),i.created_at.clone());
          if r.origin!="direct"||r.space_id!=q.space_id||r.document_id!=q.document_id||r.association!=original||!proof.retirement_matches(&r)||!completed_absences.contains(&id){return Err("Completed original retirement differs".into())}
         }
        }
        crate::sync::inspect_retirement_receipt(conn,&r).await?;
        retirements.push(r);
    }
    Ok(ValidatedRegistrySnapshot{completed_absences,families,versions,changes,retirements,current_associations,raw_rows})
}
