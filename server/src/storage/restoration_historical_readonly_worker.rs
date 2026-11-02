// Purpose-private historical READONLY process. This is not ordinary main,
// CurrentServing, source-reference exclusion, a WriterLease or record authority.
#[derive(Clone,serde::Serialize,serde::Deserialize)]
#[serde(tag="role",deny_unknown_fields)]
enum HistoricalReadonlyRoute { StorageHealth, SearchReindex{published_source_digest:String,count:usize},SearchPublished{space:String,term:String,page:Option<String>,published_source_digest:String},CurrentHuman, CurrentFile{asset_id:String,storage_path:String,length:u64,sha256:String},
 PageMetadata{space:String,page:String,row_sha256:String},
 PageContent{space:String,page:String,row_sha256:String,content_path:String,length:u64,sha256:String},
 Draft{space:String,page:String,draft:String,source_digest:String},
 PropertySchema{space:String,rows_sha256:String},PropertyValues{space:String,page:String,rows_sha256:String},
 PinnedMetadata{space:String,page:String,version:String,family:String},PinnedFile{space:String,page:String,version:String,family:String,length:u64,sha256:String},
}
struct HistoricalOriginalReadPlan {human:String,session:String,routes:Vec<HistoricalReadonlyRoute>}
impl HistoricalOriginalReadPlan {
 async fn from_original(source:&mut StoppedHistoricalVerificationSource<'_>,selected:Option<&HistoricalDataSelection>,observe_usage:bool)->anyhow::Result<Self>{
  use sqlx::Row;source.fence().await?;let human=source.current_human().await?;let session=source.original.admin.as_ref().ok_or_else(||fail("Original historical verified administrator absent"))?.claims.sid.clone();anyhow::ensure!(historical_selected_id(&session),"Original historical session is not bounded/canonical");
  let rows=sqlx::query("SELECT id,storage_path,size_bytes FROM attachments ORDER BY id").fetch_all(&mut *source.original.conn).await?;
  let mut routes=vec![HistoricalReadonlyRoute::CurrentHuman];
  // Initial actual matrix is finite: genuine human and <=8 current small files.
  anyhow::ensure!(rows.len()<=8,"Historical initial current-file matrix exceeds8");
  for row in rows{let asset_id:String=row.try_get("id")?;let storage_path:String=row.try_get("storage_path")?;let length:i64=row.try_get("size_bytes")?;
   anyhow::ensure!(length>=0&&length<=1024*1024&&valid_relative(&storage_path)&&storage_path.bytes().all(|b|b.is_ascii_alphanumeric()||b"/._-".contains(&b)),"Initial historical file matrix is not small/canonical");
   let(file,stamp,sha)=source.original.inventory.files.get(&storage_path).ok_or_else(||fail("Original source held current file absent"))?;
   anyhow::ensure!(stamp.length==length as u64&&physical_stamp(file)?==*stamp&&hash_file(file,stamp.length)?==*sha,"Original current file bytes differ");
   routes.push(HistoricalReadonlyRoute::CurrentFile{asset_id,storage_path,length:length as u64,sha256:sha.clone()});
  }
  if let Some(selected)=selected{routes.extend(historical_source_data_routes(source,selected).await?);}
  if observe_usage{routes.push(HistoricalReadonlyRoute::StorageHealth);}
  source.fence().await?;Ok(Self{human,session,routes})
 }
}
#[derive(serde::Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]
struct HistoricalReadonlyChildWire{version:u32,purpose:String,nonce:String,path:PathBuf,chain_stamps:Vec<String>,main_stamp:String,inventory_sha256:String,projection_sha256:String,executable_stamp:String,executable_sha256:String,derived_path:PathBuf,derived_identity:Identity,listener_identity:Identity,address:std::net::SocketAddr,human:String,session:String,routes:Vec<HistoricalReadonlyRoute>}
fn historical_child_inventory_sha(tree:&HistoricalFreshInventory)->io::Result<String>{
 let dirs:Vec<_>=tree.directories.iter().map(|(n,(_,s))|(n,format!("{s:?}"))).collect();let files:Vec<_>=tree.files.iter().map(|(n,f)|(n,format!("{:?}",f.stamp()),&f.bytes().sha256,f.bytes().size_bytes)).collect();let raw=serde_json::to_vec(&(format!("{:?}",tree.stamp),dirs,files)).map_err(io::Error::other)?;if raw.len()>8*1024*1024{return Err(fail("Historical child inventory exceeds bounded8MiB"));}Ok(digest(&raw))
}
// Output objects are reserved BEFORE immutable target ancestor stamps are admitted.
// They carry no target reader/writer authority.
struct HistoricalReadonlyChildWorkspace{root:Root,log:File}
impl HistoricalReadonlyChildWorkspace{
 async fn reserve(source:&mut StoppedHistoricalVerificationSource<'_>,ready:&BootReadyDestination)->anyhow::Result<Self>{
  use std::os::unix::fs::OpenOptionsExt;source.fence().await?;ready.fence()?;
  let path=ready.path().with_file_name(format!("historical-readonly-derived-{}",uuid::Uuid::new_v4()));
  anyhow::ensure!(!path.starts_with(source.path())&&!source.path().starts_with(&path)&&!path.starts_with(ready.path())&&!ready.path().starts_with(&path),"Historical derived workspace overlaps originals");
  std::os::unix::fs::DirBuilderExt::mode(&mut std::fs::DirBuilder::new(),0o700).create(&path)?;let root=Root::open(&path)?;let log=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path.join("actual-child.log"))?;
  source.fence().await?;ready.fence()?;Ok(Self{root,log})
 }
}
struct HistoricalReadonlyChildHandoff{authority:Arc<HistoricalReadonlyAdmission>,executable:File,derived:Root,log:File,listener:File,wire:HistoricalReadonlyChildWire,sent:bool}
impl HistoricalReadonlyChildHandoff {
 async fn prepare(target:&HistoricalReadonlySelectedTarget,source:&mut StoppedHistoricalVerificationSource<'_>,workspace:HistoricalReadonlyChildWorkspace)->anyhow::Result<Self>{
  Self::prepare_selected(target,source,workspace,None,false).await
 }
 async fn prepare_selected(target:&HistoricalReadonlySelectedTarget,source:&mut StoppedHistoricalVerificationSource<'_>,workspace:HistoricalReadonlyChildWorkspace,selected:Option<&HistoricalDataSelection>,observe_usage:bool)->anyhow::Result<Self>{
  use std::os::fd::FromRawFd;source.fence().await?;target.reader.fence()?;
  let HistoricalReadonlyReaderAuthority::Original(authority)=&target.reader.vfs.as_ref().ok_or_else(||fail("Original historical VFS absent"))?.state().authority else{return Err(fail("Child handoff requires genuine original publisher").into())};
  let plan=HistoricalOriginalReadPlan::from_original(source,selected,observe_usage).await?;
  let HistoricalReadonlyChildWorkspace{root:derived,log}=workspace;derived.check()?;let derived_path=derived.absolute.clone();
  anyhow::ensure!(derived_path.is_absolute()&&!derived_path.starts_with(source.path())&&!derived_path.starts_with(authority.path()),"Historical derived path overlaps actual source/target");
  authority.io_fence()?;
  let listener=std::net::TcpListener::bind("127.0.0.1:0")?;listener.set_nonblocking(true)?;let address=listener.local_addr()?;let listener=unsafe{File::from_raw_fd(std::os::fd::IntoRawFd::into_raw_fd(listener))};
  let executable=File::open("/proc/self/exe")?;let md=executable.metadata()?;anyhow::ensure!(md.is_file()&&md.uid()==unsafe{libc::geteuid()}&&md.mode()&0o022==0&&md.mode()&0o111!=0&&md.len()<=1024*1024*1024,"Historical actual executable not owned/bounded");
  let projection_sha256={let mut c=target.reader.pool.acquire().await?;engine_transfer::historical_application_projection_in(&mut c,None).await?};
  let tree=HistoricalFreshInventory::capture(&authority.active)?;
  let wire=HistoricalReadonlyChildWire{version:1,purpose:"historical-readonly-child-v1".into(),nonce:uuid::Uuid::new_v4().to_string(),path:authority.path().to_owned(),chain_stamps:authority.active.root.chain.iter().map(verification_worker_stamp).collect::<io::Result<_>>()?,main_stamp:verification_worker_stamp(authority.held_main())?,inventory_sha256:historical_child_inventory_sha(&tree)?,projection_sha256,executable_stamp:verification_worker_stamp(&executable)?,executable_sha256:hash_file(&executable,md.len())?,derived_path:derived_path.clone(),derived_identity:identity(derived.fd())?,listener_identity:identity(&listener)?,address,human:plan.human,session:plan.session,routes:plan.routes};
  anyhow::ensure!(wire.chain_stamps.len()+5<=32,"Historical child descriptor bound exceeded");let held=Self{authority:authority.clone(),executable,derived,log,listener,wire,sent:false};held.fence()?;source.fence().await?;Ok(held)
 }
 fn fence(&self)->io::Result<()>{self.authority.fence()?;self.derived.check()?;let md=self.derived.fd().metadata()?;if identity(self.derived.fd())?!=self.wire.derived_identity||md.uid()!=unsafe{libc::geteuid()}||md.mode()&0o7777!=0o700||identity(&self.listener)?!=self.wire.listener_identity||verification_worker_stamp(&self.executable)?!=self.wire.executable_stamp||hash_file(&self.executable,self.executable.metadata()?.len())?!=self.wire.executable_sha256{return Err(fail("Historical original handoff changed"));}Ok(())}
 fn send_once(&mut self,socket:&File)->io::Result<()>{self.fence()?;if self.sent{return Err(fail("Historical original handoff already consumed"));}let bytes=serde_json::to_vec(&self.wire).map_err(io::Error::other)?;let mut files:Vec<&File>=self.authority.active.root.chain.iter().collect();files.extend([self.authority.held_main(),&self.authority.lease,&self.executable,self.derived.fd(),&self.listener]);self.sent=true;verification_worker_send(socket,&bytes,&files)?;self.fence()}
}
struct HistoricalReadonlyChildCustody{root:Root,main:Arc<File>,main_stamp:PhysicalStamp,_lease:File,executable:File,parent_executable:File,derived:Root,listener:std::sync::Mutex<Option<File>>,tree:HistoricalFreshInventory,wire:HistoricalReadonlyChildWire,parent:libc::ucred,active:std::sync::OnceLock<Arc<Active>>}
impl HistoricalReadonlyChildCustody {
 fn receive(channel:&File)->anyhow::Result<Arc<Self>>{
  let mut packet=verification_worker_receive(channel)?;
  anyhow::ensure!(packet.credentials.pid==unsafe{libc::getppid()}&&packet.credentials.uid==unsafe{libc::geteuid()}&&packet.credentials.gid==unsafe{libc::getegid()},"Historical child peer is not original parent");
  let wire:HistoricalReadonlyChildWire=serde_json::from_slice(&packet.bytes)?;
  anyhow::ensure!(wire.version==1&&wire.purpose=="historical-readonly-child-v1"&&uuid::Uuid::parse_str(&wire.nonce)?.to_string()==wire.nonce&&wire.chain_stamps.len()>=2&&packet.files.len()==wire.chain_stamps.len()+5&&wire.routes.len()<=19&&historical_selected_id(&wire.session)&&matches!(wire.routes.first(),Some(HistoricalReadonlyRoute::CurrentHuman)),"Historical child purpose/envelope/roles differ");
  let mut paths=std::collections::HashSet::new();for route in &wire.routes{anyhow::ensure!(historical_data_route_valid(route)&&paths.insert(historical_route_path(route)),"Historical typed selected routes invalid or duplicate");}
  historical_control_budget_seconds(&wire.routes)?;
  let listener=packet.files.pop().ok_or_else(||fail("Historical listener descriptor absent"))?;let derived_fd=packet.files.pop().ok_or_else(||fail("Historical derived descriptor absent"))?;let executable=packet.files.pop().ok_or_else(||fail("Historical executable descriptor absent"))?;let lease=packet.files.pop().ok_or_else(||fail("Historical exclusion descriptor absent"))?;let main=Arc::new(packet.files.pop().ok_or_else(||fail("Historical main descriptor absent"))?);let root=Root{absolute:wire.path.clone(),chain:packet.files};root.check()?;
  for(f,s)in root.chain.iter().zip(&wire.chain_stamps){anyhow::ensure!(verification_worker_stamp(f)?==*s,"Historical received ancestry stamp differs");}
  let main_stamp=physical_stamp(main.as_ref())?;anyhow::ensure!(verification_worker_stamp(main.as_ref())?==wire.main_stamp&&physical_stamp(&lease)?==main_stamp&&qualified_sqlite_path_identity(&root)?==identity(main.as_ref())?,"Historical received main/exclusion differs");
  let own=File::open("/proc/self/exe")?;let parent_executable=File::open(format!("/proc/{}/exe",packet.credentials.pid))?;
  anyhow::ensure!(identity(&own)?==identity(&executable)?&&identity(&parent_executable)?==identity(&executable)?&&verification_worker_stamp(&executable)?==wire.executable_stamp&&verification_worker_stamp(&parent_executable)?==wire.executable_stamp&&hash_file(&executable,executable.metadata()?.len())?==wire.executable_sha256,"Historical child ELF/parent identity differs");
  let derived=Root::open(&wire.derived_path)?;anyhow::ensure!(identity(derived.fd())?==identity(&derived_fd)?&&identity(&derived_fd)?==wire.derived_identity&&derived_fd.metadata()?.mode()&0o7777==0o700,"Historical derived role substituted");
  anyhow::ensure!(identity(&listener)?==wire.listener_identity,"Historical listener descriptor substituted");historical_child_listener(&listener,wire.address)?;
  let tree=HistoricalFreshInventory::capture_original_descriptors(Root::open(&root.absolute)?,main.clone())?;anyhow::ensure!(historical_child_inventory_sha(&tree)?==wire.inventory_sha256,"Historical received full inventory differs");
  let held=Arc::new(Self{root,main,main_stamp,_lease:lease,executable,parent_executable,derived,listener:std::sync::Mutex::new(Some(listener)),tree,wire,parent:packet.credentials,active:Default::default()});held.fence()?;Ok(held)
 }
 // Pager IO maps only already admitted original objects. Full hashes/rows are
 // rechecked at every actual admission, request/final-use and retirement boundary.
 // No successful-hash cache or role/SQL permission is inferred from this check.
 fn io_fence(&self)->io::Result<()>{
  self.root.check()?;self.derived.check()?;for(f,s)in self.root.chain.iter().zip(&self.wire.chain_stamps){if verification_worker_stamp(f)?!=*s{return Err(fail("Historical child ancestor exchanged"));}}
  let derived=self.derived.fd().metadata()?;
  if physical_stamp(self.main.as_ref())?!=self.main_stamp||physical_stamp(&self._lease)?!=self.main_stamp||qualified_sqlite_path_identity(&self.root)?!=identity(self.main.as_ref())?||identity(self.derived.fd())?!=self.wire.derived_identity||derived.uid()!=unsafe{libc::geteuid()}||derived.mode()&0o7777!=0o700||verification_worker_stamp(&self.executable)?!=self.wire.executable_stamp||verification_worker_stamp(&self.parent_executable)?!=self.wire.executable_stamp{return Err(fail("Historical child original native objects changed"));}
  if let Some(active)=self.active.get(){if identity(active.root.fd())?!=identity(self.root.fd())?||identity(active.sqlite_main.as_ref().ok_or_else(||fail("Historical child actual activation main absent"))?.as_ref())?!=identity(self.main.as_ref())?{return Err(fail("Historical child actual activation rebound"));}}Ok(())
 }
 fn fence(&self)->io::Result<()>{
  self.io_fence()?;if hash_file(&self.executable,self.executable.metadata()?.len())?!=self.wire.executable_sha256{return Err(fail("Historical original child executable bytes changed"));}
  self.tree.fence()?;if let Some(active)=self.active.get(){active.check()?;}self.io_fence()
 }
 async fn activate(&self,pool:&sqlx::AnyPool)->anyhow::Result<()>{
  let clock=std::time::Instant::now();self.fence()?;eprintln!("HISTORICAL_READONLY_ACTIVATION_PHASE before-real-activate elapsed_ms={}",clock.elapsed().as_millis());
  activate_in(&self.root.absolute,pool).await.map_err(|e|anyhow::anyhow!("Actual historical child activation refused:{e:?}"))?;eprintln!("HISTORICAL_READONLY_ACTIVATION_PHASE real-activate-returned elapsed_ms={}",clock.elapsed().as_millis());
  let(_,active)=registered(&self.root.absolute)?;eprintln!("HISTORICAL_READONLY_ACTIVATION_PHASE actual-registry-selected elapsed_ms={}",clock.elapsed().as_millis());let active=active.ok_or_else(||fail("Historical actual child activation absent"))?;anyhow::ensure!(active.version2&&active.ledger.version==5&&active.engine.is_none(),"Historical child activated foreign purpose");self.active.set(active).map_err(|_|fail("Historical child activation repeated"))?;
  let mut c=pool.acquire().await?;eprintln!("HISTORICAL_READONLY_ACTIVATION_PHASE before-full81 elapsed_ms={}",clock.elapsed().as_millis());
  anyhow::ensure!(engine_transfer::historical_application_projection_in(&mut c,None).await?==self.wire.projection_sha256,"Historical child complete current81 differs");eprintln!("HISTORICAL_READONLY_ACTIVATION_PHASE full81-returned elapsed_ms={}",clock.elapsed().as_millis());self.fence()?;Ok(())
 }
}
fn historical_child_listener(fd:&File,address:std::net::SocketAddr)->io::Result<()>{
 let mut ty=0i32;let mut size=std::mem::size_of_val(&ty)as libc::socklen_t;if unsafe{libc::getsockopt(fd.as_raw_fd(),libc::SOL_SOCKET,libc::SO_TYPE,(&mut ty as *mut i32).cast(),&mut size)}<0||ty!=libc::SOCK_STREAM{return Err(fail("Historical listener socket role differs"));}
 let mut listening=0i32;let mut listening_size=std::mem::size_of_val(&listening)as libc::socklen_t;if unsafe{libc::getsockopt(fd.as_raw_fd(),libc::SOL_SOCKET,libc::SO_ACCEPTCONN,(&mut listening as *mut i32).cast(),&mut listening_size)}<0||listening!=1{return Err(fail("Historical inherited socket is not listening"));}
 let mut native:libc::sockaddr_in=unsafe{std::mem::zeroed()};let mut len=std::mem::size_of_val(&native)as libc::socklen_t;if unsafe{libc::getsockname(fd.as_raw_fd(),(&mut native as *mut libc::sockaddr_in).cast(),&mut len)}<0{return Err(io::Error::last_os_error());}let actual=std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::from(native.sin_addr.s_addr.to_ne_bytes())),u16::from_be(native.sin_port));if native.sin_family!=libc::AF_INET as u16||actual!=address||!actual.ip().is_loopback(){return Err(fail("Historical inherited listener address differs"));}Ok(())
}
#[derive(Clone)]struct HistoricalReadonlyHttpBoundary{custody:Arc<HistoricalReadonlyChildCustody>,state:Arc<HistoricalReadonlyReaderState>,pool:sqlx::AnyPool,owner:Arc<HistoricalHttpUsageOwner>}
impl HistoricalReadonlyHttpBoundary{
 async fn fence(&self)->anyhow::Result<()>{
  anyhow::ensure!(matches!(&self.state.authority,HistoricalReadonlyReaderAuthority::Child(original)if Arc::ptr_eq(original,&self.custody)),"Historical request custody and actual reader differ");
  self.state.check()?;let mut c=self.pool.acquire().await?;
  anyhow::ensure!(engine_transfer::historical_application_projection_in(&mut c,None).await?==self.custody.wire.projection_sha256,"Historical registered request complete current81 changed");
  self.state.check()?;Ok(())
 }
}
async fn historical_readonly_http_boundary(axum::extract::State(scope):axum::extract::State<HistoricalReadonlyHttpBoundary>,mut request:axum::extract::Request,next:axum::middleware::Next)->axum::response::Response{
 use axum::response::IntoResponse;
 let allowed=!request.headers().contains_key("upgrade")&&!request.headers().contains_key("x-api-key")&&scope.custody.wire.routes.iter().any(|r|match r{HistoricalReadonlyRoute::SearchReindex{..}=>request.method()==axum::http::Method::POST&&request.uri().path_and_query().is_some_and(|v|v.as_str()==historical_route_path(r)),HistoricalReadonlyRoute::SearchPublished{..}=>request.method()==axum::http::Method::GET&&request.uri().path_and_query().is_some_and(|v|v.as_str()==historical_route_path(r)),_=>request.method()==axum::http::Method::GET&&request.uri().query().is_none()&&request.uri().path()==historical_route_path(r)});
 if !allowed{return (axum::http::StatusCode::SERVICE_UNAVAILABLE,axum::Json(serde_json::json!({"code":"historical_readonly_unavailable"}))).into_response();}
 let mut request=if request.method()==axum::http::Method::POST{let(parts,body)=request.into_parts();match tokio::time::timeout(std::time::Duration::from_secs(2),axum::body::to_bytes(body,1)).await{Ok(Ok(bytes))if bytes.is_empty()=>axum::http::Request::from_parts(parts,axum::body::Body::empty()),_=>return (axum::http::StatusCode::SERVICE_UNAVAILABLE,axum::Json(serde_json::json!({"code":"historical_readonly_unavailable"}))).into_response()}}else{request};
 let health=request.uri().path()=="/api/v1/admin/storage-health";
 let lease=match scope.owner.enter_request(){Ok(lease)=>Arc::new(lease),Err(_)=>return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response()};
 #[cfg(test)] if health { eprintln!("HISTORICAL_TEST_HEALTH_ACTUAL_ENTRY_LEASE_HELD"); }
 #[cfg(test)] if health && request.headers().get("x-historical-test-finish-overlap").is_some_and(|v|v=="finish-v1") {
  eprintln!("HISTORICAL_TEST_HEALTH_FINISH_WAIT_LEASE_HELD");let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
  while !scope.owner.finish_validated.load(std::sync::atomic::Ordering::Acquire){if std::time::Instant::now()>=deadline{return axum::http::StatusCode::GATEWAY_TIMEOUT.into_response();}tokio::time::sleep(std::time::Duration::from_millis(2)).await;}
  eprintln!("HISTORICAL_TEST_HEALTH_FINISH_VALIDATED_RELEASE_LEASE");
 }
 if scope.fence().await.is_err(){return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();}
 if health{request.extensions_mut().insert(HistoricalHealthRequest{lease:lease.clone()});}
 let mut response=next.run(request).await;
 if scope.fence().await.is_err(){return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();}
 if health&&response.status().is_success(){
  let Some(finalizer)=response.extensions_mut().remove::<Arc<HistoricalHealthFinalizer>>()else{return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();};
  if let Err(error)=finalizer.finish(&lease).await{return error.into_response();}
 }
 response
}
#[derive(serde::Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]struct HistoricalReadonlyChildAck{version:u32,nonce:String,phase:String,pid:i32,uid:u32,executable_sha256:String,projection_sha256:String,listener_identity:Identity}
pub(crate) async fn run_historical_readonly_worker(fd:i32)->anyhow::Result<()>{
 use std::os::fd::{FromRawFd,IntoRawFd};anyhow::ensure!(fd>=3,"Historical standard descriptor refused");let channel=unsafe{File::from_raw_fd(fd)};let boot=std::time::Instant::now();let custody=HistoricalReadonlyChildCustody::receive(&channel)?;eprintln!("HISTORICAL_READONLY_CHILD_PHASE original-kernel-received elapsed_ms={}",boot.elapsed().as_millis());std::env::set_var("DATA_DIR",&custody.root.absolute);sqlx::any::install_default_drivers();
 let reader=HistoricalReadonlySealedReader::enter_vfs(HistoricalReadonlyReaderVfs::from_authority(HistoricalReadonlyReaderAuthority::Child(custody.clone()))?).await?;eprintln!("HISTORICAL_READONLY_CHILD_PHASE readonly-vfs-open elapsed_ms={}",boot.elapsed().as_millis());custody.activate(&reader.pool).await?;reader.fence()?;eprintln!("HISTORICAL_READONLY_CHILD_PHASE actual-activation-current81 elapsed_ms={}",boot.elapsed().as_millis());
 let state=reader.vfs.as_ref().ok_or_else(||fail("Historical actual child VFS absent"))?.state().clone();let mut c=reader.pool.acquire().await?;let mut tx={use sqlx::Connection;c.begin_with("BEGIN IMMEDIATE").await?};let physical=HistoricalFinalizedObservation::hold_readonly_in(state.clone(),&mut tx).await?;physical.recheck_in(&mut tx).await?;
 let denied=sqlx::query("UPDATE documents SET title=title||'-readonly-refused'").execute(&mut *tx).await.err().ok_or_else(||fail("Historical child admitted SQL write"))?;anyhow::ensure!(denied.as_database_error().and_then(|e|e.code()).as_deref()==Some("8"),"Historical child write failed with wrong error");tx.rollback().await?;drop(c);eprintln!("HISTORICAL_READONLY_CHILD_PHASE physical-original-begin-readonly8 elapsed_ms={}",boot.elapsed().as_millis());
 let jwt_secret=std::env::var("JWT_SECRET").map_err(|_|fail("Historical actual configured secret absent"))?;anyhow::ensure!(jwt_secret.len()>=32,"Historical configured secret too short");let search_index=crate::search::SearchIndex::new(custody.derived.absolute.join("readonly-app-index"))?;
 let owner=HistoricalHttpUsageOwner::from_held(physical,&reader,custody.clone())?;let registration=owner.register()?;
 let raw=custody.listener.lock().map_err(|_|fail("Historical listener mutex poisoned"))?.take().ok_or_else(||fail("Historical listener already consumed"))?;historical_child_listener(&raw,custody.wire.address)?;let std_listener=unsafe{std::net::TcpListener::from_raw_fd(raw.into_raw_fd())};std_listener.set_nonblocking(true)?;let listener=tokio::net::TcpListener::from_std(std_listener)?;
 let base_url=format!("http://{}",custody.wire.address);let app=crate::registered_application(crate::AppState{db:reader.pool.clone(),jwt_secret,base_url:base_url.clone(),search_index,oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()},&base_url)?.layer(axum::middleware::from_fn_with_state(HistoricalReadonlyHttpBoundary{custody:custody.clone(),state:state.clone(),pool:reader.pool.clone(),owner:owner.clone()},historical_readonly_http_boundary));
 let ack=|phase:&str|HistoricalReadonlyChildAck{version:1,nonce:custody.wire.nonce.clone(),phase:phase.into(),pid:unsafe{libc::getpid()},uid:unsafe{libc::geteuid()},executable_sha256:custody.wire.executable_sha256.clone(),projection_sha256:custody.wire.projection_sha256.clone(),listener_identity:custody.wire.listener_identity};
 eprintln!("HISTORICAL_READONLY_CHILD_PHASE registered-router-ready elapsed_ms={}",boot.elapsed().as_millis());verification_worker_send(&channel,&serde_json::to_vec(&ack("registered-readonly-ready"))?,&[])?;
 let control_budget=std::time::Duration::from_secs(historical_control_budget_seconds(&custody.wire.routes)?); let control=channel.try_clone()?;let expected=custody.parent;let nonce=custody.wire.nonce.clone();let expected_elf=custody.wire.executable_sha256.clone();let expected_projection=custody.wire.projection_sha256.clone();let expected_listener=custody.wire.listener_identity;let control_custody=custody.clone();let shutdown_owner=owner.clone();let(out_tx,out_rx)=tokio::sync::oneshot::channel();
 let shutdown=async move{let result=tokio::task::spawn_blocking(move||->anyhow::Result<()>{let deadline=std::time::Instant::now()+control_budget;
  let packet=loop{
   anyhow::ensure!(std::time::Instant::now()<deadline,"Historical original control deadline exceeded");
   match verification_worker_receive(&control){
    Ok(packet)=>{anyhow::ensure!(std::time::Instant::now()<deadline,"Historical original control deadline exceeded");break packet;},
    Err(error)if matches!(error.kind(),io::ErrorKind::WouldBlock|io::ErrorKind::TimedOut)=>{
     anyhow::ensure!(std::time::Instant::now()<deadline&&unsafe{libc::getppid()}==expected.pid,"Historical original parent unavailable or control deadline exceeded");
     let parent=File::open(format!("/proc/{}/exe",expected.pid))?;
     anyhow::ensure!(identity(&parent)?==identity(&control_custody.parent_executable)?&&verification_worker_stamp(&parent)?==control_custody.wire.executable_stamp,"Historical original parent executable changed during control wait");
     control_custody.fence()?;
     anyhow::ensure!(std::time::Instant::now()<deadline,"Historical original control deadline exceeded");
    },
    Err(error)=>return Err(error.into()),
   }
  };anyhow::ensure!(packet.credentials.pid==expected.pid&&packet.credentials.uid==expected.uid&&packet.credentials.gid==expected.gid&&packet.files.is_empty(),"Historical shutdown is not original parent");let message:HistoricalReadonlyChildAck=serde_json::from_slice(&packet.bytes)?;anyhow::ensure!(message.version==1&&message.nonce==nonce&&message.phase=="finish-registered-readonly"&&message.pid==expected.pid&&message.uid==expected.uid&&message.executable_sha256==expected_elf&&message.projection_sha256==expected_projection&&message.listener_identity==expected_listener,"Historical shutdown purpose/nonce differs");Ok(())}).await.map_err(|e|e.to_string()).and_then(|r|r.map_err(|e|e.to_string()));#[cfg(test)] if result.is_ok(){shutdown_owner.finish_validated.store(true,std::sync::atomic::Ordering::Release);}
 let stopped=shutdown_owner.stop().map_err(|_|"Historical admission close failed".to_owned());drop(shutdown_owner);let _=out_tx.send(result.and(stopped));};
 axum::serve(listener,app.into_make_service_with_connect_info::<std::net::SocketAddr>()).with_graceful_shutdown(shutdown).await?;out_rx.await.map_err(|_|fail("Historical shutdown outcome missing"))?.map_err(|e|fail(&e))?;
 let closing=registration.stop_and_unregister()?;let physical=owner.drain_and_recover(closing).await?;
 reader.fence()?;{let mut c=reader.pool.acquire().await?;physical.recheck_in(&mut c).await?;anyhow::ensure!(engine_transfer::historical_application_projection_in(&mut c,None).await?==custody.wire.projection_sha256,"Historical child final81 changed");}reader.close().await?;state.native_fence()?;physical.fence_physical()?;custody.fence()?;
 verification_worker_send(&channel,&serde_json::to_vec(&ack("registered-readonly-closed"))?,&[])?;Ok(())
}
