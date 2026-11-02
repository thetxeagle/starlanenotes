//! Candidate DB-owned history capture. No journal, authorization or restore adapter.
//! Callers hold publication guards, prove source/identity and verify physical receipt.
use crate::auth::routes::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, Row};

fn conflict() -> AppError { AppError::Conflict("source_conflict: Snapshot capture changed; retain the original publication".into()) }
fn ordered(v:&Value)->Value {match v {Value::Object(m)=>{let mut keys:Vec<_>=m.keys().collect();keys.sort();let mut out=serde_json::Map::new();for k in keys {out.insert(k.clone(),ordered(&m[k]));}Value::Object(out)},Value::Array(a)=>Value::Array(a.iter().map(ordered).collect()),v=>v.clone()}}
pub(crate) fn digest(value:&Value)->Result<String,AppError> {let bytes=serde_json::to_vec(&ordered(value)).map_err(AppError::internal)?;let mut h=Sha256::new();h.update(b"starlane-publication-capture-v2\0");h.update(bytes);Ok(hex::encode(h.finalize()))}
fn raw_digest(bytes:&[u8])->String {hex::encode(Sha256::digest(bytes))}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub(crate) enum Binding { CaptureV2, LegacyContentV1 }
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub(crate) enum Extraction { ExactPublishedDraft, ProvenGeneratedHeader, RawOnly }
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Content {
    pub title:String, pub icon:Option<String>, pub cover_reference:Option<String>,
    pub portable_markdown:Option<String>, pub extraction:Extraction,
    pub raw_sha256:String, pub raw_byte_length:i64,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Comparisons {
    pub properties_schema:Vec<Value>, pub property_values:Vec<Value>,
    pub attachments:Vec<Value>, pub drawings:Vec<Value>,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Payload {
    pub schema_version:u32, pub binding:Binding, pub document_id:String,
    pub captured_space_id:String, pub content:Content, pub comparisons:Comparisons,
    // Baseline metadata is always observed at later capture, never reconstructed
    // as the original publication-time properties/assets.
    pub comparison_policy:String, pub asset_policy:String,
    pub baseline_source_page:Option<Value>,
}
#[derive(Clone,Debug,PartialEq)]
pub(crate) struct Capture {
    pub result:Payload, pub result_raw:Vec<u8>,
    pub baseline:Option<Payload>, pub baseline_raw:Option<Vec<u8>>,
    pub baseline_published_at:Option<String>,
}
impl Capture {
    pub(crate) fn descriptor(&self)->Value {
        json!({"schema_version":2,"result":self.result,"baseline":self.baseline,
            "baseline_published_at":self.baseline_published_at})
    }
    pub(crate) fn digest(&self)->Result<String,AppError>{digest(&self.descriptor())}
}

/// Producer-specific extraction only. Raw bytes stay separately preserved.
/// Duplicate generated keys or an unproven boundary are explicitly raw-only.
pub(crate) fn baseline_portable(raw:&[u8], generated_frontmatter:&str)->(Option<String>,Extraction) {
    let Ok(text)=std::str::from_utf8(raw) else{return (None,Extraction::RawOnly)};
    let prefix=format!("---\n{}\n",generated_frontmatter.trim());
    let Some(rest)=text.strip_prefix(&prefix) else{return (None,Extraction::RawOnly)};
    if let Some(body)=rest.strip_prefix("---\n\n") {return (Some(body.to_owned()),Extraction::ProvenGeneratedHeader)}
    let Some((metadata,_))=rest.split_once("\n---\n") else{return (None,Extraction::RawOnly)};
    // No YAML coercion, key normalization or body trimming.
    for line in metadata.lines() {
        if ["title:","icon:","locked:","public:"].iter().any(|key|line.trim_start().starts_with(key)) {
            return (None,Extraction::RawOnly);
        }
    }
    (Some(format!("---\n{rest}")),Extraction::ProvenGeneratedHeader)
}
pub(crate) fn content(title:String,icon:Option<String>,cover_reference:Option<String>,portable_markdown:Option<String>,extraction:Extraction,raw:&[u8])->Result<Content,AppError> {
    if raw.len()>6_000_000 {return Err(conflict())}
    Ok(Content{title,icon,cover_reference,portable_markdown,extraction,raw_sha256:raw_digest(raw),raw_byte_length:raw.len() as i64})
}

/// Raw schemas are included, not normalized property API projections. Capture
/// all current-space definitions. Missing/foreign IDs remain unresolved references;
/// never read another space's schema because of a corrupt cross-space value.
pub(crate) async fn comparisons(conn:&mut AnyConnection,space:&str,document:&str)->Result<Comparisons,AppError> {
    use super::document_deletion::rows;
    let mut attachments=Vec::new();
    for row in rows(conn,"SELECT * FROM attachments WHERE document_id=$1",&[document]).await.map_err(AppError::internal)? {
        if super::asset_audiences::is_published(conn,document,&super::draft_save::string(&row,"id")?).await? {attachments.push(row)}
    }
    attachments.sort_by_cached_key(Value::to_string);
    Ok(Comparisons {
        properties_schema:rows(conn,"SELECT * FROM properties_schema WHERE space_id=$1",&[space]).await.map_err(AppError::internal)?,
        property_values:rows(conn,"SELECT * FROM property_values WHERE document_id=$1",&[document]).await.map_err(AppError::internal)?,
        attachments,
        drawings:rows(conn,"SELECT * FROM excalidraw_files WHERE document_id=$1",&[document]).await.map_err(AppError::internal)?,
    })
}
pub(crate) async fn publication_comparisons(conn:&mut AnyConnection,space:&str,document:&str,referenced:&std::collections::BTreeSet<String>)->Result<Comparisons,AppError>{
    let mut result=comparisons(conn,space,document).await?;
    let all=super::document_deletion::rows(conn,"SELECT * FROM attachments WHERE document_id=$1",&[document]).await.map_err(AppError::internal)?;
    for row in all {let id=super::draft_save::string(&row,"id")?;if referenced.contains(&id)&&!result.attachments.iter().any(|a|a["id"]==id){result.attachments.push(row)}}
    result.attachments.sort_by_cached_key(Value::to_string);
    Ok(result)
}
pub(crate) async fn verify_comparisons(conn:&mut AnyConnection,space:&str,document:&str,expected:&Comparisons)->Result<(),AppError> {
    if comparisons(conn,space,document).await?!=*expected {return Err(conflict())} Ok(())
}
pub(crate) async fn needs_baseline(conn:&mut AnyConnection,document:&str)->Result<bool,AppError> {
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_publication_snapshots WHERE document_id=$1").bind(document).fetch_one(conn).await.map_err(AppError::internal)?;
    Ok(count==0)
}
pub(crate) fn payload(binding:Binding,document:&str,space:&str,content:Content,comparisons:Comparisons)->Payload {
    let comparison_policy=match binding {Binding::CaptureV2=>"comparison_only_capture_v2_bound",Binding::LegacyContentV1=>"comparison_only_observed_at_commit_original_prepare_unbound"};
    Payload{schema_version:1,binding,document_id:document.into(),captured_space_id:space.into(),content,comparisons,comparison_policy:comparison_policy.into(),asset_policy:"reference_only_current_authorization_and_existence_required".into(),baseline_source_page:None}
}
#[derive(Clone,Debug,PartialEq)]
pub(crate) struct Stored {pub id:String,pub payload:Payload,pub raw:Vec<u8>,pub kind:String,pub operation:String,pub published_at:String,pub captured_at:String,pub captured_by:String}
async fn insert(conn:&mut AnyConnection,row:&Stored)->Result<(),AppError> {
    let raw=&row.raw;let p=&row.payload;
    if p.content.raw_sha256!=raw_digest(raw)||p.content.raw_byte_length!=raw.len() as i64 {return Err(conflict())}
    let binding=match p.binding {Binding::CaptureV2=>"capture_v2",Binding::LegacyContentV1=>"legacy_content_v1"};
    sqlx::query("INSERT INTO document_publication_snapshots(id,document_id,captured_space_id,kind,capture_operation_id,publication_operation_id,binding,payload_json,payload_digest,raw_markdown,raw_sha256,raw_byte_length,published_at,captured_at,captured_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)")
        .bind(&row.id).bind(&p.document_id).bind(&p.captured_space_id).bind(&row.kind).bind(&row.operation)
        .bind(if row.kind=="publication"{Some(row.operation.clone())}else{None})
        .bind(binding).bind(serde_json::to_string(p).map_err(AppError::internal)?).bind(digest(&json!(p))?)
        .bind(raw).bind(&p.content.raw_sha256).bind(p.content.raw_byte_length)
        .bind(&row.published_at).bind(&row.captured_at).bind(&row.captured_by)
        .execute(conn).await.map_err(AppError::internal)?;Ok(())
}
/// Called once inside original publication transaction. Retry proves historical
/// snapshot/receipt bindings, not an INSERT/repair using current metadata.
pub(crate) async fn record(conn:&mut AnyConnection,capture:&Capture,operation:&str,revision:&str,actor:&str)->Result<Vec<Stored>,AppError> {
    let mut result=vec![];
    if let Some(baseline)=&capture.baseline {
        let row=Stored{id:format!("{operation}:baseline"),payload:baseline.clone(),raw:capture.baseline_raw.clone().ok_or_else(conflict)?,kind:"baseline".into(),operation:operation.into(),published_at:capture.baseline_published_at.clone().ok_or_else(conflict)?,captured_at:revision.into(),captured_by:actor.into()};
        insert(conn,&row).await?;result.push(row);
    } else if capture.baseline_raw.is_some()||capture.baseline_published_at.is_some(){return Err(conflict())}
    let row=Stored{id:format!("{operation}:publication"),payload:capture.result.clone(),raw:capture.result_raw.clone(),kind:"publication".into(),operation:operation.into(),published_at:revision.into(),captured_at:revision.into(),captured_by:actor.into()};
    insert(conn,&row).await?;result.push(row);Ok(result)
}
/// Expected-row proof must run after every owned SQL insertion/trigger and again
/// at final boundary; it is not a replacement for physical v2 fingerprint proof.
pub(crate) async fn verify_record(conn:&mut AnyConnection,expected:&[Stored])->Result<(),AppError> {
    for original in expected {
        validate_payload(&original.payload,&original.raw)?;
        let row=sqlx::query("SELECT * FROM document_publication_snapshots WHERE id=$1").bind(&original.id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?.ok_or_else(conflict)?;
        let string=|name|->Result<String,AppError>{row.try_get(name).map_err(AppError::internal)};
        let bytes:Vec<u8>=row.try_get("raw_markdown").map_err(AppError::internal)?;
        let p=&original.payload;let binding=match p.binding {Binding::CaptureV2=>"capture_v2",Binding::LegacyContentV1=>"legacy_content_v1"};
        let pub_op:Option<String>=row.try_get("publication_operation_id").map_err(AppError::internal)?;
        if string("document_id")?!=p.document_id||string("captured_space_id")?!=p.captured_space_id||string("kind")?!=original.kind||string("capture_operation_id")?!=original.operation||string("binding")?!=binding||pub_op!=if original.kind=="publication"{Some(original.operation.clone())}else{None}||
            string("payload_json")?!=serde_json::to_string(p).map_err(AppError::internal)?||string("payload_digest")?!=digest(&json!(p))?||bytes!=original.raw||string("raw_sha256")?!=p.content.raw_sha256||row.try_get::<i64,_>("raw_byte_length").map_err(AppError::internal)?!=p.content.raw_byte_length||string("published_at")?!=original.published_at||string("captured_at")?!=original.captured_at||string("captured_by")?!=original.captured_by {return Err(conflict())}
    }
    let Some(first)=expected.first() else{return Err(conflict())};
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_publication_snapshots WHERE capture_operation_id=$1").bind(&first.operation).fetch_one(conn).await.map_err(AppError::internal)?;
    if count!=expected.len() as i64{return Err(conflict())} Ok(())
}

pub(crate) fn fingerprint_v2(base_l3:&str,capture_digest:&str)->Result<String,AppError> {
    super::draft_save::digest(&json!({"publication_storage_fingerprint_v1":base_l3,"capture_descriptor_digest":capture_digest}))
}
async fn read_rows(conn:&mut AnyConnection,document:&str)->Result<Vec<Stored>,AppError> {
    let mut result=vec![];
    for row in sqlx::query("SELECT * FROM document_publication_snapshots WHERE document_id=$1 ORDER BY captured_at,id").bind(document).fetch_all(conn).await.map_err(AppError::internal)? {
        let string=|name|->Result<String,AppError>{row.try_get(name).map_err(AppError::internal)};
        let payload:Payload=serde_json::from_str(&string("payload_json")?).map_err(|_|conflict())?;
        if payload.schema_version!=1{return Err(conflict())}
        result.push(Stored{id:string("id")?,payload,raw:row.try_get("raw_markdown").map_err(AppError::internal)?,kind:string("kind")?,operation:string("capture_operation_id")?,published_at:string("published_at")?,captured_at:string("captured_at")?,captured_by:string("captured_by")?});
    }
    Ok(result)
}
fn validate_payload(p:&Payload,raw:&[u8])->Result<(),AppError> {
    let policy=match p.binding {Binding::CaptureV2=>"comparison_only_capture_v2_bound",Binding::LegacyContentV1=>"comparison_only_observed_at_commit_original_prepare_unbound"};
    if p.schema_version!=1||raw.len()>6_000_000||p.content.raw_sha256!=raw_digest(raw)||p.content.raw_byte_length!=raw.len() as i64||p.comparison_policy!=policy||p.asset_policy!="reference_only_current_authorization_and_existence_required" {return Err(conflict())}
    if p.content.extraction==Extraction::RawOnly && p.content.portable_markdown.is_some() {return Err(conflict())}
    Ok(())
}
/// Current route/page authority belongs to caller. This authenticates original
/// capture against exact route/physical receipt, never current canonical files.
pub(crate) async fn read_validated(conn:&mut AnyConnection,document:&str)->Result<Vec<Stored>,AppError> {
    let original=read_rows(conn,document).await?;
    let mut operations:Vec<String>=original.iter().map(|r|r.operation.clone()).collect();operations.sort();operations.dedup();
    let receipt_inventory=super::document_deletion::rows(conn,"SELECT * FROM draft_publication_receipts WHERE document_id=$1",&[document]).await.map_err(AppError::internal)?;
    for receipt in &receipt_inventory {
        let operation=super::draft_save::string(receipt,"operation_id")?;
        if operations.contains(&operation){continue}
        let request:Value=serde_json::from_str(&super::draft_save::string(receipt,"request_json")?).map_err(|_|conflict())?;
        let draft:Value=serde_json::from_str(&super::draft_save::string(receipt,"draft_json")?).map_err(|_|conflict())?;
        let actor=super::draft_save::string(receipt,"actor_id")?;let space=super::draft_save::string(receipt,"space_id")?;let draft_id=super::draft_save::string(receipt,"draft_id")?;
        let request_fp=super::draft_save::digest(&json!({"actor_id":actor,"space_id":space,"document_id":document,"draft_id":draft_id,"request":request}))?;
        let base_l3=super::draft_save::digest(&json!({"publication_request_fingerprint":request_fp,"original_draft":draft}))?;
        let manifest:Value=serde_json::from_str(&super::draft_save::string(receipt,"storage_manifest_json")?).map_err(|_|conflict())?;
        let prepared:crate::storage::publication::PreparedPublication=serde_json::from_value(manifest.clone()).map_err(|_|conflict())?;
        if receipt["request_fingerprint"]!=request_fp||manifest["payload_fingerprint"]!=base_l3||manifest["op_id"]!=operation||manifest["document_id"]!=document||manifest["space_id"]!=space||manifest["actor_id"]!=actor||manifest["draft_id"]!=draft_id {return Err(conflict())}
        crate::storage::publication::verify_recorded_publication(conn,&prepared).await?;
        crate::storage::publication::verify_snapshot_manifest(&prepared)?;
    }
    for operation in operations {
        let selected:Vec<Stored>=original.iter().filter(|r|r.operation==operation).cloned().collect();
        verify_record(conn,&selected).await?;
        if selected.iter().any(|r|r.id!=format!("{}:{}",r.operation,r.kind)){return Err(conflict())}
        let results:Vec<_>=selected.iter().filter(|r|r.kind=="publication").collect();
        let bases:Vec<_>=selected.iter().filter(|r|r.kind=="baseline").collect();
        if results.len()!=1||bases.len()>1||selected.len()!=results.len()+bases.len(){return Err(conflict())}
        let result=results[0]; let baseline=bases.first().copied();
        let receipts=super::document_deletion::rows(conn,"SELECT * FROM draft_publication_receipts WHERE operation_id=$1",&[&operation]).await.map_err(AppError::internal)?;
        if receipts.len()!=1{return Err(conflict())}let receipt=&receipts[0];
        let parse=|key:&str|->Result<Value,AppError>{serde_json::from_str(&super::draft_save::string(receipt,key)?).map_err(|_|conflict())};
        let request=parse("request_json")?;let draft=parse("draft_json")?;let page=parse("result_page_json")?;
        let manifest=parse("storage_manifest_json")?;
        let prepared:crate::storage::publication::PreparedPublication=serde_json::from_value(manifest.clone()).map_err(|_|conflict())?;
        let actor=super::draft_save::string(receipt,"actor_id")?;let space=super::draft_save::string(receipt,"space_id")?;let draft_id=super::draft_save::string(receipt,"draft_id")?;
        let request_fp=super::draft_save::digest(&json!({"actor_id":actor,"space_id":space,"document_id":document,"draft_id":draft_id,"request":request}))?;
        let base_l3=super::draft_save::digest(&json!({"publication_request_fingerprint":request_fp,"original_draft":draft}))?;
        if receipt["document_id"]!=document||receipt["request_fingerprint"]!=request_fp||request["operation_id"]!=operation||draft["id"]!=draft_id||draft["document_id"]!=document||draft["space_id"]!=space||draft["owner_id"]!=actor||super::draft_save::digest(&draft)?!=request["draft_source_digest"].as_str().ok_or_else(conflict)?||
            manifest["op_id"]!=operation||manifest["space_id"]!=space||manifest["document_id"]!=document||manifest["draft_id"]!=draft_id||manifest["actor_id"]!=actor||manifest["sha256"]!=result.payload.content.raw_sha256||manifest["length"]!=result.payload.content.raw_byte_length||manifest["revision"]!=result.published_at||
            result.payload.document_id!=document||result.payload.captured_space_id!=space||result.captured_by!=actor||result.captured_at!=result.published_at||result.payload.baseline_source_page.is_some(){return Err(conflict())}
        let portable=crate::template_guidance::strip_template_guidance(&super::draft_save::string(&draft,"content")?);
        if result.payload.content.title!=draft["title"].as_str().ok_or_else(conflict)?||result.payload.content.icon!=super::draft_save::optional(&draft,"icon")?||result.payload.content.cover_reference!=super::draft_save::optional(&draft,"cover_image")?||result.payload.content.portable_markdown!=Some(portable.clone())||result.payload.content.extraction!=Extraction::ExactPublishedDraft {return Err(conflict())}
        let expected_response=json!({"schema_version":1,"operation_id":operation,"space_id":space,"document_id":document,"draft_id":draft_id,"draft_source_digest":request["draft_source_digest"],"page_source_digest":request["page_source_digest"],"published_source_digest":super::published_source::digest(&page,&result.raw)?,"published_at":result.published_at,"publication":super::draft_publication::projection(&draft,&portable),"disposition":"applied"});
        if parse("response_json")?!=expected_response||page["id"]!=document||page["space_id"]!=space||page["title"]!=draft["title"]||page["icon"]!=draft["icon"]||page["cover_image"]!=draft["cover_image"]||page["content_path"]!=prepared.path()||page["updated_at"]!=prepared.revision()||page["published_at"]!=prepared.revision()||page["last_updated_by"]!=actor{return Err(conflict())}
        let capture=Capture{result:result.payload.clone(),result_raw:result.raw.clone(),baseline:baseline.map(|r|r.payload.clone()),baseline_raw:baseline.map(|r|r.raw.clone()),baseline_published_at:baseline.map(|r|r.published_at.clone())};
        let expected=match result.payload.binding {
            Binding::CaptureV2=>fingerprint_v2(&base_l3,&capture.digest()?)?,
            Binding::LegacyContentV1=>base_l3,
        };
        if manifest["payload_fingerprint"]!=expected{return Err(conflict())}
        if let Some(baseline)=baseline {
            let source=baseline.payload.baseline_source_page.as_ref().ok_or_else(conflict)?;
            let source_title=super::draft_save::string(source,"title")?;
            let source_icon=super::draft_save::optional(source,"icon")?;
            let frontmatter=super::documents::document_storage_frontmatter(&source_title,source_icon.as_deref(),source["locked"].as_i64()==Some(1),source["public"].as_i64()==Some(1));
            let (portable,extraction)=baseline_portable(&baseline.raw,&frontmatter);
            if baseline.payload.content.title!=source_title||baseline.payload.content.icon!=source_icon||baseline.payload.content.cover_reference!=super::draft_save::optional(source,"cover_image")?||baseline.payload.content.portable_markdown!=portable||baseline.payload.content.extraction!=extraction{return Err(conflict())}
            if baseline.payload.binding!=result.payload.binding||baseline.payload.document_id!=document||baseline.payload.captured_space_id!=space||baseline.captured_by!=actor||baseline.captured_at!=result.captured_at||source["id"]!=document||source["space_id"]!=space||source["published_at"]!=baseline.published_at||super::published_source::digest(source,&baseline.raw)?!=request["page_source_digest"].as_str().ok_or_else(conflict)? {return Err(conflict())}

        }
        crate::storage::publication::verify_recorded_publication(conn,&prepared).await?;
        // Lead-owned narrow read-only export: rooted original immutable manifest,
        // stage/anchor custody, no dependency on retired installed canonical path.
        crate::storage::publication::verify_snapshot_manifest(&prepared)?;
        verify_record(conn,&selected).await?;
        if super::document_deletion::rows(conn,"SELECT * FROM draft_publication_receipts WHERE operation_id=$1",&[&operation]).await.map_err(AppError::internal)?!=receipts{return Err(conflict())}
        crate::storage::publication::verify_recorded_publication(conn,&prepared).await?;
        crate::storage::publication::verify_snapshot_manifest(&prepared)?;
    }
    if read_rows(conn,document).await?!=original||super::document_deletion::rows(conn,"SELECT * FROM draft_publication_receipts WHERE document_id=$1",&[document]).await.map_err(AppError::internal)?!=receipt_inventory{return Err(conflict())}
    // LAST synchronous physical proof after all receipt and snapshot SQL awaits.
    for receipt in &receipt_inventory {
        let prepared:crate::storage::publication::PreparedPublication = serde_json::from_str(&super::draft_save::string(receipt,"storage_manifest_json")?).map_err(|_|conflict())?;
        crate::storage::publication::verify_snapshot_manifest(&prepared)?;
    }
    Ok(original)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn comparison()->Comparisons {Comparisons{properties_schema:vec![json!({"id":"p","name":"A","options":"{}"})],property_values:vec![],attachments:vec![],drawings:vec![]}}
    fn sample()->Payload {payload(Binding::CaptureV2,"d","s",content("".into(),None,None,Some(" body\n\n".into()),Extraction::ExactPublishedDraft,b"raw").unwrap(),comparison())}
    #[test]
    fn exact_generated_header_removes_only_proven_prefix() {
        let (body,mode)=baseline_portable(b"---\ntitle: A\nlocked: false\n---\n\n body\n\n","title: A\nlocked: false");
        assert_eq!(body.as_deref(),Some(" body\n\n"));assert_eq!(mode,Extraction::ProvenGeneratedHeader);
        let (body,mode)=baseline_portable(b"---\ntitle: A\nlocked: false\ntag: x\n---\n\n body\n","title: A\nlocked: false");
        assert_eq!(body.as_deref(),Some("---\ntag: x\n---\n\n body\n"));assert_eq!(mode,Extraction::ProvenGeneratedHeader);
        for bytes in [&b"---\ntitle: B\n---\nbody"[..],&b"---\ntitle: A\nlocked: false\ntitle: B\n---\nbody"[..],&b"\xff"[..]] {assert_eq!(baseline_portable(bytes,"title: A\nlocked: false"),(None,Extraction::RawOnly));}
    }
    #[test]
    fn complete_descriptor_binds_raw_schema_and_presentation() {
        let result=sample();let mut c=Capture{result,result_raw:b"raw".to_vec(),baseline:None,baseline_raw:None,baseline_published_at:None};
        let initial=c.digest().unwrap();c.result.comparisons.properties_schema[0]["name"]=json!("B");assert_ne!(initial,c.digest().unwrap());
        c.result.comparisons.properties_schema[0]["name"]=json!("A");c.result.content.cover_reference=Some("cover".into());assert_ne!(initial,c.digest().unwrap());
        assert_eq!(digest(&json!({"b":1,"a":{"z":2,"x":3}})).unwrap(),digest(&json!({"a":{"x":3,"z":2},"b":1})).unwrap());
    }
    #[test]
    fn coherent_sql_raw_hash_and_policy_tampering_are_refused() {
        let p=sample();assert!(validate_payload(&p,b"raw").is_ok());assert!(validate_payload(&p,b"changed").is_err());
        let mut p=p;p.comparison_policy="trusted".into();assert!(validate_payload(&p,b"raw").is_err());
        p.comparison_policy="comparison_only_capture_v2_bound".into();p.content.raw_byte_length=4;assert!(validate_payload(&p,b"raw").is_err());
    }
}

#[cfg(test)]
#[path="publication_snapshot_route_tests.rs"]
mod publication_route_tests;
#[cfg(test)]
#[path="publication_snapshot_capture_tests.rs"]
mod sql_capture_tests;
