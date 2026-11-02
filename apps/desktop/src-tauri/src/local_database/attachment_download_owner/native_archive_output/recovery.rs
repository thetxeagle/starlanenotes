//! Durable named review is evidence acknowledgment, never publication authority.
use super::*;
const MAX_OPERATIONS:usize=128;
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {format:u8,operation:String,journal_sha256:String,evidence_sha256:String,directory:Identity,original_binding_sha256:String,journal_identity:Identity}
struct Evidence {dir:Directory,journal:Journal,journal_sha:String,report:Value}
fn valid_uuid(s:&str)->bool{s.len()==36&&s.bytes().enumerate().all(|(i,b)|if [8,13,18,23].contains(&i){b==b'-'}else{b.is_ascii_hexdigit()&&!b.is_ascii_uppercase()})}
fn private(file:&File,directory:bool)->Result<(),String>{let m=file.metadata().map_err(err)?;if m.uid()!=unsafe{libc::geteuid()}||m.mode()&0o777!=if directory{0o700}else{0o600}{return Err("Archive recovery private custody mode/owner differs".into())}Ok(())}
fn read_json(dir:&Directory,leaf:&str)->Result<Vec<u8>,String>{let mut f=dir.open(leaf,libc::O_RDONLY).map_err(err)?;regular(&f,1)?;private(&f,false)?;if f.metadata().map_err(err)?.len()>65536{return Err("Archive recovery record exceeds budget".into())}let mut b=Vec::new();f.read_to_end(&mut b).map_err(err)?;dir.fence().map_err(err)?;Ok(b)}
fn evidence(parent:&Directory,operation:&str,expected:Option<&Value>)->Result<Evidence,String>{
 if !valid_uuid(operation){return Err("Archive recovery operation invalid".into())}let dir=parent.child(&format!(".starlane-archive-{operation}"),false).map_err(err)?;private(&dir.held,true)?;
 let journal_identity=dir.file_identity("journal.json").map_err(err)?.ok_or("Original archive journal absent")?;let raw=read_json(&dir,"journal.json")?;let journal:Journal=serde_json::from_slice(&raw).map_err(err)?;
 if journal.format!=1||journal.operation!=operation||journal.parent!=identity(&parent.held).map_err(err)?||expected.is_some_and(|b|journal.binding!=*b)||journal.leaf.is_empty()||journal.leaf.contains('/')||matches!(journal.leaf.as_str(),"."|"..")||!matches!(journal.state.as_str(),"staging"|"sealed"|"publication_unknown"|"published")||journal.manifest_sha256.len()!=64{return Err("Archive recovery record original binding invalid".into())}
 let status=inspect(&parent.path,operation,&journal.binding)?;
 let stage=named(&dir,"archive.zip")?.ok_or("Archive original stage absent")?;
 if stage.0!=journal.stage||stage.1>4096*104857600u64+64*1024*1024{return Err("Archive original stage identity/size differs".into())}
 let mut stage_file=dir.open("archive.zip",libc::O_RDONLY).map_err(err)?;let stage_sha=file_sha(&mut stage_file)?;if named(&dir,"archive.zip")?!=Some(stage)||identity(&stage_file).map_err(err)?!=stage.0{return Err("Archive stage changed during review".into())}
 let destination=match named(parent,&journal.leaf)?{
  None=>json!({"present":false}),
  Some((id,length,links))=>{if length>journal.length&&journal.state!="staging"{return Err("Foreign archive output exceeds original inspection budget".into())}let mut f=parent.open(&journal.leaf,libc::O_RDONLY).map_err(err)?;let digest=file_sha(&mut f)?;if named(parent,&journal.leaf)?!=Some((id,length,links))||identity(&f).map_err(err)?!=id{return Err("Archive destination changed during review".into())}json!({"present":true,"identity":id,"length":length,"links":links,"sha256":digest})}
 };
 let report=json!({"format":1,"operation":operation,"destination":parent.path.join(&journal.leaf),"state":status["state"],"journal_state":journal.state,"journal_sha256":sha(&raw),"journal_identity":journal_identity,"original_binding_sha256":sha(&serde_json::to_vec(&journal.binding).map_err(err)?),"stage":{"identity":stage.0,"length":stage.1,"links":stage.2,"sha256":stage_sha},"output":destination,"automatic_retry":false,"automatic_cleanup":false});
 parent.fence().map_err(err)?;dir.fence().map_err(err)?;if dir.file_identity("journal.json").map_err(err)?!=Some(journal_identity)||read_json(&dir,"journal.json")?!=raw{return Err("Archive journal changed during review".into())}Ok(Evidence{dir,journal,journal_sha:sha(&raw),report})
}
fn reviewed(e:&Evidence)->Result<bool,String>{
 let raw=match read_json(&e.dir,"review.json"){Ok(b)=>b,Err(reason)=>{if named(&e.dir,"review.json")?.is_none(){return Ok(false)}return Err(reason)}};
 let r:Review=serde_json::from_slice(&raw).map_err(err)?;
 if r.format!=1||r.operation!=e.journal.operation||r.journal_sha256!=e.journal_sha||r.evidence_sha256!=sha(&serde_json::to_vec(&e.report).map_err(err)?)||r.directory!=identity(&e.dir.held).map_err(err)?||r.original_binding_sha256!=e.report["original_binding_sha256"]||serde_json::to_value(r.journal_identity).map_err(err)?!=e.report["journal_identity"]{return Err("Archive review no longer matches original physical evidence".into())}Ok(true)
}
fn operations(parent:&Directory)->Result<Vec<String>,String>{
 parent.fence().map_err(err)?;let mut operations=Vec::new();let path=PathBuf::from(format!("/proc/self/fd/{}",parent.held.as_raw_fd()));
 let mut scanned=0usize;for entry in std::fs::read_dir(path).map_err(err)?{scanned+=1;if scanned>4096{return Err("Archive destination directory exceeds4096 inspected entries".into())}let entry=entry.map_err(err)?;let Some(name)=entry.file_name().to_str().map(str::to_owned)else{continue};if let Some(operation)=name.strip_prefix(".starlane-archive-"){if !valid_uuid(operation){return Err("Malformed named archive operation refuses recovery".into())}operations.push(operation.to_string());if operations.len()>MAX_OPERATIONS{return Err("Archive pending operation count exceeds128; explicit operator review required".into())}}}
 operations.sort();parent.fence().map_err(err)?;Ok(operations)
}
pub(super) fn require_clear(parent:&Directory,leaf:&str)->Result<(),String>{
 for operation in operations(parent)?{let dir=parent.child(&format!(".starlane-archive-{operation}"),false).map_err(err)?;private(&dir.held,true)?;let j:Journal=serde_json::from_slice(&read_json(&dir,"journal.json")?).map_err(err)?;if j.operation!=operation||j.parent!=identity(&parent.held).map_err(err)?{return Err("Archive pending header differs".into())}if j.leaf!=leaf{continue}let e=evidence(parent,&operation,None)?;if e.report["state"]!="published_verified"&&!reviewed(&e)?{return Err(format!("Original archive operation {operation} requires named destination review before another Save"))}}
 Ok(())
}
pub(crate) fn pending(parent_path:&Path,expected_binding:&Value)->Result<Value,String>{
 let parent=Directory::capture(parent_path).map_err(err)?;let mut rows=Vec::new();
 for operation in operations(&parent)?{let dir=parent.child(&format!(".starlane-archive-{operation}"),false).map_err(err)?;private(&dir.held,true)?;let j:Journal=serde_json::from_slice(&read_json(&dir,"journal.json")?).map_err(err)?;if j.operation!=operation||j.parent!=identity(&parent.held).map_err(err)?{return Err("Archive pending header differs".into())}if j.binding!=*expected_binding{continue}let e=evidence(&parent,&operation,Some(expected_binding))?;let mut row=e.report.clone();row["reviewed"]=json!(reviewed(&e)?);rows.push(row)}
 Ok(json!({"format":1,"operations":rows,"max_operations":MAX_OPERATIONS,"automatic_retry":false,"automatic_cleanup":false}))
}
/// Caller must bind an explicit named-destination review event to this exact report.
/// This does not resume Save, remove a file, serve bytes, or grant source authority.
pub(crate) fn acknowledge_review(parent_path:&Path,operation:&str,expected_binding:&Value,original_report:&Value)->Result<Value,String>{acknowledge_review_guarded(parent_path,operation,expected_binding,original_report,||Ok(()))}
pub(in crate::local_database::attachment_download_owner) fn acknowledge_review_guarded(parent_path:&Path,operation:&str,expected_binding:&Value,original_report:&Value,fence:impl Fn()->Result<(),String>)->Result<Value,String>{
 fence()?;
 let parent=Directory::capture(parent_path).map_err(err)?;let e=evidence(&parent,operation,Some(expected_binding))?;
 let mut requested=original_report.clone();if let Some(value)=requested.as_object_mut().ok_or("Original review report invalid")?.remove("reviewed"){if !value.is_boolean(){return Err("Original review display state invalid".into())}}
 if e.report!=requested{return Err("Original named destination review evidence changed".into())}
 if reviewed(&e)?{fence()?;return Ok(json!({"format":1,"operation":operation,"reviewed":true,"replayed":true,"retry_authority":false,"cleanup_authority":false}))}
 let record=Review{format:1,operation:operation.into(),journal_sha256:e.journal_sha.clone(),evidence_sha256:sha(&serde_json::to_vec(&e.report).map_err(err)?),directory:identity(&e.dir.held).map_err(err)?,original_binding_sha256:e.report["original_binding_sha256"].as_str().unwrap().into(),journal_identity:serde_json::from_value(e.report["journal_identity"].clone()).map_err(err)?};
 let bytes=serde_json::to_vec(&record).map_err(err)?;fence()?;let mut file=e.dir.open("review.json",libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL).map_err(err)?;file.write_all(&bytes).map_err(err)?;file.sync_all().map_err(err)?;e.dir.held.sync_all().map_err(err)?;
 fence().map_err(|e|format!("Archive review acknowledgment unknown; retained evidence: {e}"))?;let after=evidence(&parent,operation,Some(expected_binding))?;if after.report!=e.report||!reviewed(&after)?{return Err("Archive review outcome unresolved; retained record requires inspection".into())}
 Ok(json!({"format":1,"operation":operation,"reviewed":true,"replayed":false,"retry_authority":false,"cleanup_authority":false}))
}
#[cfg(test)]mod tests;

/// Held read-only historical observation. It is never an old creator/publication token.
pub(in crate::local_database::attachment_download_owner) struct SealedOriginal {parent:Directory,original:Evidence,file:File}
impl SealedOriginal {
 pub(in crate::local_database::attachment_download_owner) fn capture(parent_path:&Path,binding:&Value,report:&Value)->Result<Self,String>{
  let parent=Directory::capture(parent_path).map_err(err)?;let operation=report["operation"].as_str().ok_or("Exact original sealed observation required")?;let original=evidence(&parent,operation,Some(binding))?;
  let mut requested=report.clone();requested.as_object_mut().ok_or("Original sealed report malformed")?.remove("reviewed");
  if requested!=original.report||!matches!(original.report["state"].as_str(),Some("sealed_unpublished"|"unresolved_output_absent"))||original.report["stage"]["links"]!=1||original.report["output"]["present"]!=false{return Err("Original complete sealed absent output observation required".into())}
  let file=original.dir.open("archive.zip",libc::O_RDONLY).map_err(err)?;let result=Self{parent,original,file};result.fence()?;Ok(result)
 }
 pub(super) fn binding(&self)->&Value{&self.original.journal.binding}
 pub(super) fn report(&self)->&Value{&self.original.report}
 pub(in crate::local_database::attachment_download_owner) fn fence(&self)->Result<(),String>{
  self.parent.fence().map_err(err)?;self.original.dir.fence().map_err(err)?;regular(&self.file,1)?;private(&self.file,false)?;
  if identity(&self.file).map_err(err)?!=self.original.journal.stage{return Err("Held original sealed observation changed".into())}
  let now=evidence(&self.parent,&self.original.journal.operation,Some(self.binding()))?;if now.report!=self.original.report{return Err("Original sealed bytes/metadata/path observation changed".into())}Ok(())
 }
 pub(super) fn matches(&self,length:u64,digest:&str)->Result<(),String>{if length!=self.original.journal.length||digest!=self.original.journal.sha256{return Err("New current archive differs from original complete sealed bytes; no publication".into())}Ok(())}
}
