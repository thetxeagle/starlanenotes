// Private profile invariants. No caller-authored serving conversion.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
enum EngineCatalogueProfile { Historical49Pg3, Registry50Pg4, Capacity51Pg5, Retention52Pg6, Erasure53Pg7, Publication54Pg8, Audience55Pg9, DraftTransfer56Pg10, Discussion57Pg11 }
impl EngineCatalogueProfile {
 fn sqlite_version(self)->i64 {match self {Self::Historical49Pg3=>49,Self::Registry50Pg4=>50,Self::Capacity51Pg5=>51,Self::Retention52Pg6=>52,Self::Erasure53Pg7=>53,Self::Publication54Pg8=>54,Self::Audience55Pg9=>55,Self::DraftTransfer56Pg10=>56,Self::Discussion57Pg11=>57}}
 fn postgres_version(self)->i64 {match self {Self::Historical49Pg3=>3,Self::Registry50Pg4=>4,Self::Capacity51Pg5=>5,Self::Retention52Pg6=>6,Self::Erasure53Pg7=>7,Self::Publication54Pg8=>8,Self::Audience55Pg9=>9,Self::DraftTransfer56Pg10=>10,Self::Discussion57Pg11=>11}}
 fn catalogue_label(self)->&'static str {match self {Self::Historical49Pg3=>"sqlite49/postgres3",Self::Registry50Pg4=>"sqlite50/postgres4",Self::Capacity51Pg5=>"sqlite51/postgres5",Self::Retention52Pg6=>"sqlite52/postgres6",Self::Erasure53Pg7=>"sqlite53/postgres7",Self::Publication54Pg8=>"sqlite54/postgres8",Self::Audience55Pg9=>"sqlite55/postgres9",Self::DraftTransfer56Pg10=>"sqlite56/postgres10",Self::Discussion57Pg11=>"sqlite57/postgres11"}}
 fn application_tables(self)->usize {match self {Self::Historical49Pg3=>72,Self::Registry50Pg4=>76,Self::Capacity51Pg5=>79,Self::Retention52Pg6=>80,Self::Erasure53Pg7=>81,Self::Publication54Pg8=>84,Self::Audience55Pg9=>86,Self::DraftTransfer56Pg10=>87,Self::Discussion57Pg11=>91}}
 fn capacity_tables(self)->&'static [&'static str] {match self {Self::Capacity51Pg5|Self::Retention52Pg6|Self::Erasure53Pg7|Self::Publication54Pg8|Self::Audience55Pg9|Self::DraftTransfer56Pg10|Self::Discussion57Pg11=>&["attachment_capacity_lock","attachment_capacity_reservations","attachment_retained_byte_ledger"],_=>&[]}}
 fn registry_tables(self)->&'static [&'static str] {match self {Self::Historical49Pg3=>&[],Self::Registry50Pg4|Self::Capacity51Pg5|Self::Retention52Pg6|Self::Erasure53Pg7|Self::Publication54Pg8|Self::Audience55Pg9|Self::DraftTransfer56Pg10|Self::Discussion57Pg11=>&["attachment_families","attachment_version_records","attachment_version_changes","attachment_retirements"]}}
 const CURRENT:Self=Self::Discussion57Pg11;
 fn has_publication(self)->bool {matches!(self,Self::Publication54Pg8|Self::Audience55Pg9|Self::DraftTransfer56Pg10|Self::Discussion57Pg11)}
 fn has_audiences(self)->bool {matches!(self,Self::Audience55Pg9|Self::DraftTransfer56Pg10|Self::Discussion57Pg11)}
 fn transfer_tables(self)->&'static [&'static str] {if matches!(self,Self::DraftTransfer56Pg10|Self::Discussion57Pg11){&["draft_transfer_receipts"]}else{&[]}}
 fn discussion_tables(self)->&'static [&'static str] {if self==Self::Discussion57Pg11{&["draft_discussions","draft_discussion_participants","draft_discussion_comments","draft_discussion_subscriptions"]}else{&[]}}
 fn audience_tables(self)->&'static [&'static str] {if self.has_audiences(){&["attachment_audience_origins","attachment_audience_bindings"]}else{&[]}}
 fn publication_tables(self)->&'static [&'static str] {if self.has_publication(){&["draft_publication_receipts","document_publication_snapshots","snapshot_recovery_receipts"]}else{&[]}}
 fn has_erasure(self)->bool {matches!(self,Self::Erasure53Pg7|Self::Publication54Pg8|Self::Audience55Pg9|Self::DraftTransfer56Pg10|Self::Discussion57Pg11)}
 fn has_capacity(self)->bool {matches!(self,Self::Capacity51Pg5|Self::Retention52Pg6|Self::Erasure53Pg7|Self::Publication54Pg8|Self::Audience55Pg9|Self::DraftTransfer56Pg10|Self::Discussion57Pg11)}
 fn retention_tables(self)->&'static [&'static str] {match self{Self::Retention52Pg6=>&["attachment_erasure_outbox"],Self::Erasure53Pg7|Self::Publication54Pg8|Self::Audience55Pg9|Self::DraftTransfer56Pg10|Self::Discussion57Pg11=>&["attachment_erasure_outbox","attachment_erasure_dispositions"],_=>&[]}}

}

// Called only after exact migration checksum/prefix and held source-authority
// validation; a count alone never authenticates a profile.
fn checked_profile_shape(profile:EngineCatalogueProfile,shape:&BTreeMap<String,Table>)->anyhow::Result<()> {
 ensure!(shape.len()==profile.application_tables(),"Authenticated profile table count differs");
 for table in ["attachment_audience_origins","attachment_audience_bindings"] {ensure!(shape.contains_key(table)==profile.has_audiences(),"Authenticated audience table family differs: {table}");}
 for table in ["draft_transfer_receipts"] {ensure!(shape.contains_key(table)==profile.transfer_tables().contains(&table),"Authenticated transfer table family differs: {table}");}
 for table in ["draft_discussions","draft_discussion_participants","draft_discussion_comments","draft_discussion_subscriptions"] {ensure!(shape.contains_key(table)==profile.discussion_tables().contains(&table),"Authenticated discussion table family differs: {table}");}
 for table in ["draft_publication_receipts","document_publication_snapshots","snapshot_recovery_receipts"] {ensure!(shape.contains_key(table)==profile.has_publication(),"Authenticated publication table family differs: {table}");}
 for table in ["attachment_families","attachment_version_records","attachment_version_changes","attachment_retirements"] {
  ensure!(shape.contains_key(table)==profile.registry_tables().contains(&table),"Authenticated registry table family differs: {table}");
 }
 for table in ["attachment_capacity_lock","attachment_capacity_reservations","attachment_retained_byte_ledger"] {ensure!(shape.contains_key(table)==profile.capacity_tables().contains(&table),"Authenticated capacity table family differs: {table}");}
 for table in ["attachment_erasure_outbox","attachment_erasure_dispositions"] {ensure!(shape.contains_key(table)==profile.retention_tables().contains(&table),"Authenticated deletion table family differs: {table}");}
 Ok(())
}

// Reproduce only the new accounting migration INSERT SELECT from authenticated
// old attachment rows. This observation grants no file/receipt/disposal authority.
fn legacy_retained_ledger_seed(attachments:&Table,ledger:&Table)->anyhow::Result<Vec<Vec<Cell>>> {
 let mappings=[("asset_id","id"),("byte_length","size_bytes"),("filename","filename"),("original_name","original_name"),("mime_type","mime_type"),("uploader_id","created_by"),("original_created_at","created_at"),("original_storage_path","storage_path")];
 let expected_names=["asset_id","byte_length","filename","original_name","mime_type","uploader_id","original_created_at","original_storage_path","evidence_kind","original_receipt_json","original_receipt_sha256"];
 ensure!(ledger.columns.iter().map(|c|c.name.as_str()).collect::<Vec<_>>()==expected_names&&ledger.keys==vec!["asset_id"],"Retained accounting seed shape differs");
 let indexes=mappings.iter().map(|(_,name)|attachments.columns.iter().position(|c|c.name==*name).with_context(||format!("Original attachment accounting column absent: {name}"))).collect::<anyhow::Result<Vec<_>>>()?;
 let mut rows=Vec::with_capacity(attachments.rows.len());let mut ids=std::collections::BTreeSet::new();
 for original in &attachments.rows {
  ensure!(original.len()==attachments.columns.len(),"Original attachment row width differs");
  let mut row=Vec::with_capacity(11);
  for (position,index) in indexes.iter().enumerate(){let value=original[*index].clone();
   if position==1{ensure!(matches!(value,Cell::Integer(n)if n>=0),"Original attachment length invalid");}
   else{ensure!(matches!(value,Cell::Text(_)),"Original attachment accounting text invalid");}
   row.push(value);
  }
  let Cell::Text(id)=&row[0]else{unreachable!()};ensure!(ids.insert(id.clone()),"Duplicate original accounting asset");
  row.extend([Cell::Text(b"legacy_upgrade".to_vec()),Cell::Null,Cell::Null]);rows.push(row);
 }
 Ok(rows)
}

// Reproduce migration55 exactly: active/retired associations win over retained
// version-only originals. This classification never fabricates a draft grant.
fn legacy_audience_seed(source:&BTreeMap<String,Table>,origins:&Table)->anyhow::Result<Vec<Vec<Cell>>> {
 let names=["asset_id","kind","original_space_id","original_document_id","draft_id","draft_created_at","actor_id","operation_id","scope_json","scope_sha256"];
 ensure!(origins.columns.iter().map(|c|c.name.as_str()).collect::<Vec<_>>()==names&&origins.keys==vec!["asset_id"],"Audience migration seed shape differs");
 let mut rows=BTreeMap::<Vec<u8>,Vec<Cell>>::new();
 for (name,columns) in [("attachments",["id","space_id","document_id"]),("attachment_version_records",["version_id","original_space_id","original_document_id"])] {
  let Some(table)=source.get(name) else {ensure!(name=="attachment_version_records","Original attachments absent");continue;};
  let indices=columns.iter().map(|name|table.columns.iter().position(|c|c.name==*name).context("Original audience column absent")).collect::<anyhow::Result<Vec<_>>>()?;
  for original in &table.rows {
   ensure!(original.len()==table.columns.len(),"Original audience row width differs");
   let values=indices.iter().map(|i|original[*i].clone()).collect::<Vec<_>>();
   ensure!(values.iter().all(|c|matches!(c,Cell::Text(_))),"Original audience binding must be text");
   let Cell::Text(id)=&values[0] else{unreachable!()};
   if rows.contains_key(id){ensure!(name=="attachment_version_records","Duplicate original attachment");continue;}
   let mut row=vec![values[0].clone(),Cell::Text(b"legacy_page_v1".to_vec()),values[1].clone(),values[2].clone()];
   row.extend(std::iter::repeat_n(Cell::Null,6));rows.insert(id.clone(),row);
  }
 }
 Ok(rows.into_values().collect())
}

// Typed old-source/new-target projection. New target-only application tables
// are explicit migration output; they never enter the old source hash.
fn target_import_rows(source_profile:EngineCatalogueProfile,target_profile:EngineCatalogueProfile,source:&BTreeMap<String,Table>,target_shape:&BTreeMap<String,Table>)->anyhow::Result<BTreeMap<String,Table>> {
 checked_profile_shape(source_profile,source)?;checked_profile_shape(target_profile,target_shape)?;
 ensure!(source_profile.sqlite_version()<=target_profile.sqlite_version(),"Unsupported catalogue downgrade/evolution");
 let mut expected=target_shape.clone();
 for (name,table) in source {
  let target=expected.get_mut(name).context("Original source application table absent in target")?;
  ensure!(target.columns==table.columns&&target.keys==table.keys,"Original typed table shape changed: {name}");
  ensure!(target.rows.is_empty(),"Target shape contains caller-authored rows: {name}");
  target.rows=table.rows.clone();
 }
 for (name,table) in &mut expected {
  if !source.contains_key(name) {
   ensure!(table.rows.is_empty(),"Caller-authored target-only rows: {name}");
   match name.as_str() {
    "attachment_capacity_lock" if target_profile.has_capacity() => {
     ensure!(table.columns.len()==2&&table.columns[0].name=="id"&&table.columns[1].name=="revision"&&table.keys==vec!["id"],"Capacity migration seed shape differs");
     table.rows=vec![vec![Cell::Integer(1),Cell::Integer(1)]];
    },
    "attachment_audience_origins" if target_profile.has_audiences()=>{table.rows=legacy_audience_seed(source,table)?;},
    "attachment_audience_bindings" if target_profile.has_audiences()=>{},
    "attachment_capacity_reservations" if target_profile.has_capacity()=>{},
    "attachment_retained_byte_ledger" if target_profile.has_capacity()=>{
     table.rows=legacy_retained_ledger_seed(source.get("attachments").context("Original attachments absent")?,table)?;
    },
    _=>ensure!(target_profile.registry_tables().contains(&name.as_str())||target_profile.retention_tables().contains(&name.as_str())||target_profile.publication_tables().contains(&name.as_str())||target_profile.audience_tables().contains(&name.as_str())||target_profile.transfer_tables().contains(&name.as_str())||target_profile.discussion_tables().contains(&name.as_str()),"Unproved target-only application data: {name}"),
   }
  }
 }
 Ok(expected)
}

fn verify_source_target_readback(source:&BTreeMap<String,Table>,expected_target:&BTreeMap<String,Table>,actual_target:&BTreeMap<String,Table>)->anyhow::Result<()> {
 ensure!(expected_target.keys().eq(actual_target.keys()),"Target readback table set differs");
 for (name,expected) in expected_target {
  let actual=&actual_target[name];
  ensure!(expected.columns==actual.columns&&expected.keys==actual.keys&&expected.rows.len()==actual.rows.len()&&table_hash(expected)?==table_hash(actual)?,"Target typed readback differs: {name}");
 }
 for (name,original) in source {
  let actual=&actual_target[name];
  ensure!(original.rows.len()==actual.rows.len()&&table_hash(original)?==table_hash(actual)?,"Original source rows changed during catalogue evolution: {name}");
 }
 Ok(())
}

fn authenticated_pg_profile(migrations:&[String])->anyhow::Result<EngineCatalogueProfile> {
 let profile=match migrations.len(){3=>EngineCatalogueProfile::Historical49Pg3,4=>EngineCatalogueProfile::Registry50Pg4,5=>EngineCatalogueProfile::Capacity51Pg5,6=>EngineCatalogueProfile::Retention52Pg6,7=>EngineCatalogueProfile::Erasure53Pg7,8=>EngineCatalogueProfile::Publication54Pg8,9=>EngineCatalogueProfile::Audience55Pg9,10=>EngineCatalogueProfile::DraftTransfer56Pg10,11=>EngineCatalogueProfile::Discussion57Pg11,_=>bail!("Unrecognized independent PG migration profile")};
 let expected=[(1,"current schema",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0001_current_schema.sql")).as_slice()),(2,"current seed data",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0002_current_seed_data.sql")).as_slice()),(3,"attachment upload receipts",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0003_attachment_upload_receipts.sql")).as_slice()),(4,"immutable attachment versions",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0004_immutable_attachment_versions.sql")).as_slice()),(5,"attachment capacity reservations",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0005_attachment_capacity_reservations.sql")).as_slice()),(6,"attachment erasure outbox",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0006_attachment_erasure_outbox.sql")).as_slice()),(7,"attachment erasure dispositions",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0007_attachment_erasure_dispositions.sql")).as_slice()),(8,"document publication history",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0008_document_publication_history.sql")).as_slice()),(9,"draft asset audiences",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0009_draft_asset_audiences.sql")).as_slice()),(10,"draft transfer receipts",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0010_draft_transfer_receipts.sql")).as_slice()),(11,"draft discussions",include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/migrations-postgres/0011_draft_discussions.sql")).as_slice())];
 for (raw,(version,description,bytes)) in migrations.iter().zip(expected) {
  let value:serde_json::Value=serde_json::from_str(raw)?;let row=value.as_array().context("Invalid independent PG migration row")?;
  ensure!(row.len()==6&&row[0].as_i64()==Some(version)&&row[1].as_str()==Some(description)&&row[3].as_bool()==Some(true)&&row[4].as_str()==Some(hex::encode(sha2::Sha384::digest(bytes)).as_str())&&row[2].as_str().is_some_and(|s|s.parse::<f64>().is_ok())&&row[5].as_i64().is_some_and(|n|n>=0),"Independent PG migration bytes/record differs");
 }Ok(profile)
}
async fn pg_profile_any(conn:&mut sqlx::AnyConnection)->anyhow::Result<EngineCatalogueProfile>{
 let rows:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(conn).await?;authenticated_pg_profile(&rows)
}

#[cfg(test)] mod engine_catalogue_profile_tests {
 use super::*;
 #[test] fn deletion_profiles_preserve_original_rows_and_explicit_empty_upgrade_tables()->anyhow::Result<()> {
  let profiles=[EngineCatalogueProfile::Historical49Pg3,EngineCatalogueProfile::Registry50Pg4,EngineCatalogueProfile::Capacity51Pg5,EngineCatalogueProfile::Retention52Pg6,EngineCatalogueProfile::Erasure53Pg7,EngineCatalogueProfile::Publication54Pg8,EngineCatalogueProfile::Audience55Pg9,EngineCatalogueProfile::DraftTransfer56Pg10,EngineCatalogueProfile::Discussion57Pg11];
  let current=shape_for_profile(EngineCatalogueProfile::CURRENT)?;
  for (index,profile) in profiles.iter().copied().enumerate(){
   ensure!(profile.sqlite_version()==49+index as i64&&profile.postgres_version()==3+index as i64,"Independent exact profile versions differ");
   let mut source=shape_for_profile(profile)?;
   for table in profile.retention_tables(){let t=source.get_mut(*table).unwrap();t.rows.push(t.columns.iter().map(|c|Cell::Text(format!("original raw {} {}",table,c.name).into_bytes())).collect());}
   let expanded=target_import_rows(profile,EngineCatalogueProfile::CURRENT,&source,&current)?;
   verify_source_target_readback(&source,&expanded,&expanded)?;
   for table in EngineCatalogueProfile::CURRENT.retention_tables(){if !source.contains_key(*table){ensure!(expanded[*table].rows.is_empty(),"Upgrade invented erasure authority");}}
   let mut altered=expanded.clone();altered.get_mut("attachment_erasure_dispositions").unwrap().rows.push(vec![]);
   ensure!(verify_source_target_readback(&source,&expanded,&altered).is_err(),"Changed completed disposition accepted");
  }
  let mut caller=current.clone();caller.get_mut("attachment_erasure_outbox").unwrap().rows.push(vec![]);
  ensure!(target_import_rows(EngineCatalogueProfile::Capacity51Pg5,EngineCatalogueProfile::CURRENT,&shape_for_profile(EngineCatalogueProfile::Capacity51Pg5)?,&caller).is_err(),"Caller target-only deletion grant accepted");
  ensure!(target_import_rows(EngineCatalogueProfile::CURRENT,EngineCatalogueProfile::Retention52Pg6,&current,&shape_for_profile(EngineCatalogueProfile::Retention52Pg6)?).is_err(),"Erasure profile downgrade accepted");Ok(())
 }
 #[test] fn audience_profile_legacy_seed_and_exact_original_provenance_are_preserved()->anyhow::Result<()> {
  let old_profile=EngineCatalogueProfile::Publication54Pg8;let profile=EngineCatalogueProfile::Audience55Pg9;
  let mut old=shape_for_profile(old_profile)?;let shape=shape_for_profile(profile)?;
  ensure!(shape.len()==86&&profile.sqlite_version()==55&&profile.postgres_version()==9,"Audience current profile differs");
  fn row(table:&Table,values:&[(&str,&str)])->Vec<Cell>{let mut row=vec![Cell::Null;table.columns.len()];for(name,value)in values{let i=table.columns.iter().position(|c|c.name==*name).unwrap();row[i]=Cell::Text(value.as_bytes().to_vec());}row}
  let attachments=old.get_mut("attachments").unwrap();attachments.rows.push(row(attachments,&[("id","live"),("space_id","current-space"),("document_id","current-page")]));
  let versions=old.get_mut("attachment_version_records").unwrap();
  versions.rows.push(row(versions,&[("version_id","live"),("original_space_id","previous-space"),("original_document_id","previous-page")]));
  versions.rows.push(row(versions,&[("version_id","retained"),("original_space_id","retained-space"),("original_document_id","retained-page")]));
  let expanded=target_import_rows(old_profile,profile,&old,&shape)?;
  verify_source_target_readback(&old,&expanded,&expanded)?;
  let expected=vec![vec![Cell::Text(b"live".to_vec()),Cell::Text(b"legacy_page_v1".to_vec()),Cell::Text(b"current-space".to_vec()),Cell::Text(b"current-page".to_vec()),Cell::Null,Cell::Null,Cell::Null,Cell::Null,Cell::Null,Cell::Null],vec![Cell::Text(b"retained".to_vec()),Cell::Text(b"legacy_page_v1".to_vec()),Cell::Text(b"retained-space".to_vec()),Cell::Text(b"retained-page".to_vec()),Cell::Null,Cell::Null,Cell::Null,Cell::Null,Cell::Null,Cell::Null]];
  ensure!(expanded["attachment_audience_origins"].rows==expected&&expanded["attachment_audience_bindings"].rows.is_empty(),"Migration invented a draft audience or lost legacy original");
  let mut current=expanded.clone();
  current.get_mut("attachment_audience_origins").unwrap().rows.push(vec![Cell::Text(b"private".to_vec()),Cell::Text(b"draft_upload_v2".to_vec()),Cell::Text(b"space".to_vec()),Cell::Text(b"page".to_vec()),Cell::Text(b"draft".to_vec()),Cell::Text(b"original-created".to_vec()),Cell::Text(b"actor".to_vec()),Cell::Text(b"upload-op".to_vec()),Cell::Text(b"{ \"opaque\": true }".to_vec()),Cell::Text(vec![b'a';64])]);
  let binding=current.get_mut("attachment_audience_bindings").unwrap();binding.rows.push(row(binding,&[("asset_id","private"),("binding_kind","publication"),("target_id","snapshot"),("target_created_at","original-time"),("source_operation_id","original-op"),("evidence_json","{ \"raw\": true }"),("evidence_sha256","exact-original-digest")]));
  let retained=target_import_rows(profile,profile,&current,&shape)?;verify_source_target_readback(&current,&retained,&retained)?;
  let mut lost=retained.clone();lost.get_mut("attachment_audience_bindings").unwrap().rows.clear();ensure!(verify_source_target_readback(&current,&retained,&lost).is_err(),"Lost publication binding accepted");
  let mut forged=shape.clone();forged.get_mut("attachment_audience_bindings").unwrap().rows.push(vec![]);ensure!(target_import_rows(old_profile,profile,&old,&forged).is_err(),"Caller-authored promotion accepted");
  ensure!(target_import_rows(profile,old_profile,&current,&shape_for_profile(old_profile)?).is_err(),"Audience-dropping downgrade accepted");Ok(())
 }
 #[test] fn publication_profile_preserves_exact_receipts_and_binary_snapshots()->anyhow::Result<()> {
  let profile=EngineCatalogueProfile::Publication54Pg8;
  let current=shape_for_profile(profile)?;ensure!(current.len()==84,"Publication profile count differs");
  let mut source=current.clone();
  for name in profile.publication_tables(){let table=source.get_mut(*name).unwrap();table.rows.push(table.columns.iter().map(|column|match column.kind.as_str(){"BYTEA"=>Cell::Blob(vec![0,1,254,255]),"BIGINT"=>Cell::Integer(4),_=>Cell::Text(format!("original {} {}",name,column.name).into_bytes())}).collect());}
  let expected=target_import_rows(profile,profile,&source,&current)?;
  verify_source_target_readback(&source,&expected,&expected)?;
  let old=shape_for_profile(EngineCatalogueProfile::Erasure53Pg7)?;
  let upgraded=target_import_rows(EngineCatalogueProfile::Erasure53Pg7,profile,&old,&current)?;
  for name in profile.publication_tables(){ensure!(upgraded[*name].rows.is_empty(),"Upgrade manufactured original publication evidence");}
  let mut forged=current.clone();forged.get_mut("snapshot_recovery_receipts").unwrap().rows.push(vec![]);
  ensure!(target_import_rows(EngineCatalogueProfile::Erasure53Pg7,profile,&old,&forged).is_err(),"Caller authored recovery proof accepted");
  let mut altered=expected.clone();altered.get_mut("document_publication_snapshots").unwrap().rows.clear();
  ensure!(verify_source_target_readback(&source,&expected,&altered).is_err(),"Snapshot loss accepted");Ok(())
 }
 #[test] fn deletion_profiles_require_exact_migration_history_and_table_inventory()->anyhow::Result<()> {
  use sha2::Digest;
  for profile in [EngineCatalogueProfile::Retention52Pg6,EngineCatalogueProfile::Erasure53Pg7,EngineCatalogueProfile::Publication54Pg8,EngineCatalogueProfile::Audience55Pg9,EngineCatalogueProfile::DraftTransfer56Pg10,EngineCatalogueProfile::Discussion57Pg11]{
   let migrations=sqlx::migrate!("./migrations-postgres");let mut rows=vec![];
   for m in migrations.iter().filter(|m|m.version<=profile.postgres_version()){rows.push(serde_json::to_string(&serde_json::json!([m.version,m.description,"1.0",true,hex::encode(&m.checksum),1]))?);}
   ensure!(authenticated_pg_profile(&rows)?==profile,"Exact selected migration history misclassified");
   let mut changed=rows.clone();let mut row:serde_json::Value=serde_json::from_str(changed.last().unwrap())?;row[4]=serde_json::json!(hex::encode(sha2::Sha384::digest(b"altered DDL")));*changed.last_mut().unwrap()=serde_json::to_string(&row)?;
   ensure!(authenticated_pg_profile(&changed).is_err(),"Changed deletion DDL checksum accepted");
   let mut missing=shape_for_profile(profile)?;missing.remove("attachment_erasure_outbox");ensure!(checked_profile_shape(profile,&missing).is_err(),"Missing deletion evidence table accepted");
   let mut extra=shape_for_profile(profile)?;extra.insert("unknown_future_authority".into(),extra["attachment_erasure_outbox"].clone());ensure!(checked_profile_shape(profile,&extra).is_err(),"Unknown future table accepted");
  }Ok(())
 }

 #[test] fn old_source_current_target_keeps_exact_rows_and_explicit_empty_tables()->anyhow::Result<()> {
  let mut old=shape_for_profile(EngineCatalogueProfile::Historical49Pg3)?;let current=shape_for_profile(EngineCatalogueProfile::Registry50Pg4)?;
  let width=old["users"].columns.len();old.get_mut("users").unwrap().rows.push(vec![Cell::Null;width]);
  let expanded=target_import_rows(EngineCatalogueProfile::Historical49Pg3,EngineCatalogueProfile::Registry50Pg4,&old,&current)?;
  ensure!(expanded.len()==76&&old.len()==72,"Exact profiles differ");verify_source_target_readback(&old,&expanded,&expanded)?;
  let mut mutated=expanded.clone();mutated.get_mut("attachment_families").unwrap().rows.push(vec![]);ensure!(verify_source_target_readback(&old,&expanded,&mutated).is_err(),"Extra target-only rows accepted");
  ensure!(target_import_rows(EngineCatalogueProfile::Registry50Pg4,EngineCatalogueProfile::Historical49Pg3,&current,&old).is_err(),"Downgrade accepted");Ok(())
 }
 #[test] fn old_source_capacity_target_authenticates_seed_without_rehashing_original_rows()->anyhow::Result<()> {
  let old=shape_for_profile(EngineCatalogueProfile::Registry50Pg4)?;
  let target=shape_for_profile(EngineCatalogueProfile::Capacity51Pg5)?;
  let expanded=target_import_rows(EngineCatalogueProfile::Registry50Pg4,EngineCatalogueProfile::Capacity51Pg5,&old,&target)?;
  ensure!(expanded["attachment_capacity_lock"].rows==vec![vec![Cell::Integer(1),Cell::Integer(1)]],"Wrong migration seed");
  ensure!(expanded["attachment_capacity_reservations"].rows.is_empty(),"Migration invented admission rows");
  verify_source_target_readback(&old,&expanded,&expanded)?;
  let mut tampered=expanded.clone();tampered.get_mut("attachment_capacity_lock").unwrap().rows[0][1]=Cell::Integer(2);
  ensure!(verify_source_target_readback(&old,&expanded,&tampered).is_err(),"Altered seed passed readback");
  let mut caller_rows=target.clone();caller_rows.get_mut("attachment_capacity_lock").unwrap().rows=vec![vec![Cell::Integer(1),Cell::Integer(1)]];
  ensure!(target_import_rows(EngineCatalogueProfile::Registry50Pg4,EngineCatalogueProfile::Capacity51Pg5,&old,&caller_rows).is_err(),"Caller supplied migration seed accepted");Ok(())
 }
 #[test] fn fresh_capacity_import_requires_exact_migration_seed_and_no_reservations()->anyhow::Result<()> {
  let mut baseline=shape_for_profile(EngineCatalogueProfile::Capacity51Pg5)?;
  ensure!(validate_fresh_import_baseline(EngineCatalogueProfile::Capacity51Pg5,&baseline).is_err(),"Missing migration seed accepted");
  baseline.get_mut("attachment_capacity_lock").unwrap().rows=vec![vec![Cell::Integer(1),Cell::Integer(1)]];
  validate_fresh_import_baseline(EngineCatalogueProfile::Capacity51Pg5,&baseline)?;
  let mut changed=baseline.clone();changed.get_mut("attachment_capacity_lock").unwrap().rows[0][1]=Cell::Integer(2);
  ensure!(validate_fresh_import_baseline(EngineCatalogueProfile::Capacity51Pg5,&changed).is_err(),"Foreign seed revision accepted");
  changed=baseline.clone();changed.get_mut("attachment_capacity_reservations").unwrap().rows.push(vec![]);
  ensure!(validate_fresh_import_baseline(EngineCatalogueProfile::Capacity51Pg5,&changed).is_err(),"Fresh caller reservation accepted");
  Ok(())
 }
 #[test] fn old_attachment_rows_seed_accounting_without_inventing_receipts()->anyhow::Result<()> {
  let mut source=shape_for_profile(EngineCatalogueProfile::Registry50Pg4)?;
  let attachment=source.get_mut("attachments").unwrap();let mut row=vec![Cell::Null;attachment.columns.len()];
  for(name,value)in [("id","legacy-asset"),("filename","legacy.bin"),("original_name","Legacy original.bin"),("mime_type","application/octet-stream"),("created_by","original-uploader"),("created_at","2026-10-10T00:00:00Z"),("storage_path","uploads/space/legacy.bin")]{let index=attachment.columns.iter().position(|c|c.name==name).unwrap();row[index]=Cell::Text(value.as_bytes().to_vec());}
  let length=attachment.columns.iter().position(|c|c.name=="size_bytes").unwrap();row[length]=Cell::Integer(17);attachment.rows.push(row);
  let target=shape_for_profile(EngineCatalogueProfile::Capacity51Pg5)?;
  let expected=target_import_rows(EngineCatalogueProfile::Registry50Pg4,EngineCatalogueProfile::Capacity51Pg5,&source,&target)?;
  let rows=&expected["attachment_retained_byte_ledger"].rows;ensure!(rows.len()==1&&rows[0][1]==Cell::Integer(17)&&rows[0][8]==Cell::Text(b"legacy_upgrade".to_vec())&&rows[0][9..]==[Cell::Null,Cell::Null],"Ledger observation differs or fabricated receipt");
  verify_source_target_readback(&source,&expected,&expected)?;
  let mut lost=expected.clone();lost.get_mut("attachment_retained_byte_ledger").unwrap().rows.clear();ensure!(verify_source_target_readback(&source,&expected,&lost).is_err(),"Lost migration charge accepted");
  let mut bad=source.clone();bad.get_mut("attachments").unwrap().rows[0][length]=Cell::Integer(-1);ensure!(target_import_rows(EngineCatalogueProfile::Registry50Pg4,EngineCatalogueProfile::Capacity51Pg5,&bad,&target).is_err(),"Negative seed admitted");
  Ok(())
 }
 #[test] fn populated_capacity_source_preserves_original_charge_and_revision()->anyhow::Result<()> {
  let target=shape_for_profile(EngineCatalogueProfile::Capacity51Pg5)?;let mut source=target.clone();
  source.get_mut("attachment_capacity_lock").unwrap().rows=vec![vec![Cell::Integer(1),Cell::Integer(42)]];
  source.get_mut("attachment_capacity_reservations").unwrap().rows=vec![vec![Cell::Text(b"11111111-1111-4111-8111-111111111111".to_vec()),Cell::Text(vec![b'a';64]),Cell::Text(vec![b'b';64]),Cell::Integer(104857600),Cell::Text(b"reserved".to_vec()),Cell::Null,Cell::Null,Cell::Null,Cell::Null]];
  let expanded=target_import_rows(EngineCatalogueProfile::Capacity51Pg5,EngineCatalogueProfile::Capacity51Pg5,&source,&target)?;
  ensure!(expanded["attachment_capacity_lock"].rows==source["attachment_capacity_lock"].rows,"Original lock revision replaced with migration seed");
  verify_source_target_readback(&source,&expanded,&expanded)?;
  let mut lost=expanded.clone();lost.get_mut("attachment_capacity_reservations").unwrap().rows.clear();
  ensure!(verify_source_target_readback(&source,&expanded,&lost).is_err(),"Lost original charge accepted");
  ensure!(target_import_rows(EngineCatalogueProfile::Capacity51Pg5,EngineCatalogueProfile::Registry50Pg4,&source,&shape_for_profile(EngineCatalogueProfile::Registry50Pg4)?).is_err(),"Quota downgrade accepted");Ok(())
 }

}

// Fresh target-only relation census, after selected migrations and before import.
async fn target_relation_census(conn:&mut sqlx::PgConnection,schema:&str,shape:&BTreeMap<String,Table>,control:&str)->anyhow::Result<()> {
 let rows:Vec<(String,String,String,bool,bool,i64,i64,i64)>=sqlx::query_as("SELECT c.relname::text,c.relkind::text,c.relpersistence::text,c.relrowsecurity,c.relforcerowsecurity,(SELECT count(*) FROM pg_catalog.pg_trigger t WHERE t.tgrelid=c.oid AND NOT t.tgisinternal),(SELECT count(*) FROM pg_catalog.pg_policy p WHERE p.polrelid=c.oid),(SELECT count(*) FROM pg_catalog.pg_inherits i WHERE i.inhrelid=c.oid OR i.inhparent=c.oid) FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relkind IN ('r','p','v','m','f') ORDER BY c.relname").bind(schema).fetch_all(&mut *conn).await?;
 let expected=shape.keys().cloned().chain(["_sqlx_migrations".into(),control.to_owned()]).collect::<std::collections::BTreeSet<_>>();
 ensure!(rows.iter().map(|r|r.0.clone()).collect::<std::collections::BTreeSet<_>>()==expected,"Unexpected fresh target relation inventory");
 for (name,kind,persistence,rls,forced,triggers,policies,inherits) in rows {
  ensure!(kind=="r"&&persistence=="p"&&!rls&&!forced&&triggers==0&&policies==0&&inherits==0,"Fresh target relation has foreign behavior: {name}");
  if let Some(table)=shape.get(&name) {
   let columns:Vec<(String,String,bool,String,String)>=sqlx::query_as("SELECT a.attname::text,format_type(a.atttypid,a.atttypmod)::text,a.attnotnull,a.attidentity::text,a.attgenerated::text FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_class c ON c.oid=a.attrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=$2 AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum").bind(schema).bind(&name).fetch_all(&mut *conn).await?;
   let expected_columns=table.columns.iter().map(|c|(c.name.clone(),c.kind.to_ascii_lowercase(),c.required,String::new(),String::new())).collect::<Vec<_>>();ensure!(columns==expected_columns,"Fresh target column definition differs: {name}");
   let keys:Vec<String>=sqlx::query_scalar("SELECT a.attname::text FROM pg_catalog.pg_constraint k JOIN pg_catalog.pg_class c ON c.oid=k.conrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace CROSS JOIN LATERAL unnest(k.conkey) WITH ORDINALITY AS key(attnum,ordinality) JOIN pg_catalog.pg_attribute a ON a.attrelid=c.oid AND a.attnum=key.attnum WHERE n.nspname=$1 AND c.relname=$2 AND k.contype='p' ORDER BY key.ordinality").bind(schema).bind(&name).fetch_all(&mut *conn).await?;ensure!(keys==table.keys,"Fresh target primary key differs: {name}");
  }
 }
 let foreign:i64=sqlx::query_scalar("SELECT count(*) FROM pg_catalog.pg_constraint k JOIN pg_catalog.pg_class c ON c.oid=k.conrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace JOIN pg_catalog.pg_class referenced ON referenced.oid=k.confrelid WHERE n.nspname=$1 AND k.contype='f' AND referenced.relnamespace<>c.relnamespace").bind(schema).fetch_one(conn).await?;ensure!(foreign==0,"Fresh target foreign-key relation escapes owned schema");Ok(())
}


// Physical lineage is directed by exact committed owner evidence, never by equal bytes alone.
fn registry_certified_current(version:&crate::storage::attachment_versions::VersionRecord,path:&str,scopes:&[Scope],relocations:&[(crate::storage::relocation::PreparedRelocation,Vec<u8>)])->anyhow::Result<()> {
 let original=scopes.iter().find(|s|s.namespace==".attachment-upload-journal"&&s.op_id==version.upload_attempt_id).context("Original version upload custody absent")?;
 ensure!(original.evidence_sha256==version.install_proof_sha256,"Original version proof differs from classified custody");
 let valid=|b:&Binding|b.length==version.byte_length as u64&&b.sha256==version.binary_sha256;
 let mut reachable=original.bindings.iter().filter(|b|valid(b)).map(|b|(b.adopted_device,b.adopted_inode)).collect::<std::collections::BTreeSet<_>>();
 ensure!(!reachable.is_empty(),"Version upload group has no exact original binding");
 let mut edges=Vec::new();
 for (manifest,evidence) in relocations {
  let scope=scopes.iter().find(|s|s.namespace==".move-relocation-journal"&&s.op_id==manifest.op_id).context("Committed relocation custody missing")?;
  ensure!(scope.evidence_sha256==digest(evidence),"Relocation owner evidence differs from custody");
  for f in &manifest.files {
   if f.kind!="attachment"||f.subject_id!=version.version_id {continue}
   ensure!(f.length==version.byte_length as u64&&f.sha256==version.binary_sha256,"Version relocation original bytes differ");
   let find=|path:&str,identity:Identity|scope.bindings.iter().find(|b|b.relative==path&&valid(b)&&(b.original_device,b.original_inode)==identity);
   if let (Some(source),Some(destination))=(find(&f.source,(f.source_device,f.source_inode)),find(&f.destination,(f.destination_device,f.destination_inode))) {
    edges.push(((source.adopted_device,source.adopted_inode),(destination.adopted_device,destination.adopted_inode)));
   }
  }
 }
 loop {let before=reachable.len();for (source,destination) in &edges {if reachable.contains(source){reachable.insert(*destination);}}if reachable.len()==before{break}}
 ensure!(scopes.iter().flat_map(|s|&s.bindings).any(|b|b.relative==path&&valid(b)&&reachable.contains(&(b.adopted_device,b.adopted_inode))),"Version current path lacks exact owner-certified upload/relocation chain");Ok(())
}

async fn native_registry_before_pg_import(prepared:&PreparedEngineTransfer<'_>)->anyhow::Result<()> {
 if prepared.original.profile==EngineCatalogueProfile::Historical49Pg3 {return Ok(())}
 prepared.fence()?;let file=&prepared.original._database;let before=physical_stamp(file)?;
 sqlx::any::install_default_drivers();
 let mut conn=sqlx::AnyConnection::connect(&format!("sqlite:///proc/self/fd/{}?mode=ro&immutable=true",file.as_raw_fd())).await?;
 sqlx::query("PRAGMA query_only=ON").execute(&mut conn).await?;let mut tx=conn.begin().await?;
 let registry=crate::storage::attachment_versions::inspect_registry_snapshot(&mut tx).await.map_err(|e|anyhow!(e))?;
 let raw=read_ledger(&prepared.destination.leaves.get(".restoration-custody/current.json").context("Final custody absent")?.held)?;
 ensure!(digest(&raw)==prepared.custody_ledger_sha256,"Final owner ledger changed before registry import");let ledger:HistoryEnvelope=serde_json::from_slice(&raw)?;
 let mut relocations=Vec::new();
 for scope in ledger.runtime.scopes.iter().filter(|s|s.namespace==".move-relocation-journal") {
  let i=crate::storage::relocation::restoration_inspection::inspect(prepared.path(),&mut tx,&scope.op_id).await.map_err(|e|anyhow!("{e:?}"))?;
  ensure!(i.state==crate::storage::relocation::restoration_inspection::CommitState::Committed,"Version relocation not committed");relocations.push((i.manifest,i.manifest_bytes));
 }
 for version in registry.versions(){registry_certified_current(version,&registry.current_associations()[&version.version_id].association.4,&ledger.runtime.scopes,&relocations)?;}
 tx.rollback().await?;conn.close().await?;ensure!(physical_stamp(file)?==before,"Original readonly registry database changed");prepared.fence()?;Ok(())
}

// The new migration seed is authenticated before baseline deletion. It is then
// imported from expected_target_tables, without entering original-source hashes.
fn validate_fresh_import_baseline(profile:EngineCatalogueProfile,baseline:&BTreeMap<String,Table>)->anyhow::Result<()> {
 checked_profile_shape(profile,baseline)?;
 for(name,table) in baseline {
  if name=="attachment_capacity_lock" {
   ensure!(profile.has_capacity()&&table.rows==vec![vec![Cell::Integer(1),Cell::Integer(1)]],"Fresh capacity migration seed missing or differs");
  } else {ensure!(table.rows.is_empty()||matches!(name.as_str(),"global_roles"|"system_config"|"audience_mutation_fence"),"Unexpected pre-import target rows: {name}");}
 } Ok(())
}
