// Genuine zero-argument ordinary main; the old positional recovery worker is
// deliberately not reused as this producer's child.
pub(crate) async fn verify_sqlite_ordinary_application(source:&Path,workspace:&Path,version:&str)->anyhow::Result<()>{
    use std::io::Read;use std::os::unix::{fs::OpenOptionsExt,process::CommandExt};
    let _=tracing_subscriber::fmt().with_writer(std::io::stderr).with_max_level(tracing::Level::ERROR).try_init();
    let mut input=String::new();std::io::stdin().take(82*1024+1).read_to_string(&mut input)?;
    let supplied=normal_verification_supplied_input(input)?;
    anyhow::ensure!((1..=8).contains(&supplied.audiences.len()),"Ordinary application verification requires1..8 genuine supplied audience sessions");
    let secret=std::env::var("JWT_SECRET").map_err(|_|fail("Original configured authentication secret absent"))?;
    let credential=VerificationHumanCredential::supplied(&supplied.producer,&secret)?;
    let audiences=supplied.audiences.iter().map(|t|VerificationSuppliedAudience::supplied(t,&secret)).collect::<anyhow::Result<Vec<_>>>()?;
    let token=supplied.producer;
    anyhow::ensure!(source.is_absolute()&&workspace.is_absolute()&&!workspace.starts_with(source)&&!source.starts_with(workspace),"Ordinary source/workspace overlap or nonabsolute path");
    let original_root=Root::open(source)?;
    let owned_workspace=engine_transfer::reserve_sqlite_ordinary_workspace(&original_root,workspace)?;
    let bundle_path=workspace.join("bundle");let target_path=workspace.join("application/target");let derived=workspace.join("derived");
    let stdout=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(derived.join("ordinary.stdout.log"))?;
    let stderr=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(derived.join("ordinary.stderr.log"))?;
    let store=engine_transfer::SqliteOrdinaryHistoryStore::for_producer(&original_root,&owned_workspace,&secret)?;
    let version=version.to_owned();
    let summary=capture_sqlite_for_verification(source,&bundle_path,&version.clone(),true,credential.admin(),move|mut original,bundle|Box::pin(async move{
        original.fence().await?;owned_workspace.check()?;
        let pages=original.application_page_plan().await?;let drafts=original.application_draft_plan().await?;let assets=original.application_asset_plan(&pages).await?;
        let audiences=original.application_audience_plan(&pages,&drafts,audiences).await?;
        let sockets=OrdinarySqliteSockets::capture(&mut original,&pages,&audiences).await?;
        let projection=engine_transfer::VerificationSqlProjection::original(&bundle)?;
        crate::storage::backup_restore::run(bundle.data_path().parent().ok_or_else(||fail("Original bundle parent absent"))?,&target_path,&version).await?;
        original.fence().await?;std::env::set_var("DATA_DIR",&target_path);std::env::set_var("SQLITE_ENABLED","true");
        let(pool,engine)=crate::db::init_pool().await?;anyhow::ensure!(engine==crate::db::DatabaseEngine::Sqlite,"Ordinary selected target engine differs");
        crate::db::migrate_selected(&pool,engine).await?;activate_in(&target_path,&pool).await.map_err(|e|anyhow::anyhow!("Actual ordinary selected target activation refused:{e:?}"))?;
        let selected=OrdinarySqliteSelectedTarget::from_original(OriginalTargetRecoverySelection::bind(&bundle,&target_path,&pool).await?).await?;
        original.fence().await?;selected.fence()?;
        let mut listener=OrdinaryListenerParent::prepare(&selected.active.root)?;
        let(parent,child_channel)=verification_worker_pair()?;listener.queue_once(&parent)?;
        let channel_fd=child_channel.as_raw_fd();let executable_fd=listener.executable.as_raw_fd();
        let mut command=std::process::Command::new(format!("/proc/self/fd/{executable_fd}"));
        // No args: bootstrap, initialization, recovery, real auth/log/search/
        // background setup and registered_application are ordinary main's path.
        command.env(ORDINARY_LISTENER_CHANNEL,channel_fd.to_string()).env_remove(SERVICE_OBSERVATION_CHANNEL)
            .env("DATA_DIR",&target_path).env("SQLITE_ENABLED","true").env("DATABASE_URL",format!("sqlite://{}?mode=rw",target_path.join("starlane-notes.db").display()))
            .env("JWT_SECRET",&secret).env("STARLANE_ENV","production").env("PORT",listener.wire.address.port().to_string()).env("BASE_URL",format!("http://{}",listener.wire.address))
            .stdin(std::process::Stdio::null()).stdout(stdout).stderr(stderr);
        unsafe{command.pre_exec(move||{for fd in [channel_fd,executable_fd]{let flags=libc::fcntl(fd,libc::F_GETFD);if flags<0||libc::fcntl(fd,libc::F_SETFD,flags&!libc::FD_CLOEXEC)<0{return Err(io::Error::last_os_error());}}Ok(())});}
        let started=chrono::Utc::now();original.fence().await?;selected.fence()?;
        let mut child=command.spawn()?;drop(child_channel);
        let observed=tokio::time::timeout(std::time::Duration::from_secs(1800),async{
            let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(90);let mut next_fence=tokio::time::Instant::now();
            loop{
                anyhow::ensure!(child.try_wait()?.is_none(),"Actual zero-argument ordinary child exited before readiness");
                if tokio::time::Instant::now()>=next_fence{original.fence().await?;selected.fence()?;listener.fence()?;next_fence=tokio::time::Instant::now()+std::time::Duration::from_secs(3);}
                let mut poll=libc::pollfd{fd:parent.as_raw_fd(),events:libc::POLLIN,revents:0};let status=unsafe{libc::poll(&mut poll,1,0)};anyhow::ensure!(status>=0,"Ordinary readiness native poll failed");
                if status>0{original.fence().await?;listener.receive_ready_once(&parent,&mut child)?;selected.fence()?;break;}
                anyhow::ensure!(tokio::time::Instant::now()<deadline,"Actual zero-argument ordinary readiness exceeded90s");tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            let address=listener.wire.address;
            let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).no_proxy().redirect(reqwest::redirect::Policy::none()).build()?;
            let(status,me)=ordinary_sqlite_live(&mut original,&pages,&mut child,normal_tcp_json(&client,address,Some(&token),"/auth/me")).await?;
            anyhow::ensure!(status==reqwest::StatusCode::OK&&me["id"]==original.current_human().await?,"Actual ordinary current human differs from original source");
            eprintln!("ORDINARY_SQLITE_PROBE phase=ready elapsed_ms=0");let clock=std::time::Instant::now();
            ordinary_sqlite_verify_normal_tcp_pages(&mut original,&pages,address,&token,&mut child).await?;
            eprintln!("ORDINARY_SQLITE_PROBE phase=pages elapsed_ms={}",clock.elapsed().as_millis());listener.fence()?;selected.fence()?;
            ordinary_sqlite_verify_normal_tcp_drafts(&mut original,&pages,&drafts,address,&token,&mut child).await?;
            eprintln!("ORDINARY_SQLITE_PROBE phase=drafts elapsed_ms={}",clock.elapsed().as_millis());listener.fence()?;
            ordinary_sqlite_verify_normal_tcp_exact_assets(&mut original,&pages,&assets,address,&token,&mut child).await?;
            eprintln!("ORDINARY_SQLITE_PROBE phase=assets elapsed_ms={}",clock.elapsed().as_millis());listener.fence()?;
            ordinary_sqlite_verify_normal_tcp_rebuilt_search(&mut original,&pages,address,&token,&mut child).await?;
            ordinary_sqlite_verify_normal_owner_private_search(&mut original,&pages,&drafts,address,&token,&mut child).await?;
            eprintln!("ORDINARY_SQLITE_PROBE phase=search elapsed_ms={}",clock.elapsed().as_millis());listener.fence()?;
            ordinary_sqlite_verify_normal_tcp_audiences(&mut original,&pages,&drafts,&assets,&audiences,&mut child,address).await?;
            sockets.verify(&mut original,&pages,&drafts,&assets,&audiences,&mut child,address,&token).await?;
            eprintln!("ORDINARY_SQLITE_PROBE phase=audiences-socket elapsed_ms={}",clock.elapsed().as_millis());
            selected.fence()?;listener.fence()?;original.fence().await?;Ok::<_,anyhow::Error>(())
        }).await;
        let observed=observed.unwrap_or_else(|_|Err(anyhow::anyhow!("Actual ordinary application probes exceeded1800s")));
        if let Err(error)=observed{let _=child.kill();let _=child.wait();listener.fence()?;return Err(error);}
        let finished=chrono::Utc::now();let stopped=ordinary_sqlite_stop(&selected,&listener,&mut child)?;drop(listener);
        let mut exercised=std::collections::BTreeSet::from([original.admin.ok_or_else(||fail("Original source human absent"))?.claims.sid.clone()]);
        for sample in &audiences.samples{exercised.insert(sample.credential.auth.claims.sid.clone());}
        let window=OrdinarySqliteActivityWindow{exercised,started,finished};
        let mut target=SqliteFinalObservedTarget::hold(selected,stopped,&mut original,&projection,&window).await?;
        anyhow::ensure!(target.qualified.activity_count()>0,"Actual writable ordinary authentication activity was not observed");
        original.fence().await?;target.fence(&projection).await?;
        let complete=engine_transfer::SqliteCompleteOrdinaryEvidence::after_probes(&mut original,&mut target,&projection,&pages,&drafts,&assets,&audiences).await?;
        let mut target=target.retire(&projection,&mut original).await?;
        let summary=store.publish(complete,&mut original,&mut target,&projection).await?;
        target.fence(&projection).await?;drop(target);original.fence().await?;owned_workspace.check()?;Ok(summary)
    })).await?;
    println!("{}",serde_json::to_string(&summary)?);Ok(())
}
