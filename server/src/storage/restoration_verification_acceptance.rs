// Genuine standard-source and target control for custody seam acceptance only.
// The sample actor is obtained from registered auth routes, never token minting.
// This does NOT publish a complete application verified-restore record.
#[cfg(test)]
mod verification_registered_acceptance {
 use super::*;use anyhow::{ensure,Context};
 use axum::{body::{Body,to_bytes},http::{Request,StatusCode},extract::ConnectInfo};
 use serde_json::{json,Value};use tower::ServiceExt;
 const KEY:&str="isolated-real-registered-verifier-authentication-key";
 async fn request(state:&crate::AppState,token:Option<&str>,method:&str,uri:&str,payload:Value)->anyhow::Result<(StatusCode,Value)> {
  let mut r=Request::builder().method(method).uri(uri).header("content-type","application/json");if let Some(token)=token {r=r.header("authorization",format!("Bearer {token}"));}
  let mut r=r.body(Body::from(serde_json::to_vec(&payload)?))?;
  r.extensions_mut().insert(ConnectInfo("127.0.0.1:61321".parse::<std::net::SocketAddr>()?));
  let response=crate::api_routes().with_state(state.clone()).oneshot(r).await?;let status=response.status();let bytes=to_bytes(response.into_body(),8*1024*1024).await?;
  Ok((status,if bytes.is_empty(){Value::Null}else{serde_json::from_slice(&bytes)?}))
 }
 async fn genuine_history_change(state:&crate::AppState,token:&str)->anyhow::Result<()> {
  use tokio::io::{AsyncReadExt,AsyncWriteExt};
  let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;let address=listener.local_addr()?;let app=crate::api_routes().merge(crate::sync::router()).with_state(state.clone());
  let server=tokio::spawn(async move {let _=axum::serve(listener,app).await;});
  let result=tokio::time::timeout(std::time::Duration::from_secs(10),async {
   let mut socket=tokio::net::TcpStream::connect(address).await?;
   socket.write_all(format!("GET /ws/sync HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").as_bytes()).await?;
   let mut header=Vec::new();while !header.ends_with(b"\r\n\r\n") {ensure!(header.len()<8192,"Owned WebSocket handshake exceeded bound");header.push(socket.read_u8().await?);}ensure!(header.starts_with(b"HTTP/1.1 101"),"Actual registered WebSocket upgrade unavailable");
   async fn send(socket:&mut tokio::net::TcpStream,text:&str)->anyhow::Result<()> {use tokio::io::AsyncWriteExt;let bytes=text.as_bytes();ensure!(bytes.len()<=8192,"Owned WebSocket frame too large");let mut frame=vec![0x81];if bytes.len()<126 {frame.push(0x80|bytes.len() as u8);}else{frame.push(0x80|126);frame.extend_from_slice(&(bytes.len() as u16).to_be_bytes());}let mask=[17,39,71,103];frame.extend_from_slice(&mask);frame.extend(bytes.iter().enumerate().map(|(i,b)|b^mask[i%4]));socket.write_all(&frame).await?;Ok(())}
   async fn receive(socket:&mut tokio::net::TcpStream)->anyhow::Result<Vec<u8>> {use tokio::io::AsyncReadExt;let opcode=socket.read_u8().await?;ensure!(opcode&0x80!=0&&opcode&15!=8,"Actual WebSocket closed or fragmented owned sample");let size=socket.read_u8().await?;ensure!(size&0x80==0,"Unexpected masked server frame");let length=match size&127 {126=>socket.read_u16().await? as usize,127=>{let n=socket.read_u64().await?;ensure!(n<=65536,"Owned WebSocket response exceeds bound");n as usize},n=>n as usize};ensure!(length<=65536,"Owned WebSocket response exceeds bound");let mut bytes=vec![0;length];socket.read_exact(&mut bytes).await?;Ok(bytes)}
   send(&mut socket,token).await?;let authenticated:Value=serde_json::from_slice(&receive(&mut socket).await?)?;ensure!(authenticated["status"]=="authenticated","Actual original bearer WebSocket authentication refused");
   send(&mut socket,&json!({"msg_type":"subscribe","doc_id":"genuine-page","user_id":"","payload":{}}).to_string()).await?;receive(&mut socket).await?;
   send(&mut socket,&json!({"msg_type":"change","doc_id":"genuine-page","user_id":"untrusted-caller-attribution","payload":{"original_binary_fixture":[0,1,2,255]}}).to_string()).await?;
   loop {let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM doc_versions WHERE document_id='genuine-page'").fetch_one(&state.db).await?;if count==1 {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
   socket.shutdown().await?;Ok::<(),anyhow::Error>(())
  }).await;
  server.abort();let _=server.await;result??;Ok(())
 }
 fn state(pool:sqlx::AnyPool,path:&Path)->anyhow::Result<crate::AppState> {Ok(crate::AppState{db:pool,jwt_secret:KEY.into(),base_url:"http://isolated.invalid".into(),search_index:crate::search::SearchIndex::new(path.join("search_index"))?,oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()})}
 #[tokio::test]
 #[ignore="owned single-process DATA_DIR and genuine trusted Python capture required"]
 async fn verification_registered_human_original_source_and_restored_target() -> anyhow::Result<()> { genuine_journey(0).await }
 #[tokio::test]
 #[ignore="owned original activated target pager and trusted host Python required"]
 async fn verification_activated_delete_pager_retains_original_journal() -> anyhow::Result<()> { genuine_journey(1).await }
 #[tokio::test]
 #[ignore="owned actual pre-pager equal-byte foreign SQLite FD acceptance"]
 async fn verification_activated_pager_refuses_foreign_main_fd_after_original_returns() -> anyhow::Result<()> {genuine_journey(2).await}
 #[tokio::test]
 #[ignore="owned genuine normal publication owner through admitted activated DELETE pager"]
 async fn verification_activated_pager_actual_normal_owner_and_exact_sql_ack() -> anyhow::Result<()> {genuine_journey(3).await}
 #[tokio::test]
 #[ignore="owned exact sealed-reader successor and actual registered page/history/property probes"]
 async fn verification_sealed_readers_actual_page_history_property_and_source_fences() -> anyhow::Result<()> {genuine_journey(4).await}
 #[tokio::test]
 #[ignore="owned sealed-reader real readonly write and equal-content main replacement refusal"]
 async fn verification_sealed_reader_refuses_write_and_equal_main_replacement() -> anyhow::Result<()> {genuine_journey(5).await}
 #[tokio::test]
 #[ignore="actual separate process target writer-window exclusion control"]
 async fn verification_target_startup_writer_excludes_actual_foreign_process()->anyhow::Result<()> {genuine_journey(7).await}
 #[tokio::test]
 #[ignore="owned original registered source and one-use kernel child admission"]
 async fn verification_original_target_fresh_executable_inherited_initial_custody()->anyhow::Result<()> {genuine_journey(6).await}
 #[tokio::test]
 #[ignore="actual normal production executable source producer and same-ELF worker TCP"]
 async fn verification_genuine_normal_executable_source_and_worker_tcp()->anyhow::Result<()> {genuine_journey(8).await}
 #[tokio::test]#[ignore="genuine queued actual-child forged version refusal"]async fn verification_queued_forged_frame_refuses()->anyhow::Result<()>{genuine_journey(10).await}
 #[tokio::test]#[ignore="genuine queued actual-child missing main descriptor refusal"]async fn verification_queued_missing_main_refuses()->anyhow::Result<()>{genuine_journey(11).await}
 #[tokio::test]#[ignore="genuine queued actual-child byte-equal foreign main descriptor refusal"]async fn verification_queued_foreign_main_refuses()->anyhow::Result<()>{genuine_journey(12).await}
 #[tokio::test]#[ignore="genuine original queued handoff failed-spawn consumes operation"]async fn verification_queued_failed_spawn_never_retries()->anyhow::Result<()>{genuine_journey(13).await}
 #[tokio::test]#[ignore="actual kernel startup progress pending original authority refusal"]async fn verification_pending_startup_actor_demoted()->anyhow::Result<()>{genuine_journey(14).await}
 #[tokio::test]#[ignore="actual kernel startup progress pending original authority refusal"]async fn verification_pending_startup_session_revoked()->anyhow::Result<()>{genuine_journey(15).await}
 #[tokio::test]#[ignore="actual kernel startup progress pending original authority refusal"]async fn verification_pending_startup_source_main_exchanged()->anyhow::Result<()>{genuine_journey(16).await}
 #[tokio::test]#[ignore="actual kernel startup progress pending original authority refusal"]async fn verification_pending_startup_foreign_phase_nonce()->anyhow::Result<()>{genuine_journey(17).await}
 #[tokio::test]#[ignore="actual kernel startup progress pending original authority refusal"]async fn verification_pending_startup_replayed_phase()->anyhow::Result<()>{genuine_journey(18).await}
 #[tokio::test]#[ignore="actual kernel startup progress pending original authority refusal"]async fn verification_pending_startup_foreign_phase_pid()->anyhow::Result<()>{genuine_journey(19).await}
 #[tokio::test]#[ignore="genuine separate restored target actual no-argument ordinary-main startup"]async fn verification_genuine_ordinary_main_source_and_registered_runtime()->anyhow::Result<()>{genuine_journey(20).await}
 #[tokio::test]#[ignore="genuine ordinary-main100MiB exact assets rebuilt search and configured background init"]async fn verification_genuine_ordinary_main_exact100mib_assets_search()->anyhow::Result<()>{genuine_journey(21).await}
 async fn ordinary_source_observe<T>(source:&mut StoppedVerificationSource<'_>,pages:&OriginalApplicationPlan<'_>,process:&mut std::process::Child,observation:impl std::future::Future<Output=anyhow::Result<T>>)->anyhow::Result<T>{
  tokio::pin!(observation);let deadline=tokio::time::sleep(std::time::Duration::from_secs(120));tokio::pin!(deadline);let mut pulse=tokio::time::interval(std::time::Duration::from_secs(3));pulse.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
  loop{tokio::select!{result=&mut observation=>{source.fence().await?;pages.fence(source)?;ensure!(process.try_wait()?.is_none(),"Original ordinary process exited during observation");return result;},_=&mut deadline=>anyhow::bail!("Ordinary source-fenced observation exceeded120s"),_=pulse.tick()=>{source.fence().await?;pages.fence(source)?;ensure!(process.try_wait()?.is_none(),"Original ordinary process exited while pending");}}}
 }
 async fn ordinary_exact_assets(source:&mut StoppedVerificationSource<'_>,pages:&OriginalApplicationPlan<'_>,assets:&OriginalApplicationAssets<'_>,process:&mut std::process::Child,address:std::net::SocketAddr,token:&str)->anyhow::Result<()> {
  use sha2::{Digest,Sha256};let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
  for asset in &assets.assets {assets.fence(source).await?;let uri=format!("http://{address}/api/v1/files/{}",asset.path.split('/').map(|v|urlencoding::encode(v).into_owned()).collect::<Vec<_>>().join("/"));
   ordinary_source_observe(source,pages,process,async{let mut response=client.get(&uri).bearer_auth(token).send().await?;ensure!(response.status()==reqwest::StatusCode::OK,"Ordinary exact owned asset refused");let mut offset=0u64;let mut sha=Sha256::new();let mut bytes=[0;65536];while let Some(chunk)=response.chunk().await?{ensure!(offset.checked_add(chunk.len()as u64).is_some_and(|v|v<=asset.length),"Ordinary asset exceeds original length");for part in chunk.chunks(bytes.len()){ensure!(physical_stamp(&asset.held)?==asset.stamp,"Original binary changed");asset.binding.fence_files().map_err(|e|anyhow::anyhow!(e))?;asset.held.read_exact_at(&mut bytes[..part.len()],offset)?;ensure!(part==&bytes[..part.len()],"Ordinary asset exact original bytes differ");sha.update(part);offset+=part.len()as u64;}}ensure!(offset==asset.length&&hex::encode(sha.finalize())==asset.sha,"Ordinary exact binary length/hash differs");Ok(())}).await?;
   assets.fence(source).await?;let response=client.get(&uri).send().await?;ensure!(matches!(response.status().as_u16(),401|403|404),"Ordinary anonymous private asset was served");assets.fence(source).await?;
  }Ok(())
 }
 async fn ordinary_rebuilt_search(source:&mut StoppedVerificationSource<'_>,pages:&OriginalApplicationPlan<'_>,process:&mut std::process::Child,address:std::net::SocketAddr,token:&str)->anyhow::Result<()> {
  let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;let mut queries=Vec::new();
  for page in &pages.pages {let word=page.body.split(|c:char|!c.is_ascii_alphanumeric()).find(|v|v.len()>=8&&v.len()<=100).context("Original page meaningful search sample absent")?;let uri=format!("/search?q={}&space_id={}&limit=100",urlencoding::encode(word),urlencoding::encode(&page.space));let(status,found)=ordinary_source_observe(source,pages,process,normal_tcp_json(&client,address,Some(token),&uri)).await?;ensure!(status==reqwest::StatusCode::OK&&found.as_array().is_some_and(|v|v.is_empty()),"Actual ordinary new derived search was not empty");queries.push((uri,page.id.clone(),page.space.clone(),word.to_owned()));}
  let rebuilt=ordinary_source_observe(source,pages,process,async{let mut response=client.post(format!("http://{address}/api/v1/admin/search/reindex")).bearer_auth(token).send().await?;ensure!(response.status()==reqwest::StatusCode::OK,"Ordinary registered rebuild refused");let mut bytes=Vec::new();while let Some(chunk)=response.chunk().await?{ensure!(bytes.len().checked_add(chunk.len()).is_some_and(|n|n<=65536),"Ordinary rebuild response exceeds bound");bytes.extend_from_slice(&chunk);}Ok(serde_json::from_slice::<serde_json::Value>(&bytes)?)}).await?;ensure!(rebuilt["status"]=="reindex_complete"&&rebuilt["documents_indexed"].as_u64()==Some(pages.pages.len()as u64),"Ordinary rebuilt coverage differs");
  for(uri,id,space,word)in queries {let(status,found)=ordinary_source_observe(source,pages,process,normal_tcp_json(&client,address,Some(token),&uri)).await?;ensure!(status==reqwest::StatusCode::OK&&found.as_array().is_some_and(|v|v.iter().any(|h|h["document_id"]==id&&h["space_id"]==space&&h["excerpt"].as_str().is_some_and(|s|s.contains(&word)))),"Ordinary rebuilt search lost exact original body/page");let(status,_)=ordinary_source_observe(source,pages,process,normal_tcp_json(&client,address,None,&uri)).await?;ensure!(matches!(status.as_u16(),401|403|404),"Ordinary anonymous private search allowed");}Ok(())
 }
 async fn genuine_ordinary_boot(original:&mut StoppedVerificationSource<'_>,plan:&OriginalApplicationPlan<'_>,drafts:&OriginalApplicationDraftPlan<'_>,target:&std::path::Path,output:&std::path::Path,token:&str,assets:Option<&OriginalApplicationAssets<'_>>)->anyhow::Result<()> {
  use std::os::unix::process::CommandExt;
  let target_root=Root::open(target)?;let target_main=target_root.leaf("starlane-notes.db")?;let root_id=identity(target_root.fd())?;let main_id=identity(&target_main)?;
  let executable_path=std::env::current_exe()?.parent().context("Test executable parent absent")?.parent().context("Normal executable parent absent")?.join("starlane-notes-server");
  let executable=File::open(executable_path)?;let stamp=physical_stamp(&executable)?;let md=executable.metadata()?;ensure!(md.is_file()&&md.uid()==unsafe{libc::geteuid()}&&md.mode()&0o022==0&&md.mode()&0o111!=0&&md.len()<=1024*1024*1024,"Selected ordinary ELF is not owned bounded executable code");let executable_hash=hash_file(&executable,md.len())?;let fd=executable.as_raw_fd();
  let port_owner=std::net::TcpListener::bind("127.0.0.1:0")?;let address=port_owner.local_addr()?;drop(port_owner);
  let mut command=std::process::Command::new(format!("/proc/self/fd/{fd}"));
  command.env("DATA_DIR",target).env("PORT",address.port().to_string()).env("BASE_URL",format!("http://{address}")).env("STARLANE_ENV","production").env("JWT_SECRET",KEY).env("RUST_LOG","info").stdin(std::process::Stdio::null()).stdout(File::create(output.join("stdout.log"))?).stderr(File::create(output.join("stderr.log"))?);
  if assets.is_some(){command.env("WORKFLOW_DEADLINE_ESCALATION_INTERVAL_MINUTES","1").env("WORKFLOW_DEADLINE_ESCALATION_ACTOR_ID",&plan.actor).env("WORKFLOW_DEADLINE_ESCALATION_TARGET","admin");}else{command.env_remove("WORKFLOW_DEADLINE_ESCALATION_INTERVAL_MINUTES").env_remove("WORKFLOW_DEADLINE_ESCALATION_ACTOR_ID").env_remove("WORKFLOW_DEADLINE_ESCALATION_TARGET");}
  unsafe{command.pre_exec(move||{let flags=libc::fcntl(fd,libc::F_GETFD);if flags<0||libc::fcntl(fd,libc::F_SETFD,flags&!libc::FD_CLOEXEC)<0{return Err(std::io::Error::last_os_error());}Ok(())});}
  original.fence().await?;plan.fence(original)?;let mut process=command.spawn()?;
  let observed=async {
   let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
   let until=tokio::time::Instant::now()+std::time::Duration::from_secs(90);
   loop {
    original.fence().await?;plan.fence(original)?;
    ensure!(process.try_wait()?.is_none(),"Actual ordinary main exited before registered health");
    if client.get(format!("http://{address}/api/v1/health")).send().await.is_ok_and(|r|r.status()==reqwest::StatusCode::OK){break;}
    ensure!(tokio::time::Instant::now()<until,"Actual ordinary main did not start within90s");tokio::time::sleep(std::time::Duration::from_millis(100)).await;
   }
   let actual_elf=File::open(format!("/proc/{}/exe",process.id()))?;ensure!(physical_stamp(&actual_elf)?==stamp&&hash_file(&actual_elf,actual_elf.metadata()?.len())?==executable_hash,"Actual ordinary process selected ELF differs");
   ensure!(identity(target_root.fd())?==root_id&&identity(&target_root.leaf("starlane-notes.db")?)?==main_id,"Ordinary startup exchanged selected restored root/main inode");
   ensure!(target.join("logging").is_dir()&&target.join("search_index").is_dir(),"Actual ordinary logging/search initialization missing");
   for page in &plan.pages {
    let uri=format!("/spaces/{}/documents/{}",urlencoding::encode(&page.space),urlencoding::encode(&page.id));
    for(suffix,expected)in [("",page.metadata.clone()),("/content",serde_json::json!({"content":page.body})),("/versions",serde_json::json!(page.history)),("/properties",serde_json::json!(page.properties))] {
     original.fence().await?;plan.fence(original)?;let(status,actual)=normal_tcp_json(&client,address,Some(token),&format!("{uri}{suffix}")).await?;original.fence().await?;plan.fence(original)?;
     ensure!(status==reqwest::StatusCode::OK,"Ordinary registered original page route refused");
     if suffix.is_empty(){for(k,v)in expected.as_object().context("Original page metadata absent")?{ensure!(actual.get(k)==Some(v),"Ordinary original metadata differs");}}else if suffix=="/content"{ensure!(actual["content"]==expected["content"],"Ordinary original bytes differ");}else{ensure!(actual==expected,"Ordinary original history/property presentation differs");}
     let(status,_)=normal_tcp_json(&client,address,None,&format!("{uri}{suffix}")).await?;original.fence().await?;ensure!(matches!(status.as_u16(),401|403|404),"Ordinary anonymous private page was served");
    }
   }
   for draft in &drafts.drafts {original.fence().await?;drafts.fence(original)?;let(status,actual)=normal_tcp_json(&client,address,Some(token),&draft.uri).await?;original.fence().await?;drafts.fence(original)?;ensure!(status==reqwest::StatusCode::OK&&actual==draft.expected,"Ordinary canonical draft/raw digest differs");}
   if let Some(assets)=assets {ensure!(assets.assets.len()==1&&assets.assets[0].length==100*1024*1024,"Meaningful genuine100MiB fixture absent");ordinary_exact_assets(original,plan,assets,&mut process,address,token).await?;ordinary_rebuilt_search(original,plan,&mut process,address,token).await?;}
   let logs=std::fs::read_to_string(output.join("stdout.log"))?;ensure!(logs.contains("Durable file logging enabled")&&logs.contains("Search index initialized")&&logs.contains("server listening"),"Ordinary main startup execution receipts incomplete");
   if assets.is_some(){ensure!(logs.contains("Workflow deadline escalation worker enabled"),"Configured ordinary background worker was not initialized");}
   original.fence().await?;plan.fence(original)?;ensure!(process.try_wait()?.is_none(),"Ordinary process exited during probes");Ok::<_,anyhow::Error>(())
  }.await;
  if process.try_wait()?.is_none(){process.kill()?;}let status=process.wait()?;
  observed?;eprintln!("ACTUAL_SAME_ELF_NO_ARGUMENT_ORDINARY_MAIN_RESTORE_BOOT_PAGE_HISTORY_PROPERTY_DRAFT_ANONYMOUS_PASS; terminated_by_test={}; record=false; assets_search_rebuild_audiences_pg_pending=true",!status.success());Ok(())
 }
 #[tokio::test]
 #[ignore="genuine original-source inode replacement during actual pending HTTP observation"]
 async fn verification_pending_http_original_source_replacement_cancels()->anyhow::Result<()> {genuine_journey(9).await}
 #[tokio::test]#[ignore="genuine normal source and restored worker viewer/editor/nonmember sessions"]
 async fn verification_genuine_normal_sqlite_sampled_audiences()->anyhow::Result<()>{genuine_journey(22).await}
 #[tokio::test]#[ignore="genuine original sampled session revocation during pending observation"]
 async fn verification_sqlite_sampled_session_pending_refuses()->anyhow::Result<()>{genuine_journey(23).await}
 #[tokio::test]#[ignore="genuine original sampled membership revocation during pending observation"]
 async fn verification_sqlite_sampled_membership_pending_refuses()->anyhow::Result<()>{genuine_journey(24).await}
 #[tokio::test]#[ignore="genuine original historical versions, replacement, cover, legacy and four audiences"]
 async fn verification_genuine_normal_sqlite_full_assets()->anyhow::Result<()>{genuine_journey(25).await}
 async fn sqlite_legacy_upload(state:&crate::AppState,token:&str,page:&str)->anyhow::Result<serde_json::Value>{let(space,doc)=page.strip_prefix("/spaces/").and_then(|s|s.split_once("/documents/")).context("Original legacy page selection invalid")?;let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;let address=listener.local_addr()?;let app=crate::registered_application(state.clone(),&format!("http://{address}"))?.merge(crate::spaces::attachments::verification_legacy_fixture_router().with_state(state.clone()));let server=tokio::spawn(async move{axum::serve(listener,app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await});let result=async{let bytes=b"LEGACY EXACT";let boundary="owned-original-legacy-handler";let mut body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"original-legacy.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();body.extend_from_slice(bytes);body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());let client=reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(15)).build()?;let mut response=client.post(format!("http://{address}/owned-legacy-fixture/{space}/{doc}")).bearer_auth(token).header("content-type",format!("multipart/form-data; boundary={boundary}")).body(body).send().await?;let status=response.status();let mut bytes=Vec::new();while let Some(chunk)=response.chunk().await?{ensure!(bytes.len()+chunk.len()<=65536,"Original legacy result exceeds64KiB");bytes.extend_from_slice(&chunk);}ensure!(status.is_success(),"Genuine original legacy handler refused:{status}");Ok::<_,anyhow::Error>(serde_json::from_slice(&bytes)?)}.await;server.abort();let _=server.await;result}
 async fn sqlite_small_native_upload(state:&crate::AppState,token:&str,uri:&str,name:&str,mime:&str,bytes:&[u8],expected:Option<(&str,i64)>)->anyhow::Result<serde_json::Value>{
  ensure!(bytes.len()<=65536,"Owned small exact fixture exceeds64KiB");let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;let address=listener.local_addr()?;let app=crate::registered_application(state.clone(),&format!("http://{address}"))?;let server=tokio::spawn(async move{axum::serve(listener,app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await});
  let result=async{let boundary="owned-small-exact-version-boundary";let mut body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: {mime}\r\n\r\n").into_bytes();body.extend_from_slice(bytes);body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());let client=reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(30)).build()?;let mut request=client.post(format!("http://{address}/api/v1{uri}")).bearer_auth(token).header("content-type",format!("multipart/form-data; boundary={boundary}")).header("x-starlane-upload-operation",uuid::Uuid::new_v4().to_string()).body(body);if let Some((version,revision))=expected{request=request.header("x-starlane-attachment-version",version).header("x-starlane-attachment-revision",revision);}let mut response=request.send().await?;let status=response.status();let mut result=Vec::new();while let Some(chunk)=response.chunk().await?{ensure!(result.len()+chunk.len()<=65536,"Small fixture response exceeds64KiB");result.extend_from_slice(&chunk);}ensure!(status.is_success(),"Actual small native upload/replacement refused: {status}; bounded error={}",String::from_utf8_lossy(&result).chars().take(256).collect::<String>());let value:serde_json::Value=serde_json::from_slice(&result)?;let actual_length=if expected.is_some(){value["version"]["byte_length"].as_u64()}else{value["size_bytes"].as_u64()};ensure!(actual_length==Some(bytes.len()as u64),"Actual small immutable byte length differs");Ok::<_,anyhow::Error>(value)}.await;server.abort();let _=server.await;result
 }
 #[tokio::test]#[ignore="genuine SQLite stopped source zero published/all-draft exact assets and supplied audiences"]async fn verification_genuine_sqlite_all_draft_worker()->anyhow::Result<()>{genuine_journey(26).await}
 #[tokio::test]#[ignore="genuine SQLite stopped mixed published/unpublished exact assets and supplied audiences"]async fn verification_genuine_sqlite_mixed_unpublished_worker()->anyhow::Result<()>{genuine_journey(27).await}
 #[tokio::test]#[ignore="genuine SQLite stopped empty workspace, actual no-op owner and current audience search"]async fn verification_genuine_sqlite_zero_active_worker()->anyhow::Result<()>{genuine_journey(28).await}
 #[tokio::test]#[ignore="genuine selected sealed-reader name-only native boundary"]async fn verification_reader_name_exact_names()->anyhow::Result<()>{genuine_journey(29).await}
 #[tokio::test]#[ignore="genuine selected sealed-reader name-only native boundary"]async fn verification_reader_name_journal()->anyhow::Result<()>{genuine_journey(30).await}
 #[tokio::test]#[ignore="genuine selected sealed-reader name-only native boundary"]async fn verification_reader_name_wal()->anyhow::Result<()>{genuine_journey(31).await}
 #[tokio::test]#[ignore="genuine selected sealed-reader name-only native boundary"]async fn verification_reader_name_shm()->anyhow::Result<()>{genuine_journey(32).await}
 #[tokio::test]#[ignore="genuine selected sealed-reader name-only native boundary"]async fn verification_reader_name_equal_main_and_return()->anyhow::Result<()>{genuine_journey(33).await}
 #[tokio::test]#[ignore="genuine selected sealed-reader name-only native boundary"]async fn verification_reader_name_same_inode_restored_mtime()->anyhow::Result<()>{genuine_journey(34).await}
 #[tokio::test]#[ignore="genuine selected sealed-reader name-only native boundary"]async fn verification_reader_name_unrelated_asset_final_refusal()->anyhow::Result<()>{genuine_journey(35).await}
 #[tokio::test]#[ignore="genuine selected sealed-reader name-only native boundary"]async fn verification_reader_name_control_record_final_refusal()->anyhow::Result<()>{genuine_journey(36).await}
 async fn genuine_journey(pager_only:u8) -> anyhow::Result<()> {
  use std::os::unix::fs::PermissionsExt;
  let _=tracing_subscriber::fmt().with_max_level(tracing::Level::ERROR).with_writer(std::io::stderr).try_init();
  let base=std::env::temp_dir().join(format!("verification-registered-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&base)?;std::fs::set_permissions(&base,std::fs::Permissions::from_mode(0o700))?;
  let ordinary_output=base.join("ordinary-output");if pager_only==20||pager_only==21{std::fs::create_dir(&ordinary_output)?;std::fs::set_permissions(&ordinary_output,std::fs::Permissions::from_mode(0o700))?;}
  let source=base.join("source");std::fs::create_dir(&source)?;std::fs::set_permissions(&source,std::fs::Permissions::from_mode(0o700))?;
  std::env::set_var("DATA_DIR",&source);std::env::set_var("SQLITE_ENABLED","true");crate::storage::init_data_dir().await?;
  let (pool,engine)=crate::db::init_pool().await?;crate::db::migrate_selected(&pool,engine).await?;let s=state(pool.clone(),&source)?;
  let (status,registered)=request(&s,None,"POST","/auth/register",json!({"email":"genuine@example.invalid","display_name":"Genuine original human","password":"Owned-disposable-strong-password"})).await?;
  ensure!(status.is_success(),"Actual registered first-user bootstrap failed: {status}");
  let (status,login)=request(&s,None,"POST","/auth/login",json!({"email":"genuine@example.invalid","password":"Owned-disposable-strong-password"})).await?;
  ensure!(status.is_success(),"Actual registered login failed: {status}");
  let token=login["access_token"].as_str().context("Actual login access credential absent")?.to_owned();let original_id=registered["user"]["id"].as_str().context("Registered original identity absent")?.to_owned();
  let (status,second_login)=request(&s,None,"POST","/auth/login",json!({"email":"genuine@example.invalid","password":"Owned-disposable-strong-password"})).await?;ensure!(status.is_success(),"Actual distinct session login failed");
  let distinct_session=second_login["access_token"].as_str().context("Actual distinct session absent")?.to_owned();
  let credential=VerificationHumanCredential::supplied(&token,KEY)?;
  assert!(VerificationHumanCredential::supplied(&token,"a-different-configured-secret-key-012345").is_err());
  let (status,space)=request(&s,Some(&token),"POST","/spaces",json!({"name":"Verifier private","slug":"verifier-private","visibility":"private"})).await?;ensure!(status.is_success(),"Actual source private space creation failed: {status}");let space_id=space["id"].as_str().context("Actual source space absent")?.to_owned();
  if pager_only==28 {
   let mut audiences=Vec::new();
   for(name,role)in [("viewer",Some("viewer")),("editor",Some("editor")),("nonmember",None),("grant",None)]{
    let email=format!("sqlite-empty-{name}@example.invalid");
    let(status,created)=request(&s,Some(&token),"POST","/admin/users",json!({"email":email,"display_name":format!("Actual empty SQLite {name}"),"role":"member"})).await?;ensure!(status.is_success(),"Original empty sampled user creation refused");
    let temporary=created["temporary_password"].as_str().context("Original empty provisioned credential absent")?;
    let(status,login)=request(&s,None,"POST","/auth/login",json!({"email":email,"password":temporary})).await?;ensure!(status.is_success(),"Original empty sampled login refused");
    if let Some(role)=role{let(status,_)=request(&s,Some(&token),"POST",&format!("/spaces/{space_id}/members"),json!({"user_id":login["user"]["id"],"role":role})).await?;ensure!(status.is_success(),"Original empty sampled membership refused");}
    audiences.push(login["access_token"].as_str().context("Original empty sampled access absent")?.to_owned());
   }
   for table in ["documents","document_drafts","attachments","doc_versions"]{let count:i64=sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}")).fetch_one(&s.db).await?;ensure!(count==0,"Genuine original empty workspace unexpectedly contains application data");}
   drop(s);pool.close().await;drop(pool);
   fn private_empty_tree(path:&Path)->anyhow::Result<()>{std::fs::set_permissions(path,std::fs::Permissions::from_mode(0o700))?;for e in std::fs::read_dir(path)?{let e=e?;if e.file_type()?.is_dir(){private_empty_tree(&e.path())?;}else{std::fs::set_permissions(e.path(),std::fs::Permissions::from_mode(0o600))?;}}Ok(())}private_empty_tree(&source)?;
   use std::io::Write;use std::process::Stdio;
   let executable=std::env::current_exe()?.parent().context("Test executable parent absent")?.parent().context("Normal executable parent absent")?.join("starlane-notes-server");
   let mut process=std::process::Command::new(&executable).args(["verify-sqlite-application-startup",source.to_str().context("Source path invalid")?,base.join("normal-journey").to_str().context("Workspace path invalid")?,"--application-version","v1.5.1","--writers-stopped"]).env("JWT_SECRET",KEY).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
   let mut input=process.stdin.take().context("Protected empty source credential input absent")?;input.write_all(&serde_json::to_vec(&json!({"producer":token,"audiences":audiences}))?)?;drop(input);
   let output=tokio::task::spawn_blocking(move||process.wait_with_output()).await??;ensure!(output.status.success(),"Genuine empty normal-worker journey refused:{}",String::from_utf8_lossy(&output.stderr));
   let report:Value=serde_json::from_slice(&output.stdout)?;ensure!(report["actual_normal_executable_startup"]==true&&report["registered_tcp_pages"]==0&&report["registered_tcp_unpublished_pages"]==0&&report["registered_tcp_assets"]==0&&report["registered_tcp_legacy_assets"]==0&&report["exact_attachment_bytes"]==0&&report["registered_audience_samples"]==4&&report["startup_sql_unchanged"]==true&&report["complete_verified_record"]==false,"Empty original workspace scope differs");
   eprintln!("GENUINE_EMPTY_SQLITE_NORMAL_RECOVERY_WORKER_REBUILD0_CURRENT_HUMAN_ANONYMOUS_DENIAL_PASS; complete_verified_record=false");return Ok(());
  }
  let page_uri=format!("/spaces/{space_id}/documents/genuine-page");let body="Original canonical producer body VERIFICATIONMARKER";
  let (status,page)=request(&s,Some(&token),"POST",&format!("/spaces/{space_id}/documents"),json!({"source_id":"genuine-page","title":"Genuine page","content":body})).await?;ensure!(status.is_success(),"Actual source page creation failed: {status}");
  let draft=page["draft_id"].as_str().context("Actual owned draft absent")?;
  if pager_only==35{sqlite_small_native_upload(&s,&token,&format!("{page_uri}/attachments"),"unrelated.bin","application/octet-stream",b"UNRELATED ORIGINAL NATIVE BYTES",None).await?;}
  let mut full_original_asset=None;let mut full_private_references=String::new();let mut full_cover=Value::Null;
  if pager_only==8||pager_only==21||pager_only==22||(25..=27).contains(&pager_only) {
   let prefix=b"--originalasset\r\nContent-Disposition: form-data; name=\"file\"; filename=\"same-original.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n";let tail=b"\r\n--originalasset--\r\n";let blocks=1600usize;let source=futures::stream::iter(std::iter::once(Ok::<_,std::io::Error>(axum::body::Bytes::from_static(prefix))).chain((0..blocks).map(|_|Ok(axum::body::Bytes::from(vec![7u8;65536])))).chain(std::iter::once(Ok(axum::body::Bytes::from_static(tail)))));
   let operation=uuid::Uuid::new_v4().to_string();let mut req=Request::builder().method("POST").uri(format!("{page_uri}/attachments")).header("authorization",format!("Bearer {token}")).header("x-starlane-upload-operation",&operation).header("content-type","multipart/form-data; boundary=originalasset").body(Body::from_stream(source))?;req.extensions_mut().insert(ConnectInfo("127.0.0.1:61322".parse::<std::net::SocketAddr>()?));
   let uploaded=crate::api_routes().with_state(s.clone()).oneshot(req).await?;ensure!(uploaded.status().is_success(),"Genuine original native attachment upload refused");let asset:Value=serde_json::from_slice(&to_bytes(uploaded.into_body(),65536).await?)?;ensure!(asset["size_bytes"].as_u64()==Some(100*1024*1024),"Actual streamed100MiB original upload length differs");let url=asset["url"].as_str().context("Actual exact original asset URL absent")?;
   let(status,selected)=request(&s,Some(&token),"GET",&format!("{page_uri}/drafts/{draft}"),Value::Null).await?;ensure!(status.is_success(),"Original owned draft context absent");
   let(status,_)=request(&s,Some(&token),"PUT",&format!("{page_uri}/drafts/{draft}"),json!({"source_digest":selected["source_digest"],"title":"Genuine page","content":format!("{body}\n\n![Exact original binary]({url})")})).await?;ensure!(status.is_success(),"Genuine original internal reference save refused");if (25..=27).contains(&pager_only){full_original_asset=Some(asset.clone());}
  }

  if pager_only!=26{let (status,_)=request(&s,Some(&token),"POST",&format!("{page_uri}/drafts/{draft}/publish"),json!({})).await?;ensure!(status.is_success(),"Actual original publication failed: {status}");
  genuine_history_change(&s,&token).await.context("Actual registered source WebSocket history production")?;}
  if (25..=27).contains(&pager_only) {
   let original=full_original_asset.as_ref().context("Genuine original full asset absent")?;let original_id=original["id"].as_str().context("Original full version absent")?;
   let(status,newdraft)=request(&s,Some(&token),"POST",&format!("{page_uri}/drafts"),json!({})).await?;ensure!(status.is_success(),"Original full draft creation refused");let selected=newdraft["id"].as_str().context("Original full draft absent")?;
   let replacement=sqlite_small_native_upload(&s,&token,&format!("{page_uri}/attachment-families/{original_id}/versions"),"same-original.bin","application/octet-stream",b"NEWER SAME NAME",Some((original_id,1))).await?;
   let newer_id=replacement["version"]["version_id"].as_str().context("Newer full version absent")?;ensure!(newer_id!=original_id&&replacement["version"]["original_name"]==original["original_name"],"Genuine replacement identity/name differs");
   let png=hex::decode("89504e470d0a1a0a0000000d4948445200000001000000010804000000b51c0c020000000b4944415478da6364f80f00010501012718e3660000000049454e44ae426082")?;
   let cover=sqlite_small_native_upload(&s,&token,&format!("{page_uri}/attachments"),"cover.png","image/png",&png,None).await?;let cover_id=cover["id"].as_str().context("Original full cover absent")?;
   let legacy=sqlite_legacy_upload(&s,&token,&page_uri).await?;let legacy_url=legacy["url"].as_str().context("Original full legacy URL absent")?;
   full_private_references=format!("[Historical100MiB](starlane-attachment://{original_id})\n[Newer15B](starlane-attachment://{newer_id})\n[Legacy12B]({legacy_url})");full_cover=json!(format!("starlane-attachment://{cover_id}"));
   let(status,_)=request(&s,Some(&token),"PUT",&format!("{page_uri}/drafts/{selected}"),json!({"source_digest":newdraft["source_digest"],"title":"Genuine page","cover_image":full_cover,"content":format!("{body}\n\n{full_private_references}")})).await?;ensure!(status.is_success(),"Original full references save refused");
   if pager_only!=26{let(status,_)=request(&s,Some(&token),"POST",&format!("{page_uri}/drafts/{selected}/publish"),json!({})).await?;ensure!(status.is_success(),"Original full references publish refused");}
  }
  let (status,original_content)=request(&s,Some(&token),"GET",&format!("{page_uri}/content"),Value::Null).await?;ensure!(status==StatusCode::OK,"Genuine original source canonical read unavailable");
  let expected_body=original_content["content"].as_str().context("Actual canonical original content absent")?.to_owned();
  let (status,property)=request(&s,Some(&token),"POST",&format!("/spaces/{space_id}/properties"),json!({"name":"Original exact property","property_type":"text","source_id":"original-property"})).await?;ensure!(status.is_success(),"Genuine original property schema route unavailable");let property_id=property["id"].as_str().context("Actual property identity absent")?;
  let (status,_)=request(&s,Some(&token),"PUT",&format!("{page_uri}/properties"),json!({"values":[{"property_id":property_id,"value":"EXACT ORIGINAL PROPERTY <opaque> \u{2603}"}]})).await?;ensure!(status.is_success(),"Genuine original property value route unavailable");
  if pager_only==8||pager_only==20||pager_only==21||(22..=27).contains(&pager_only) {
   let (status,draft)=request(&s,Some(&token),"POST",&format!("{page_uri}/drafts"),json!({})).await?;ensure!(status.is_success(),"Genuine original unpublished draft creation refused");let id=draft["id"].as_str().context("Actual original draft absent")?;
   let (status,saved)=request(&s,Some(&token),"PUT",&format!("{page_uri}/drafts/{id}"),json!({"source_digest":draft["source_digest"],"title":"Unpublished exact original draft","content":if (25..=27).contains(&pager_only){format!("PRIVATEUNPUBLISHEDDRAFT original canonical SQL content <opaque> \u{2603}\n{full_private_references}")}else{"PRIVATEUNPUBLISHEDDRAFT original canonical SQL content <opaque> \u{2603}".to_owned()}})).await?;
   ensure!(status.is_success()&&saved["content"].as_str().is_some_and(|v|v.starts_with("PRIVATEUNPUBLISHEDDRAFT")),"Genuine original unpublished draft save refused");
  }
  let mut audience_tokens=Vec::new();
  if (22..=27).contains(&pager_only) {
   for(name,role)in [("viewer",Some("viewer")),("editor",Some("editor")),("nonmember",None),("grant",None)] {
    let email=format!("sqlite-{name}@example.invalid");
    let(status,created)=request(&s,Some(&token),"POST","/admin/users",json!({"email":email,"display_name":format!("Actual SQLite {name}"),"role":"member"})).await?;
    ensure!(status.is_success(),"Genuine sampled user provisioning refused:{status}");
    let temporary=created["temporary_password"].as_str().context("Original provisioned credential absent")?;
    let(status,login)=request(&s,None,"POST","/auth/login",json!({"email":email,"password":temporary})).await?;ensure!(status.is_success(),"Genuine sampled login refused");
    if let Some(role)=role {let(status,_)=request(&s,Some(&token),"POST",&format!("/spaces/{space_id}/members"),json!({"user_id":login["user"]["id"],"role":role})).await?;ensure!(status.is_success(),"Original sampled membership refused:{status}");}
    let sampled=login["access_token"].as_str().context("Genuine sampled access absent")?;
    if name=="grant"&&pager_only!=26 {
     let proposal=json!({"user_id":login["user"]["id"],"action":"grant"});
     let(status,review)=request(&s,Some(&token),"POST",&format!("{page_uri}/viewer-grants/review"),proposal.clone()).await?;
     ensure!(status.is_success(),"Genuine original direct grant review refused:{status}");
     let(status,_)=request(&s,Some(&token),"POST",&format!("{page_uri}/viewer-grants/confirm"),json!({"proposal":proposal,"source_digest":review["source_digest"],"confirm":true})).await?;
     ensure!(status.is_success(),"Genuine original direct grant confirmation refused:{status}");
     let(status,metadata)=request(&s,Some(sampled),"GET",&page_uri,Value::Null).await?;
     ensure!(status==StatusCode::OK&&metadata["page_access_source"]=="direct_page_grant", "Actual source direct grant metadata did not exist");
     let(status,_)=request(&s,Some(sampled),"GET",&format!("{page_uri}/content"),Value::Null).await?;
     ensure!(status==StatusCode::OK,"Actual source direct grant content refused");
     let(status,_)=request(&s,Some(sampled),"GET",&format!("{page_uri}/versions"),Value::Null).await?;
     ensure!(matches!(status.as_u16(),401|403|404),"Direct published grant unexpectedly acquired version-history authority");
    }
    audience_tokens.push(sampled.to_owned());
   }
  }
  if pager_only==26||pager_only==27{let selected=if pager_only==27{let uri=format!("/spaces/{space_id}/documents/sqlite-unpublished");let(status,page)=request(&s,Some(&token),"POST",&format!("/spaces/{space_id}/documents"),json!({"source_id":"sqlite-unpublished","title":"UNPUBLISHEDSELECTION title","content":"PRIVATEUNPUBLISHEDSELECTION source body"})).await?;ensure!(status.is_success(),"Genuine SQLite mixed unpublished creation refused");let draft=page["draft_id"].as_str().context("Mixed original draft absent")?;let draft_uri=format!("{uri}/drafts/{draft}");let asset=sqlite_small_native_upload(&s,&token,&format!("{uri}/attachments"),"unpublished.bin","application/octet-stream",b"UNPUBLISHED EXACT ASSET",None).await?;let(status,current)=request(&s,Some(&token),"GET",&draft_uri,Value::Null).await?;ensure!(status.is_success(),"Selected mixed draft unavailable");let(status,_)=request(&s,Some(&token),"PUT",&draft_uri,json!({"source_digest":current["source_digest"],"title":"UNPUBLISHEDSELECTION title","content":format!("PRIVATEUNPUBLISHEDSELECTION source body\n\n[Exact unpublished](starlane-attachment://{})",asset["id"].as_str().context("Mixed original asset absent")?)})).await?;ensure!(status.is_success(),"Genuine mixed SQLite draft reference save refused");draft_uri}else{let(status,current)=request(&s,Some(&token),"GET",&page_uri,Value::Null).await?;ensure!(status.is_success(),"All-draft source metadata unavailable");format!("{page_uri}/drafts/{}",current["draft_id"].as_str().context("Original selected all-draft absent")?)};let sampled=audience_tokens.first().context("Original viewer absent")?;let(status,me)=request(&s,Some(sampled),"GET","/auth/me",Value::Null).await?;ensure!(status.is_success(),"Actual sampled viewer identity unavailable");let(status,_)=request(&s,Some(&token),"PUT",&format!("{selected}/access/{}",me["id"].as_str().context("Actual sampled viewer id absent")?),json!({"role":"viewer"})).await?;ensure!(status.is_success(),"Genuine SQLite selected draft sharing refused");let published:i64=sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL").fetch_one(&s.db).await?;let unpublished:i64=sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE published_at IS NULL AND archived_at IS NULL AND deleted_at IS NULL").fetch_one(&s.db).await?;ensure!(published==if pager_only==26{0}else{1}&&unpublished==1,"Actual SQLite active page census differs");let unpubassets:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachments a JOIN documents d ON d.id=a.document_id AND d.space_id=a.space_id WHERE d.published_at IS NULL").fetch_one(&s.db).await?;ensure!(unpubassets>=1,"Original SQLite unpublished native asset absent");}
  drop(s);pool.close().await;drop(pool);
  // No live search writer or source application survives stopped capture.
  fn private_tree(path:&Path)->anyhow::Result<()> {if std::fs::metadata(path)?.permissions().mode()&0o777!=0o700{std::fs::set_permissions(path,std::fs::Permissions::from_mode(0o700))?;}for e in std::fs::read_dir(path)?{let e=e?;if e.file_type()?.is_dir(){private_tree(&e.path())?;}else{if e.metadata()?.permissions().mode()&0o777!=0o600{std::fs::set_permissions(e.path(),std::fs::Permissions::from_mode(0o600))?;}}}Ok(())}private_tree(&source)?;
  if pager_only==8||pager_only==22||(25..=27).contains(&pager_only) {
   use std::io::Write;use std::process::Stdio;
   let executable=std::env::current_exe()?.parent().context("Test executable parent absent")?.parent().context("Normal executable parent absent")?.join("starlane-notes-server");
   let mut process=std::process::Command::new(&executable).args(["verify-sqlite-application-startup",source.to_str().context("Source path invalid")?,base.join("normal-journey").to_str().context("Workspace path invalid")?,"--application-version","v1.5.1","--writers-stopped"]).env("JWT_SECRET",KEY).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
   let mut input=process.stdin.take().context("Protected producer stdin absent")?;
   if pager_only==22||(25..=27).contains(&pager_only){input.write_all(&serde_json::to_vec(&json!({"producer":token,"audiences":audience_tokens}))?)?;}else{input.write_all(token.as_bytes())?;}drop(input);
   let output=tokio::task::spawn_blocking(move||process.wait_with_output()).await??;
   ensure!(output.status.success(),"Actual normal executable journey refused: {}",String::from_utf8_lossy(&output.stderr));
   let report:Value=serde_json::from_slice(&output.stdout)?;ensure!(report["actual_normal_executable_startup"]==true&&report["registered_tcp_pages"]==if pager_only==26{0}else{1}&&report["complete_verified_record"]==false&&report["registered_tcp_assets"]==(if pager_only==27{4}else if (25..=27).contains(&pager_only){3}else{1})&&report["exact_attachment_bytes"]==(if (25..=27).contains(&pager_only){100*1024*1024+15+68+12+if pager_only==27{23}else{0}}else{100*1024*1024}),"Normal executable must report exact bounded scope");
   if pager_only==26||pager_only==27{ensure!(report["registered_tcp_unpublished_pages"]==1&&report["startup_sql_unchanged"]==(pager_only==26),"Actual SQLite unpublished/no-op vs changed-owner scope differs");}
   if (25..=27).contains(&pager_only){ensure!(report["registered_tcp_legacy_assets"]==1,"Actual full legacy observation absent");}
   if pager_only==22||(25..=27).contains(&pager_only){ensure!(report["registered_audience_samples"]==4&&report["private_draft_only_queries"].as_u64().is_some_and(|n|n>0),"Actual sampled audience scope absent");}
   eprintln!("GENUINE_NORMAL_EXECUTABLE_PRODUCER_SAME_ELF_WORKER_ORDERED_RECOVERY_TCP_PAGE_HISTORY_PROPERTIES_DRAFT_EXACT100MIB_ASSET_PARSED_REF_REBUILT_SEARCH_PASS; sampled_audiences={}; complete_verified_record=false; full_assets_mode={}",pager_only==22||(25..=27).contains(&pager_only),(25..=27).contains(&pager_only));return Ok(());
  }
  if pager_only==23||pager_only==24 {
   use std::sync::atomic::{AtomicBool,Ordering};
   struct PendingDrop(Arc<AtomicBool>);impl Drop for PendingDrop{fn drop(&mut self){self.0.store(true,Ordering::SeqCst);}}
   let canceled=Arc::new(AtomicBool::new(false));let canceled_check=canceled.clone();
   let supplied=audience_tokens.iter().map(|t|VerificationSuppliedAudience::supplied(t,KEY)).collect::<anyhow::Result<Vec<_>>>()?;
   let first=supplied.first().context("Original sampled actor absent")?;let sid=first.auth.claims.sid.clone();let actor=first.auth.claims.sub.clone();
   let error=capture_sqlite_for_verification(&source,&base.join("sampled-pending-bundle"),"v1.5.1",true,credential.admin(),move|mut original,_|Box::pin(async move{
    let pages=original.application_page_plan().await?;let drafts=original.application_draft_plan().await?;let assets=original.application_asset_plan(&pages).await?;
    let audiences=original.application_audience_plan(&pages,&drafts,supplied).await?;
    ensure!(audiences.samples.len()==4&&audiences.samples[0].expected.pages==vec![true]&&audiences.samples[1].expected.pages==vec![true]&&audiences.samples[2].expected.published_pages==vec![false]&&audiences.samples[3].expected.pages==vec![false]&&audiences.samples[3].expected.published_pages==vec![true],"Original genuine sampled policy fixture differs");
    let(channel,_peer)=verification_worker_pair()?;let mut process=std::process::Command::new("/bin/sleep").arg("30").spawn()?;
    let future=async move{let _drop=PendingDrop(canceled);std::future::pending::<anyhow::Result<()>>().await};
    let result=NORMAL_PENDING_AUDIENCE_CHANGE.scope(std::cell::Cell::new(if pager_only==23{1}else{2}),normal_audience_observation(&mut original,&pages,&drafts,&audiences,&assets,&mut process,&channel,"original-sampled-negative",future)).await;
    let _=process.kill();let _=process.wait();ensure!(result.is_err()&&audiences.fence(&mut original,&pages,&drafts).await.is_err(),"Original sampled change did not cancel/refuse");
    Err::<(),_>(anyhow::anyhow!("EXPECTED_ORIGINAL_SQLITE_SAMPLED_REFUSAL"))
   })).await.expect_err("Changed original sampled source must refuse whole capture");
   ensure!(format!("{error:#}").contains("EXPECTED_ORIGINAL_SQLITE_SAMPLED_REFUSAL")&&canceled_check.load(Ordering::SeqCst),"Unexpected sampled source refusal or pending future not canceled");
   std::env::set_var("DATA_DIR",&source);let(pool,_)=crate::db::init_pool().await?;
   let sessions:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE id=$1").bind(&sid).fetch_one(&pool).await?;
   let memberships:i64=sqlx::query_scalar("SELECT COUNT(*) FROM space_members WHERE user_id=$1 AND space_id=$2").bind(&actor).bind(&space_id).fetch_one(&pool).await?;
   ensure!(sessions==1&&memberships==1,"Rejected sampled source change escaped original rollback");pool.close().await;
   eprintln!("ACTUAL_ORIGINAL_SQLITE_SAMPLED_SOURCE_PENDING_REFUSAL_ROLLBACK_PASS mode={pager_only}; child=sleep; record=false");return Ok(());
  }
  if pager_only==9 {
   use std::sync::atomic::{AtomicBool,Ordering};
   struct ObservationDrop(Arc<AtomicBool>);impl Drop for ObservationDrop {fn drop(&mut self){self.0.store(true,Ordering::SeqCst);}}
   let canceled=Arc::new(AtomicBool::new(false));let started=Arc::new(AtomicBool::new(false));let canceled_check=canceled.clone();let started_check=started.clone();
   let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;let address=listener.local_addr()?;
   let handler_started=started.clone();let app=axum::Router::new().route("/api/v1/pending",axum::routing::get(move||{let started=handler_started.clone();async move{started.store(true,Ordering::SeqCst);tokio::time::sleep(std::time::Duration::from_secs(60)).await;axum::Json(json!({"unexpected_success":true}))}}));
   let server=tokio::spawn(async move{axum::serve(listener,app).await});
   let selected=source.clone();let displaced=base.join("original-source-displaced");let bundle_path=base.join("pending-source-bundle");
   let result:anyhow::Result<()>=capture_sqlite_for_verification(&source,&bundle_path,"v1.5.1",true,credential.admin(),move|mut original,_bundle|Box::pin(async move{
    let plan=original.application_page_plan().await?;let(channel,_peer)=verification_worker_pair()?;
    let replacement=tokio::spawn(async move {tokio::time::sleep(std::time::Duration::from_secs(1)).await;std::fs::rename(&selected,&displaced)?;std::fs::create_dir(&selected)?;std::fs::set_permissions(&selected,std::fs::Permissions::from_mode(0o700))?;std::fs::copy(displaced.join("starlane-notes.db"),selected.join("starlane-notes.db"))?;std::fs::set_permissions(selected.join("starlane-notes.db"),std::fs::Permissions::from_mode(0o600))?;Ok::<_,anyhow::Error>(())});
    let client=reqwest::Client::builder().no_proxy().timeout(std::time::Duration::from_secs(120)).build()?;
    let observation=async move{let _drop=ObservationDrop(canceled);let _=normal_tcp_json(&client,address,None,"/pending").await?;Ok(())};
    let refused=normal_source_fenced_observation(&mut original,&plan,&channel,"original-pending-source-negative",observation).await;
    replacement.await??;ensure!(refused.is_err(),"Source inode replacement did not cancel pending observation");
    ensure!(started.load(Ordering::SeqCst),"Actual HTTP handler was never reached");
    anyhow::bail!("EXPECTED_ORIGINAL_SOURCE_PENDING_OBSERVATION_REFUSAL")
   })).await;
   server.abort();let _=server.await;
   ensure!(result.is_err()&&canceled_check.load(Ordering::SeqCst)&&started_check.load(Ordering::SeqCst),"Pending HTTP cancellation did not retain original source refusal");
   ensure!(format!("{:?}",result.err()).contains("EXPECTED_ORIGINAL_SOURCE_PENDING_OBSERVATION_REFUSAL"),"Negative failed before the intended original source observation");
   eprintln!("GENUINE_ORIGINAL_SOURCE_EQUAL_MAIN_INODE_REPLACEMENT_PENDING_HTTP_CANCELED; no record publication");return Ok(());
  }
  let bundle_path=base.join("standard-bundle");let target=base.join("restored");
  let target_copy=target.clone();let token_copy=token.clone();let page_uri_copy=page_uri.clone();let original_id_copy=original_id.clone();
  let foreign_main_path=base.join("original-equal-foreign-main.sqlite");
  let observed=capture_sqlite_for_verification(&source,&bundle_path,"v1.5.1",true,credential.admin(),move|mut original,bundle|Box::pin(async move {
   ensure!(original.current_human().await?==original_id_copy,"Original current human changed");original.fence().await?;
   let blocked=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rw",original.path().join("starlane-notes.db").display())).await?;let mut writer=blocked.acquire().await?;sqlx::query("PRAGMA busy_timeout=0").execute(&mut *writer).await?;
   ensure!(sqlx::query("UPDATE users SET role='member'").execute(&mut *writer).await.is_err(),"Original actual source writer was not excluded through verification continuation");drop(writer);blocked.close().await;
   let mut application_plan=original.application_page_plan().await?;ensure!(application_plan.pages[0].history.len()==1&&application_plan.pages[0].properties.len()==1,"Genuine original history/property samples must exist");
   // Genuine existing restore owner, never reconstructed sidecars or log proof.
   crate::storage::backup_restore::run(bundle.data_path().parent().context("Bundle root missing")?,&target_copy,"v1.5.1").await?;
   if pager_only==20||pager_only==21 {let drafts=original.application_draft_plan().await?;let assets=if pager_only==21{Some(original.application_asset_plan(&application_plan).await?)}else{None};genuine_ordinary_boot(&mut original,&application_plan,&drafts,&target_copy,&ordinary_output,&token_copy,assets.as_ref()).await?;ensure!(!target_copy.join(".verified-application-restores").exists(),"Engineering ordinary boot published verification record");return Ok(application_plan.pages.len());}
   std::env::set_var("DATA_DIR",&target_copy);
   let (target_pool,engine)=crate::db::init_pool().await?;crate::db::migrate_selected(&target_pool,engine).await?;activate_in(&target_copy,&target_pool).await.map_err(|e|anyhow::anyhow!("Actual activation refused: {e:?}"))?;
   let restored=state(target_pool.clone(),&target_copy)?;
   let mut initial=VerificationReservation::begin(&target_pool).await?;let initial_target=verification_target(&bundle,&target_copy,&target_pool,&mut initial).await?;
   let projection=engine_transfer::VerificationSqlProjection::original(&bundle)?;let initial_closure=projection.compare_exact(initial.connection()).await?;ensure!(!initial_closure.owner_acknowledged_noop(),"Exact SQL comparison cannot grant actual owner no-op acknowledgment");ensure!(initial_closure.source_sha256()==initial_closure.target_sha256(),"Lossless initial original SQL differs");drop(initial_target);let pager_anchor=target_pool.acquire().await?;initial.rollback().await?;
   let foreign_main=if pager_only==12{let path=foreign_main_path;std::fs::copy(target_copy.join("starlane-notes.db"),&path)?;std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600))?;Some(File::open(path)?)}else{None};
   let mut recovery_selection=OriginalTargetRecoverySelection::bind(&bundle,&target_copy,&target_pool).await?;drop(pager_anchor);
   if (14..=19).contains(&pager_only) {
    let root=identity(recovery_selection.active.root.fd())?;let database=identity(&recovery_selection.database)?;
    let(first,mut packet,pid)=normal_actual_progress_packet(if pager_only==17{"foreign"}else{"original"},root,database,pager_only==18)?;
    let admitted=normal_admit_startup_progress(&first,pid,"original",root,database,0,0,&mut original,&application_plan).await?;
    if pager_only==19 {let(_,foreign,foreign_pid)=normal_actual_progress_packet("original",root,database,false)?;ensure!(foreign_pid!=pid,"Foreign sender must be a genuinely separate process");packet=foreign;}
    if pager_only==14 {sqlx::query("UPDATE users SET role='member' WHERE id=?").bind(&original_id_copy).execute(&mut *original.conn).await?;ensure!(original.current_human().await.is_err(),"Pending original demotion must remove actual current human authority");}
    if pager_only==15 {let sid=original.admin.context("Original admin absent")?.claims.sid.clone();sqlx::query("DELETE FROM sessions WHERE id=?").bind(sid).execute(&mut *original.conn).await?;ensure!(original.current_human().await.is_err(),"Pending original session deletion must remove actual current human authority");}
    if pager_only==16 {let path=original.path().join("starlane-notes.db");let replacement=original.path().join("foreign-equal-main");std::fs::copy(&path,&replacement)?;std::fs::rename(&path,original.path().join("kept-original-main"))?;std::fs::rename(replacement,path)?;}
    let result=normal_admit_startup_progress(&packet,pid,"original",root,database,admitted.sequence,admitted.phase,&mut original,&application_plan).await;
    ensure!(result.is_err(),"Pending changed original authority or forged/replayed kernel phase advanced");
    ensure!(!target_copy.join(".verified-application-restores").exists(),"Refused progress published record");
    drop(restored);drop(recovery_selection);target_pool.close().await;
    return Err(anyhow::anyhow!("EXPECTED_PENDING_STARTUP_REFUSAL mode={pager_only}"));
   }
   if (10..=13).contains(&pager_only) {
    drop(recovery_selection._anchor.take());drop(restored);target_pool.close().await;recovery_selection.fence()?;original.fence().await?;
    let mut handoff=VerificationWorkerHandoff::prepare(&recovery_selection)?;handoff.actual_queued_refusal(pager_only,foreign_main.as_ref()).await?;original.fence().await?;
    eprintln!("ACTUAL_ORIGINAL_PRESPAWN_QUEUED_REFUSAL_ONE_USE_CONTROL_PASS mode={pager_only}; no record");return Ok(initial_closure.tables());
   }
   if pager_only==6 {
    let mut handoff=VerificationWorkerHandoff::prepare(&recovery_selection)?;
    handoff.actual_child_initial_read().await?;original.fence().await?;
    eprintln!("ACTUAL_ORIGINAL_TARGET_ONE_USE_PARENT_CHILD_KERNEL_ADMISSION_PASS; normal_application_worker=false; complete_verified_record=false");
    return Ok(initial_closure.tables());
   }
   if pager_only==3||pager_only==4||pager_only==5||pager_only==7||(29..=36).contains(&pager_only) {
    drop(restored);
    let writer=VerificationRecoveryWriter::enter(recovery_selection).await.context("Actual selected writer entry")?;writer.fence()?;
    if pager_only==7 {
     let mut owner_reservation=writer.selection().pool().acquire().await?;
     sqlx::query("BEGIN IMMEDIATE").execute(&mut *owner_reservation).await?;
     writer.fence()?;original.fence().await?;
     let original_bytes=std::fs::read(target_copy.join("starlane-notes.db"))?;
     let foreign=std::process::Command::new("/usr/bin/python3").args(["-I","-c", "import sqlite3,sys; c=sqlite3.connect(sys.argv[1],timeout=0.1);
try:
 c.execute(\"UPDATE documents SET title=title || 'FOREIGN OWNER WINDOW COMMIT' WHERE id='genuine-page'\"); c.commit(); sys.exit(42)
except sqlite3.OperationalError as e:
 sys.exit(0 if ('locked' in str(e).lower() or 'busy' in str(e).lower()) else 43)
finally:
 c.close()"]).arg(target_copy.join("starlane-notes.db")).output()?;
     ensure!(foreign.status.success(),"Actual foreign process committed during original startup writer reservation (exit {:?})",foreign.status.code());
     ensure!(std::fs::read(target_copy.join("starlane-notes.db"))?==original_bytes,"Foreign process changed original startup canonical bytes");
     sqlx::query("ROLLBACK").execute(&mut *owner_reservation).await?;drop(owner_reservation);
    }
    let recovered=crate::startup_recovery::recover_initialized(writer.selection().pool(),Some(writer.selection())).await;
    if recovered.is_err(){if let Some(pager)=&writer.selection().owner_pager {for (held,before) in &pager.parent_stamps {let after=physical_stamp(held)?;if &after!=before {eprintln!("ORIGINAL_ANCESTOR_REFUSAL path={} before={before:?} after={after:?}",std::fs::read_link(format!("/proc/self/fd/{}",held.as_raw_fd()))?.display());}}}}
    let recovery=recovered.context("Actual complete ordered normal recovery owners refused")?.context("Actual publication owner acknowledgment absent")?;
    ensure!(recovery.changes().count()==1,"Actual original owner receipt acknowledgment required");writer.fence()?;original.fence().await?;
    let mut reservation=VerificationReservation::begin(writer.selection().pool()).await?;
    let held=verification_target(&bundle,&target_copy,writer.selection().pool(),&mut reservation).await?;
    ensure!(projection.compare_exact(reservation.connection()).await.is_err(),"Actual owner timestamp cannot claim raw original SQL equality");
    let closure=projection.compare_after_publication_recovery(&mut reservation,&held,&recovery).await?;
    ensure!(closure.source_sha256()!=closure.target_sha256()&&closure.qualified_source_sha256()==closure.target_sha256(),"Exact original owner-qualified SQL closure required");ensure!(!closure.owner_acknowledged_noop(),"Actual changed receipt update cannot be relabeled as no-op");
    held.fence(&mut reservation).await?;original.fence().await?;
    let seal=writer.seal(&held,&mut reservation,&projection,&recovery).await?;
    ensure!(seal.reader_ready().is_err(),"Pre-close SQL snapshot cannot grant reader authority");
    drop(held);reservation.rollback().await?;drop(recovery);
    let sealed=writer.close(seal).await?;sealed.fence()?;original.fence().await?;
    if pager_only==4||pager_only==5||(29..=36).contains(&pager_only) {
      let reader=VerificationSealedReader::enter(sealed).await.context("Actual opaque sealed reader entry")?;reader.fence()?;original.fence().await?;
      if (29..=36).contains(&pager_only){verification_reader_actual_name_control(&reader,&target_copy,pager_only).await?;if pager_only==29{reader.close().await?;}else{reader.pool.close().await;drop(reader);}original.fence().await?;eprintln!("ACTUAL_SELECTED_READER_NAME_CONTROL_PASS mode={pager_only}; no completion record");return Ok(closure.tables());}

      let mut read_reservation=VerificationReservation::begin(&reader.pool).await.context("Actual sealed readonly SQL reservation")?;
      let read_target=verification_target(&bundle,&target_copy,&reader.pool,&mut read_reservation).await?;
      if pager_only==5 {
       ensure!(sqlx::query("UPDATE documents SET title=title || 'UNOWNED WRITE' WHERE id='genuine-page'").execute(read_reservation.connection()).await.is_err(),"Actual readonly successor must reject canonical write");
       let reads=(0..64).map(|_|async {sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM documents WHERE id='genuine-page'").fetch_one(&reader.pool).await});
       let observed=futures::future::try_join_all(reads).await.context("Bounded actual readonly pool pressure")?;
       ensure!(observed.iter().all(|count|*count==1),"Original canonical row changed during readonly pressure");reader.fence()?;original.fence().await?;
      }
      let ready=state(reader.pool.clone(),&target_copy)?;reader.fence()?;read_target.fence(&mut read_reservation).await?;
      verify_application_pages(&mut original,&application_plan,&read_target,&mut read_reservation,&ready,&token_copy).await.context("Actual readonly successor registered page/history/property probes")?;
      reader.fence()?;read_target.fence(&mut read_reservation).await?;original.fence().await?;
      if pager_only==5 {
       let original_bytes=std::fs::read(target_copy.join("starlane-notes.db"))?;
       let competing_process=std::process::Command::new("/usr/bin/python3").args(["-I","-c", "import sqlite3,sys; c=sqlite3.connect(sys.argv[1],timeout=0.1);
try:
 c.execute(\"UPDATE documents SET title=title || 'FRESH FOREIGN COMMIT' WHERE id='genuine-page'\"); c.commit(); sys.exit(42)
except sqlite3.OperationalError as e:
 sys.exit(0 if ('locked' in str(e).lower() or 'busy' in str(e).lower()) else 43)
finally:
 c.close()"])
        .arg(target_copy.join("starlane-notes.db")).output().context("Actual fresh-process ordinary SQLite writer")?;
       ensure!(competing_process.status.success(),"Fresh ordinary SQLite process must refuse commit under original selected read reservation (exit {:?})",competing_process.status.code());
       ensure!(std::fs::read(target_copy.join("starlane-notes.db"))?==original_bytes,"Fresh process changed original selected canonical bytes");
       reader.fence()?;read_target.fence(&mut read_reservation).await?;
       ensure!(!target_copy.join("starlane-notes.db-journal").exists(),"Foreign RESERVED start must not dirty original target journal inventory");
       use sqlx::Connection;
       let options=sqlx::sqlite::SqliteConnectOptions::new().filename(target_copy.join("starlane-notes.db")).create_if_missing(false).journal_mode(sqlx::sqlite::SqliteJournalMode::Delete).busy_timeout(std::time::Duration::from_millis(100));
       let mut competing=sqlx::SqliteConnection::connect_with(&options).await.context("Actual ordinary competing SQLite connection")?;
       ensure!(sqlx::query("UPDATE documents SET title=title || 'FOREIGN COMMIT' WHERE id='genuine-page'").execute(&mut competing).await.is_err(),"Actual ordinary writer cannot commit through selected readonly reservation");
       competing.close().await?;
       ensure!(std::fs::read(target_copy.join("starlane-notes.db"))?==original_bytes,"Ordinary connection changed original selected canonical bytes");
      }
      drop(read_target);read_reservation.rollback().await?;drop(ready);
      if pager_only==5 {
       let main=target_copy.join("starlane-notes.db");let held_original=target_copy.parent().context("Target parent absent")?.join("sealed-original-main.db");let foreign=target_copy.parent().context("Target parent absent")?.join("sealed-equal-foreign.db");
       std::fs::copy(&main,&foreign)?;ensure!(std::fs::read(&main)?==std::fs::read(&foreign)?,"Foreign control must have exact equal original bytes");
       std::fs::rename(&main,&held_original)?;std::fs::rename(&foreign,&main)?;
       ensure!(reader.fence().is_err(),"Equal-content foreign main must refuse original reader authority");
       std::fs::rename(&main,&foreign)?;std::fs::rename(&held_original,&main)?;
       ensure!(reader.fence().is_err(),"Original inode return cannot renew changed root selection");
       reader.pool.close().await;drop(reader);
       println!("ACTUAL_SEALED_READONLY_WRITE_AND_EQUAL_CONTENT_MAIN_REPLACEMENT_REFUSED; returning_original_refuses=true; foreign_bytes_retained=true");
      }else{reader.close().await?;}
      original.fence().await?;
      println!("ACTUAL_SEALED_READONLY_SUCCESSOR_CURRENT_HUMAN_AND_ANONYMOUS_PAGE_HISTORY_PROPERTY_PROBES_PASS; fresh_normal_worker=false; complete_verified_record=false");
    }

    println!("ACTUAL_ORDERED_NORMAL_RECOVERY_OWNERS_ORIGINAL_DELETE_PAGER_AND_EXACT_SQL_ACK_PASS; original_source_current=true; full_verified_record=false");
    return Ok(closure.tables());
   }
   if pager_only!=0 {
    use sqlx::Connection;
    let foreign=target_copy.parent().context("Target parent absent")?.join("equal-original-main-foreign.db");
    if pager_only==2 {std::fs::copy(target_copy.join("starlane-notes.db"),&foreign)?;}
    let pager=VerificationVfs::new(&recovery_selection)?;
    if pager_only==2 {
      *pager.state().open_fault.lock().map_err(|_|anyhow::anyhow!("Owned fault poisoned"))?=Some(foreign);
      ensure!(sqlx::AnyConnection::connect(&format!("sqlite://{}?mode=rw&vfs={}",pager.state().main_name,pager.name())).await.is_err(),"Foreign actual opened DB must refuse");
      ensure!(pager.state().rejected_foreign_main.load(std::sync::atomic::Ordering::SeqCst),"Actual underlying foreign main FD rejection required after visible original returns");
      ensure!(recovery_selection.fence().is_err(),"Returning original main cannot renew original directory selection");original.fence().await?;
      drop(pager);drop(recovery_selection);drop(restored);target_pool.close().await;
      println!("ACTUAL_FOREIGN_OPENED_MAIN_FD_REFUSED_AFTER_VISIBLE_ORIGINAL_RETURNS; foreign_bytes_retained=true; full_verified_record=false");return Ok(initial_closure.tables());
    }
    let mut conn=sqlx::AnyConnection::connect(&format!("sqlite://{}?mode=rw&vfs={}",pager.state().main_name,pager.name())).await?;
    pager.check()?;pager.permit_own_journal()?;
    let before=projection.compare_exact(&mut conn).await?;
    sqlx::query("PRAGMA synchronous=FULL").execute(&mut conn).await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut conn).await?;
    sqlx::query("UPDATE publication_receipts SET verified_at='bounded-owned-pager-control-rollback'").execute(&mut conn).await?;
    pager.check()?;
    sqlx::query("ROLLBACK").execute(&mut conn).await?;
    pager.check()?;
    let after=projection.compare_exact(&mut conn).await?;
    ensure!(before.source_sha256()==after.target_sha256(),"Actual owned pager rollback must preserve every original SQL cell");
    {let spool=pager.state().spool.lock().map_err(|_|anyhow::anyhow!("Owned spool poisoned"))?;spool.fence()?;ensure!(spool.records.len()==2,"Actual DELETE journal must be retained alongside original provenance");println!("ACTUAL_ACTIVATED_DELETE_PAGER_JOURNAL_RETAINED; mode_preserved=true; full_verified_record=false");}
    let mode:String=sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&mut conn).await?;ensure!(mode=="delete","Original journal mode cannot change");
    original.fence().await?;
    conn.close().await?;drop(pager);drop(recovery_selection);drop(restored);target_pool.close().await;
    return Ok(after.tables());
   }
   let recovery=crate::storage::publication::recover_for_verification(&recovery_selection).await.map_err(|e|anyhow::anyhow!("Actual publication recovery owner refused: {e:?}"))?;ensure!(recovery.changes().count()==1,"Genuine publication owner acknowledgment required");original.fence().await?;
   let mut reservation=VerificationReservation::begin(&target_pool).await?;
   let held=verification_target(&bundle,&target_copy,&target_pool,&mut reservation).await?;
   ensure!(projection.compare_exact(reservation.connection()).await.is_err(),"Startup timestamp changes cannot pass raw original SQL equality");
   let closure=projection.compare_after_publication_recovery(&mut reservation,&held,&recovery).await?;ensure!(closure.qualified_source_sha256()==closure.target_sha256()&&closure.source_sha256()!=closure.target_sha256(),"Exact owner-qualified SQL must retain distinct original source fingerprint");
   sqlx::query("SAVEPOINT actual_recovery_ack_negative").execute(reservation.connection()).await?;
   sqlx::query("UPDATE publication_receipts SET verified_at='caller-selected-timestamp'").execute(reservation.connection()).await?;
   ensure!(projection.compare_after_publication_recovery(&mut reservation,&held,&recovery).await.is_err(),"Actual owner acknowledgment cannot whitelist arbitrary verification timestamp");
   sqlx::query("ROLLBACK TO actual_recovery_ack_negative").execute(reservation.connection()).await?;
   sqlx::query("UPDATE publication_receipts SET manifest_json=manifest_json||' '").execute(reservation.connection()).await?;
   ensure!(projection.compare_after_publication_recovery(&mut reservation,&held,&recovery).await.is_err(),"Semantically equal receipt JSON cannot replace exact original raw receipt");
   sqlx::query("ROLLBACK TO actual_recovery_ack_negative").execute(reservation.connection()).await?;sqlx::query("RELEASE actual_recovery_ack_negative").execute(reservation.connection()).await?;
   projection.compare_after_publication_recovery(&mut reservation,&held,&recovery).await?;
   let (status,content)=request(&restored,Some(&token_copy),"GET",&format!("{page_uri_copy}/content"),Value::Null).await?;ensure!(status==StatusCode::OK&&content["content"]==expected_body,"Actual original-human canonical read differs: status={status}, fixture_body={content}");
   original.fence().await?;held.fence(&mut reservation).await?;
   ensure!(verify_application_pages(&mut original,&application_plan,&held,&mut reservation,&restored,&distinct_session).await.is_err(),"Another genuine current session of same actor cannot retarget original application proof");
   let genuine_history=application_plan.pages[0].history.clone();application_plan.pages[0].history[0]["author_name"]=json!("copied caller self-attestation");
   ensure!(verify_application_pages(&mut original,&application_plan,&held,&mut reservation,&restored,&token_copy).await.is_err(),"Historical API result cannot be replaced by caller flags or expected copied identity");application_plan.pages[0].history=genuine_history;
   let application_checks=verify_application_pages(&mut original,&application_plan,&held,&mut reservation,&restored,&token_copy).await?;ensure!(application_checks.page_count()==1&&valid_hash(application_checks.check_sha256()),"Actual private application page producer absent");
   let (status,denied)=request(&restored,None,"GET",&format!("{page_uri_copy}/content"),Value::Null).await?;ensure!(status==StatusCode::UNAUTHORIZED&&!denied.to_string().contains(body),"Actual anonymous private denial differs");
   projection.compare_after_publication_recovery(&mut reservation,&held,&recovery).await?;held.fence(&mut reservation).await?;original.fence().await?;
   // Actual original directory ABA is permanent refusal for this selection.
   // Even when the original path/inode returns, its observed custody stamp is
   // stale. Keep foreign evidence; never recursively remove someone else's root.
   let kept=target_copy.with_file_name("actual-original-root-kept");let foreign_kept=target_copy.with_file_name("actual-foreign-root-kept");
   std::fs::rename(&target_copy,&kept)?;std::fs::create_dir(&target_copy)?;std::fs::set_permissions(&target_copy,std::fs::Permissions::from_mode(0o700))?;
   fn copy_tree(source:&Path,dest:&Path)->anyhow::Result<()> {for e in std::fs::read_dir(source)?{let e=e?;let p=dest.join(e.file_name());if e.file_type()?.is_dir(){std::fs::create_dir(&p)?;std::fs::set_permissions(&p,e.metadata()?.permissions())?;copy_tree(&e.path(),&p)?;}else if e.file_type()?.is_file(){std::fs::copy(e.path(),&p)?;std::fs::set_permissions(&p,e.metadata()?.permissions())?;}}Ok(())}copy_tree(&kept,&target_copy)?;
   ensure!(held.fence(&mut reservation).await.is_err(),"Equal-content foreign target root must refuse original selection");
   std::fs::rename(&target_copy,&foreign_kept)?;std::fs::rename(&kept,&target_copy)?;
   ensure!(held.fence(&mut reservation).await.is_err(),"Original root inode returning cannot renew stale selected directory stamp");
   ensure!(recovery.fence(&target_pool,&target_copy).is_err(),"Actual recovery acknowledgment cannot renew target after root ABA");
   ensure!(verify_application_pages(&mut original,&application_plan,&held,&mut reservation,&restored,&token_copy).await.is_err(),"Actual application producer cannot renew original selection after root ABA");
   sqlx::query("UPDATE users SET display_name='foreign unapproved cell'").execute(reservation.connection()).await?;
   ensure!(projection.compare_exact(reservation.connection()).await.is_err(),"Original custody alone must not certify altered non-custody SQL cells");
   drop(restored);drop(held);reservation.rollback().await?;drop(recovery);drop(recovery_selection);target_pool.close().await;
   ensure!(!target_copy.join(".verified-application-restores").exists(),"Custody-only gate cannot self-attest full application verification");
   Ok(closure.tables())
  })).await;
  if pager_only==20||pager_only==21 {let count=observed?;ensure!(count>0,"Ordinary boot did not probe any original page");println!("ACTUAL_ORDINARY_MAIN_ORIGINAL_SOURCE_GUARDED_ENGINEERING_ACCEPTANCE_PASS; pages={count}; complete_record=false");return Ok(());}
  if (14..=19).contains(&pager_only) {let error=observed.expect_err("Pending refusal must cancel original capture publication");ensure!(format!("{error:#}").contains("EXPECTED_PENDING_STARTUP_REFUSAL"),"Unexpected pending control failure: {error:#}");ensure!(!base.join(".verified-application-restores").exists(),"Refused original continuation published verification record");println!("ACTUAL_KERNEL_PENDING_ORIGINAL_AUTHORITY_OR_PHASE_REFUSAL_PASS mode={pager_only}; no record");return Ok(());}
  let observed=observed?;
  ensure!(observed>50,"Complete original SQL table closure missing");
  // Same genuine original bearer cannot use its signed stale admin role to
  // capture output after current source demotion or original session revoke.
  let url=format!("sqlite://{}?mode=rw",source.join("starlane-notes.db").display());
  let current=sqlx::AnyPool::connect(&url).await?;
  sqlx::query("UPDATE users SET role='member' WHERE id=?").bind(&original_id).execute(&current).await?;current.close().await;
  let demoted=base.join("demoted-refused-bundle");
  ensure!(capture_sqlite_for_verification(&source,&demoted,"v1.5.1",true,credential.admin(),|_,_|Box::pin(async{Err::<(),_>(anyhow::anyhow!("Demoted original may never reach verifier continuation"))})).await.is_err(),"Stale signed administrator role must refuse actual source");ensure!(!demoted.exists(),"Current source demotion must refuse before final output");
  let current=sqlx::AnyPool::connect(&url).await?;
  sqlx::query("UPDATE users SET role='admin' WHERE id=?").bind(&original_id).execute(&current).await?;
  sqlx::query("DELETE FROM sessions WHERE id=?").bind(&credential.admin().claims.sid).execute(&current).await?;current.close().await;
  let revoked=base.join("revoked-refused-bundle");
  ensure!(capture_sqlite_for_verification(&source,&revoked,"v1.5.1",true,credential.admin(),|_,_|Box::pin(async{Err::<(),_>(anyhow::anyhow!("Revoked original may never reach verifier continuation"))})).await.is_err(),"Actual original session revocation must refuse");ensure!(!revoked.exists(),"Original session revocation must refuse before final output");
  println!("GENUINE_REGISTERED_HUMAN_STANDARD_CAPTURE_RESTORE_CUSTODY_PASS; stale_role_and_revoked_original_refuse=true; full_application_record=false");
  // Retain genuine new source/bundle/target as private acceptance evidence.
  Ok(())
 }
}
