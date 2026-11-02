// Genuine normal-executable producer, deliberately transport/startup scope only.
// This command cannot construct CompleteEvidence or publish verified records.
pub(crate) async fn verify_pg_ordinary_listener_startup(source:&Path,workspace:&Path)->anyhow::Result<()> {
    verify_pg_ordinary_selected(source,workspace,false).await
}
pub(crate) async fn verify_pg_ordinary_application(source:&Path,workspace:&Path)->anyhow::Result<()> {
    verify_pg_ordinary_selected(source,workspace,true).await
}
#[derive(serde::Deserialize)]#[serde(deny_unknown_fields)]
struct PgOrdinaryCredentials {administrator:String,audiences:Vec<String>}
fn pg_ordinary_credentials(bytes:&[u8],secret:&str)->anyhow::Result<(String,Vec<PgSuppliedAudience>)>{
    ensure!(bytes.len()<=65536,"Supplied original credentials exceed64KiB");
    let supplied:PgOrdinaryCredentials=serde_json::from_slice(bytes).map_err(|_|fail("Supplied original credential packet invalid"))?;
    ensure!(!supplied.audiences.is_empty()&&supplied.audiences.len()<=8,"Supply1..8 genuine audience credentials");
    VerificationHumanCredential::supplied(&supplied.administrator,secret)?;
    let mut audiences=Vec::new();for token in supplied.audiences{audiences.push(PgSuppliedAudience::supplied(&token,secret)?);}
    Ok((supplied.administrator,audiences))
}
async fn verify_pg_ordinary_selected(source:&Path,workspace:&Path,application:bool)->anyhow::Result<()> {
    use std::io::Read;use std::os::unix::{fs::OpenOptionsExt,process::CommandExt};
    let secret=std::env::var("JWT_SECRET").context("Actual source authentication configuration absent")?;
    let (token,audiences)=if application {let mut bytes=Vec::new();std::io::stdin().take(65537).read_to_end(&mut bytes)?;pg_ordinary_credentials(&bytes,&secret)?}else{let mut token=String::new();std::io::stdin().take(8194).read_to_string(&mut token)?;if token.ends_with('\n'){token.pop();if token.ends_with('\r'){token.pop();}}(token,Vec::new())};
    let credential=VerificationHumanCredential::supplied(&token,&secret)?;
    let url=crate::db::database_url()?;
    ensure!(workspace.is_absolute()&&source.is_absolute()&&!workspace.starts_with(source)&&!source.starts_with(workspace),"Ordinary diagnostic workspace overlaps source or is not absolute");
    std::env::set_var("DATA_DIR",source);
    let serving=open_restored_pg_serving(source,&url).await?;serving.fence().await?;
    {let mut connection=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut connection).await?;verification_source_human(&mut connection,credential.admin()).await?;serving.proof.verify_any_connection(&mut connection).await?;}
    let owned_workspace=ordinary_reserve_workspace(&serving.proof.root,workspace)?;serving.fence().await?;
    let store=match std::env::var("VERIFIED_RESTORE_RECORDS_DIR"){Ok(path)=>{ensure!(application,"Verification history publication requires full application mode");Some(PgVerificationStore::selected(Path::new(&path),&serving.proof.root,Some(&owned_workspace),&secret,true)?)},Err(std::env::VarError::NotPresent)=>None,Err(_)=>bail!("Verification history configuration invalid")};
    let target=workspace.join("application/target");let derived=workspace.join("derived");let backup=workspace.join("bundle");
    let (actor,record_operation)=capture_restored_postgres_for_verification(&serving,&backup,true,credential.admin(),move|mut original,backup|Box::pin(async move {
        original.fence().await?;backup.fence()?;owned_workspace.check()?;
        let plans=if application{Some(PgOrdinaryApplicationPlan::capture(&mut original,audiences).await?)}else{None};
        restore_qualified_postgres(backup.path(),&target,&url,true).await?;original.fence().await?;
        let selected=open_restored_pg_serving(&target,&url).await?;selected.fence().await?;
        // Create outputs before freezing the original listener target ancestry.
        let stdout=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(derived.join("ordinary.stdout.log"))?;let stderr=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(derived.join("ordinary.stderr.log"))?;
        let mut admission=OrdinaryListenerParent::prepare(&selected.proof.root).context("Qualified PG target listener transport selection refused")?;
        let (parent_channel,child_channel)=verification_worker_pair()?;admission.queue_once(&parent_channel)?;
        let fd=child_channel.as_raw_fd();let executable_fd=admission.executable.as_raw_fd();
        let schema=selected.proof.attestation.target.schema_name.clone();let mut database_url=reqwest::Url::parse(&url)?;database_url.query_pairs_mut().append_pair("options",&format!("-csearch_path={schema}"));
        let mut command=std::process::Command::new(format!("/proc/self/fd/{executable_fd}"));
        command.env(ORDINARY_LISTENER_CHANNEL,fd.to_string()).env("DATA_DIR",&target).env("SQLITE_ENABLED","false").env("DATABASE_URL",database_url.as_str()).env("JWT_SECRET",&secret).env("PORT",admission.wire.address.port().to_string()).env("BASE_URL",format!("http://{}",admission.wire.address)).env("STARLANE_ENV","production").stdin(std::process::Stdio::null()).stdout(stdout).stderr(stderr);
        unsafe{command.pre_exec(move||{for fd in [fd,executable_fd]{let flags=libc::fcntl(fd,libc::F_GETFD);if flags<0||libc::fcntl(fd,libc::F_SETFD,flags&!libc::FD_CLOEXEC)<0{return Err(io::Error::last_os_error());}}Ok(())});}
        let activity_started=chrono::Utc::now();original.fence().await?;selected.fence().await?;let mut child=command.spawn()?;drop(child_channel);
        let total_deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(1800);let result=match tokio::time::timeout_at(total_deadline,async {
            let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(90);let mut next_fence=tokio::time::Instant::now();
            loop {ensure!(child.try_wait()?.is_none(),"Actual no-argument ordinary child exited before readiness");
                if tokio::time::Instant::now()>=next_fence {original.fence().await?;selected.fence().await?;next_fence=tokio::time::Instant::now()+std::time::Duration::from_secs(3);}
                let mut poll=libc::pollfd{fd:parent_channel.as_raw_fd(),events:libc::POLLIN,revents:0};let status=unsafe{libc::poll(&mut poll,1,0)};ensure!(status>=0,"Ordinary kernel readiness poll failed");
                if status>0{original.fence().await?;admission.receive_ready_once(&parent_channel,&mut child)?;selected.fence().await?;original.fence().await?;break;}
                ensure!(tokio::time::Instant::now()<deadline,"Ordinary no-argument readiness exceeded90s");tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            let address=admission.wire.address;let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).no_proxy().redirect(reqwest::redirect::Policy::none()).build()?;
            let health=pg_live_observation(&mut original,&mut child,async{let response=client.get(format!("http://{address}/api/v1/health")).send().await?;ensure!(response.status()==reqwest::StatusCode::OK,"Actual ordinary health refused");Ok(())}).await?;let _=health;
            let actor=original.current_human().await?;
            let actual=pg_live_observation(&mut original,&mut child,async{let mut response=client.get(format!("http://{address}/api/v1/auth/me")).bearer_auth(&token).send().await?;ensure!(response.status()==reqwest::StatusCode::OK,"Actual ordinary current human refused");let mut bytes=Vec::new();while let Some(chunk)=response.chunk().await?{ensure!(bytes.len().checked_add(chunk.len()).is_some_and(|n|n<=65536),"Actual ordinary auth projection exceeds64KiB");bytes.extend_from_slice(&chunk);}Ok(serde_json::from_slice::<serde_json::Value>(&bytes)?)}).await?;
            ensure!(actual["id"]==actor,"Actual ordinary authenticated human differs from original source");
            if let Some(plans)=&plans{pg_ordinary_application_probes(&mut original,&selected,&mut child,address,&token,plans).await?;}
            admission.fence()?;original.fence().await?;selected.fence().await?;Ok::<_,anyhow::Error>(actor)
        }).await {Ok(result)=>result,Err(_)=>Err(anyhow::anyhow!("Ordinary application observation exceeded total1800s"))};
        // Retain the original listening socket until actual child termination,
        // even on probe failure. No dropped Child may outlive this scope.
        let _=child.kill();let terminated=child.wait();let actor=result?;ensure!(terminated.is_ok(),"Actual ordinary child termination unconfirmed");
        admission.fence()?;original.fence().await?;selected.fence().await?;owned_workspace.check()?;let tested_executable=admission.executable.try_clone()?;drop(admission);let mut record_operation=None;
        if let Some(plans)=&plans{let mut exercised=std::collections::BTreeSet::new();exercised.insert(original.admin.claims.sid.clone());for sample in &plans.audience.samples{exercised.insert(sample.credential.auth.claims.sid.clone());}let mut final_target=PgFinalObservedTarget::hold(selected,&mut original,backup,&exercised,activity_started,chrono::Utc::now()).await?;final_target.fence().await?;original.fence().await?;ensure!(!final_target.activity.is_empty(),"Actual authenticated target activity was not observed");if let Some(store)=&store{let complete=PgCompleteApplicationEvidence::after_actual_probes(&mut original,&mut final_target,backup,plans,&tested_executable).await?;record_operation=Some(store.publish(complete,&mut original,&mut final_target).await?);}
 final_target.tx.rollback().await?;}
        Ok((actor,record_operation))
    })).await?;
    println!("{}",serde_json::to_string(&serde_json::json!({"scope":if application{"original_source_bound_ordinary_application_probes"}else{"original_source_bound_ordinary_listener_startup_and_current_human"},"actor_id":actor,"complete_record":false,"final_target_sql_exclusion_qualified":application,"historical_record_operation":record_operation}))?);Ok(())
}

// A directory census is exclusion evidence only; it grants no source reads,
// engine role, writer lease or permission to mutate the original source.
struct OrdinarySourceDirectories<'a>{root:&'a Root,dirs:Vec<(String,File,PhysicalStamp)>,entries:usize}
impl<'a> OrdinarySourceDirectories<'a>{
 fn hold(root:&'a Root)->anyhow::Result<Self>{
  fn walk(root:&File,path:&str,depth:usize,dirs:&mut Vec<(String,File,PhysicalStamp)>,entries:&mut usize)->anyhow::Result<()>{
   ensure!(depth<=64&&dirs.len()<4096&&path.len()<=8192,"Original directory census exceeds supported bounds");
   let stamp=physical_stamp(root)?;dirs.push((path.into(),root.try_clone()?,stamp));
   for entry in std::fs::read_dir(format!("/proc/self/fd/{}",root.as_raw_fd()))?{*entries=entries.checked_add(1).context("Original directory entry count overflow")?;ensure!(*entries<=100000,"Original directory census exceeds100000 entries");let entry=entry?;let name=entry.file_name().into_string().map_err(|_|fail("Original directory name invalid"))?;ensure!(valid_relative(&name)&&!name.contains(['/','\\']),"Original directory component unsafe");let kind=entry.file_type()?;
    if kind.is_dir(){let selected=open_at(root,&name,libc::O_RDONLY|libc::O_DIRECTORY)?;let path=if path.is_empty(){name}else{format!("{path}/{name}")};let id=identity(&selected)?;for(_,prior,_)in dirs.iter(){ensure!(identity(prior)?!=id,"Original source has aliased/cyclic directory topology");}walk(&selected,&path,depth+1,dirs,entries)?;}
    else{ensure!(kind.is_file(),"Original source contains unsupported link/special topology");}
   }Ok(())
  }
  ordinary_listener_root_identity(root)?;let mut value=Self{root,dirs:Vec::new(),entries:0};walk(root.fd(),"",0,&mut value.dirs,&mut value.entries)?;value.fence()?;Ok(value)
 }
 fn fence(&self)->anyhow::Result<()>{ordinary_listener_root_identity(self.root)?;for(path,held,stamp)in &self.dirs{let current=if path.is_empty(){self.root.fd().try_clone()?}else{destination_directory(self.root.fd(),path)?};ensure!(physical_stamp(held)?==*stamp&&physical_stamp(&current)?==*stamp,"Original source directory census changed");}Ok(())}
 fn outside(&self,parent:&Root)->anyhow::Result<()>{self.fence()?;parent.check()?;for ancestor in &parent.chain{let id=identity(ancestor)?;for(_,source,_)in &self.dirs{ensure!(identity(source)?!=id,"Workspace parent is inside original source native directory tree");}}self.fence()}
}
fn ordinary_mkdir_private(parent:&File,name:&str)->anyhow::Result<File>{
 ensure!(valid_relative(name)&&!name.contains(['/','\\']),"Workspace leaf unsafe");let name=CString::new(name)?;
 ensure!(unsafe{libc::mkdirat(parent.as_raw_fd(),name.as_ptr(),0o700)}==0,"New private workspace leaf unavailable");
 let created=open_at(parent,name.to_str()?,libc::O_RDONLY|libc::O_DIRECTORY)?;let md=created.metadata()?;ensure!(md.uid()==unsafe{libc::geteuid()}&&md.mode()&0o7777==0o700,"New workspace leaf not private/current-owner");parent.sync_all()?;Ok(created)
}
fn ordinary_reserve_workspace(source:&Root,path:&Path)->anyhow::Result<Root>{
 let parent=Root::open(path.parent().context("Workspace parent absent")?)?;parent.check()?;
 let md=parent.fd().metadata()?;ensure!(md.uid()==unsafe{libc::geteuid()}&&md.mode()&0o7777==0o700,"Workspace parent must be private0700 and current-owner");
 let census=OrdinarySourceDirectories::hold(source)?;census.outside(&parent)?;
 #[cfg(test)]ordinary_workspace_precreate_checkpoint()?;census.outside(&parent)?;
 let name=path.file_name().and_then(|v|v.to_str()).context("Workspace leaf invalid")?;let created=ordinary_mkdir_private(parent.fd(),name)?;
 parent.check()?;census.fence()?;let owned=Root::open(path)?;ensure!(identity(owned.fd())?==identity(&created)?,"Created workspace inode replaced");owned.check()?;
 ordinary_mkdir_private(owned.fd(),"application")?;owned.check()?;ordinary_mkdir_private(owned.fd(),"derived")?;owned.check()?;parent.check()?;census.fence()?;Ok(owned)
}

#[cfg(test)]std::thread_local!{static ORDINARY_WORKSPACE_PRECREATE:std::cell::RefCell<Option<Box<dyn FnOnce()->anyhow::Result<()>>>>=std::cell::RefCell::new(None);}
#[cfg(test)]fn ordinary_workspace_precreate_checkpoint()->anyhow::Result<()>{let hook=ORDINARY_WORKSPACE_PRECREATE.with(|slot|slot.borrow_mut().take());if let Some(hook)=hook{hook()?;}Ok(())}
#[cfg(test)]mod ordinary_workspace_controls {
 use super::*;use std::os::unix::fs::DirBuilderExt;
 fn private(parent:&Path,name:&str)->anyhow::Result<PathBuf>{let path=parent.join(format!("{name}-{}",uuid::Uuid::new_v4()));let mut builder=std::fs::DirBuilder::new();builder.mode(0o700);builder.create(&path)?;Ok(path)}
 #[test]fn ordinary_workspace_native_source_descendant_refuses_before_creation()->anyhow::Result<()>{let base=private(&std::env::temp_dir(),"workspace-source")?;let child=private(&base,"original")?;let source=Root::open(&base)?;let before=physical_stamp(source.fd())?;let child_before=physical_stamp(Root::open(&child)?.fd())?;let output=child.join("must-not-create");ensure!(ordinary_reserve_workspace(&source,&output).is_err(),"Original native source descendant granted output ownership");ensure!(!output.exists()&&physical_stamp(source.fd())?==before&&physical_stamp(Root::open(&child)?.fd())?==child_before,"Rejected source selection mutated source namespace");Ok(())}
 #[test]fn ordinary_workspace_held_foreign_parent_replacement_refuses()->anyhow::Result<()>{let base=private(&std::env::temp_dir(),"workspace-foreign")?;let source_path=private(&base,"source")?;let parent=private(&base,"selected")?;let source=Root::open(&source_path)?;let moved=base.join("original-selected-retained");let selected=parent.clone();let old=identity(Root::open(&parent)?.fd())?;
 struct Reset;impl Drop for Reset{fn drop(&mut self){ORDINARY_WORKSPACE_PRECREATE.with(|slot|{slot.borrow_mut().take();});}}let _reset=Reset;
 ORDINARY_WORKSPACE_PRECREATE.with(|slot|{*slot.borrow_mut()=Some(Box::new(move||{std::fs::rename(&selected,&moved)?;let mut builder=std::fs::DirBuilder::new();builder.mode(0o700);builder.create(&selected)?;ensure!(identity(Root::open(&selected)?.fd())?!=old,"Foreign parent must have distinct inode");Ok(())}));});
 let output=parent.join("must-not-create");ensure!(ordinary_reserve_workspace(&source,&output).is_err()&&!output.exists(),"Foreign selected parent received workspace output");Ok(())}
 #[test]fn ordinary_workspace_real_external_filesystem_and_no_clobber()->anyhow::Result<()>{let base=private(&std::env::temp_dir(),"workspace-external-source")?;let source=Root::open(&base)?;let parent=private(Path::new("/dev/shm"),"workspace-external-output")?;let parent_root=Root::open(&parent)?;ensure!(source.fd().metadata()?.dev()!=parent_root.fd().metadata()?.dev(),"External fixture must use genuinely separate filesystem");let before=physical_stamp(source.fd())?;let output=parent.join("new-owned-workspace");let owned=ordinary_reserve_workspace(&source,&output)?;owned.check()?;ensure!(ordinary_reserve_workspace(&source,&output).is_err(),"Existing destination accepted");ensure!(physical_stamp(source.fd())?==before,"External workspace changed source");Ok(())}
 #[test]fn ordinary_workspace_unknown_symlink_topology_refuses()->anyhow::Result<()>{let base=private(&std::env::temp_dir(),"workspace-link-source")?;let source_path=private(&base,"source")?;let parent=private(&base,"output-parent")?;std::os::unix::fs::symlink(&parent,source_path.join("foreign-alias"))?;let source=Root::open(&source_path)?;let output=parent.join("must-not-create");ensure!(ordinary_reserve_workspace(&source,&output).is_err()&&!output.exists(),"Unknown source link topology granted workspace exclusion");Ok(())}
}
