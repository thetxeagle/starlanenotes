// Producer metadata is observational. It grants no backup/source/restore role.
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PgBackupProducer {package_version:String,executable_sha256:String}
fn pg_valid_producer_package_version(value:&str)->bool {
 if value.is_empty()||value.len()>128{return false;}
 let valid_part=|part:&str,numeric_leading_zero:bool|!part.is_empty()&&part.split('.').all(|p|!p.is_empty()&&p.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')&&(!numeric_leading_zero||!p.bytes().all(|b|b.is_ascii_digit())||p.len()==1||!p.starts_with('0')));
 let (without_build,build)=value.split_once('+').map_or((value,None),|(a,b)|(a,Some(b)));if build.is_some_and(|v|!valid_part(v,false)){return false;}
 let (core,pre)=without_build.split_once('-').map_or((without_build,None),|(a,b)|(a,Some(b)));if pre.is_some_and(|v|!valid_part(v,true)){return false;}
 let numbers=core.split('.').collect::<Vec<_>>();numbers.len()==3&&numbers.iter().all(|v|!v.is_empty()&&v.bytes().all(|b|b.is_ascii_digit())&&(v.len()==1||!v.starts_with('0'))&&v.parse::<u64>().is_ok())
}
impl PgBackupProducer {
 fn validate(&self)->anyhow::Result<()> {
  ensure!(pg_valid_producer_package_version(&self.package_version),"Invalid recorded producer package version");
  ensure!(self.executable_sha256.len()==64&&self.executable_sha256.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)),"Invalid recorded producer executable hash");Ok(())
 }
}
fn pg_producer_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<PgBackupProducer>,D::Error>{let value=<PgBackupProducer as serde::Deserialize>::deserialize(d)?;value.validate().map_err(serde::de::Error::custom)?;Ok(Some(value))}
struct PgHeldBackupProducer {file:File,stamp:PhysicalStamp,metadata:PgBackupProducer}
impl PgHeldBackupProducer {
 fn hold()->anyhow::Result<Self>{let file=File::open("/proc/self/exe")?;let stamp=physical_stamp(&file)?;ensure!(file.metadata()?.is_file()&&stamp.length<=1024*1024*1024,"Actual producer executable not bounded regular file");let metadata=PgBackupProducer{package_version:env!("CARGO_PKG_VERSION").into(),executable_sha256:hash_file(&file,stamp.length)?};metadata.validate()?;let held=Self{file,stamp,metadata};held.fence()?;Ok(held)}
 fn fence(&self)->anyhow::Result<()>{let actual=File::open("/proc/self/exe")?;ensure!(physical_stamp(&self.file)?==self.stamp&&physical_stamp(&actual)?==self.stamp&&identity(&actual)?==identity(&self.file)?,"Original running producer executable identity changed");self.metadata.validate()}
}
#[cfg(test)]mod pg_backup_producer_controls {
 use super::*;
 #[derive(serde::Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]struct OptionalProducer{#[serde(default,skip_serializing_if="Option::is_none",deserialize_with="pg_producer_nonnull")]producer:Option<PgBackupProducer>}
 #[test]fn absent_old_metadata_stays_absent_and_null_or_malformed_refuses(){let old:OptionalProducer=serde_json::from_str("{}").unwrap();assert!(old.producer.is_none());assert_eq!(serde_json::to_string(&old).unwrap(),"{}");for v in [serde_json::json!({"producer":null}),serde_json::json!({"producer":{}}),serde_json::json!({"producer":{"package_version":"branch-v1.5.1/forged","executable_sha256":"a".repeat(64)}}),serde_json::json!({"producer":{"package_version":"1.4.0","executable_sha256":"A".repeat(64)}}),serde_json::json!({"producer":{"package_version":"1.4.0","executable_sha256":"a".repeat(64),"serving_allowed":true}})]{assert!(serde_json::from_value::<OptionalProducer>(v).is_err());}}
 #[test]fn actual_running_producer_uses_compiled_component_version_and_held_elf(){let held=PgHeldBackupProducer::hold().unwrap();assert_eq!(held.metadata.package_version,env!("CARGO_PKG_VERSION"));assert_eq!(held.metadata.executable_sha256,hash_file(&File::open("/proc/self/exe").unwrap(),held.stamp.length).unwrap());held.fence().unwrap();let raw=serde_json::to_vec(&OptionalProducer{producer:Some(held.metadata.clone())}).unwrap();let read:OptionalProducer=serde_json::from_slice(&raw).unwrap();assert_eq!(read.producer.as_ref(),Some(&held.metadata));}
}
