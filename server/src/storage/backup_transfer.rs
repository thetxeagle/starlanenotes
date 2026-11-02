//! Explicit stopped-write SQLite-to-PostgreSQL transfer. No running deployment is switched.
use std::path::{Path,PathBuf};
use serde::Serialize;
#[derive(Serialize)]
pub(crate) struct Report {
    backup_id:String, generation_id:String, destination:PathBuf,
    retained_sql_artifacts:PathBuf, retained_transfer_artifacts:PathBuf,
    fresh_startup_required:bool, application_validation_required:bool,
}
#[cfg(target_os="linux")]
pub(crate) async fn run(bundle_path:&Path,target:&Path,artifacts:&Path,version:&str,url:&str)->anyhow::Result<Report> {
    use anyhow::Context;
    use super::restoration::linux::engine_transfer::*;
    anyhow::ensure!(url.starts_with("postgres://")||url.starts_with("postgresql://"),"Explicit PostgreSQL configuration required");
    // New output roles only, in existing parents. Refuse source/output overlap before writing.
    let source=std::fs::canonicalize(bundle_path)?;
    let new_path=|path:&Path|->anyhow::Result<PathBuf>{
        anyhow::ensure!(path.is_absolute(),"Transfer output must be absolute");
        let name=path.file_name().context("New output name required")?;
        let parent=std::fs::canonicalize(path.parent().context("Output parent required")?)?;
        let resolved=parent.join(name);
        anyhow::ensure!(std::fs::symlink_metadata(&resolved).is_err_and(|e|e.kind()==std::io::ErrorKind::NotFound),"Transfer output already exists or cannot be inspected");
        Ok(resolved)
    };
    let target=new_path(target)?;let artifacts=new_path(artifacts)?;
    for(a,b)in [(&target,&source),(&artifacts,&source),(&target,&artifacts)] {
        anyhow::ensure!(!a.starts_with(b)&&!b.starts_with(a),"Transfer source and output roles overlap");
    }
    let bundle=super::backup_inventory::verify(&source).await.context("Verify transfer source bundle")?;
    anyhow::ensure!(bundle.application_version()==version,"Recorded application version differs");
    let _destination_parent=super::restoration::linux::TransferOutputParent::validate(&bundle,&target)?;
    let artifact_parent=super::restoration::linux::TransferOutputParent::validate(&bundle,&artifacts)?;
    let artifacts=artifact_parent.create_private()?;bundle.fence()?;
    sqlx::any::install_default_drivers();
    let ready=super::backup_restore::prepare_ready(&bundle,&target,true).await?;
    let generation_id=ready.generation_id().to_owned();
    let retained_sql_artifacts=ready.retained_sql_artifacts().to_owned();
    let prepared=prepare_engine_transfer(&bundle,ready)?;
    let imported=commit_engine_import_bound(prepared,url,&artifacts).await?;
    let sealed=seal_engine_import(imported)?;
    let committed=commit_engine_seal(sealed,url).await?;
    let finalized=retire_engine_commit(committed)?;finalized.fence()?;
    // Independent live admission after physical publication. This is not full application verification.
    let serving=open_engine_serving(&target,url).await?;serving.fence().await?;
    serving.pool().close().await;bundle.fence()?;
    let report=Report{backup_id:bundle.manifest.backup_id.clone(),generation_id,destination:target,
        retained_sql_artifacts,retained_transfer_artifacts:artifacts.path().to_owned(),
        fresh_startup_required:true,application_validation_required:true};
    bundle.db.close().await;Ok(report)
}
#[cfg(not(target_os="linux"))]
pub(crate) async fn run(_: &Path,_: &Path,_: &Path,_:&str,_:&str)->anyhow::Result<Report>{
    anyhow::bail!("Qualified PostgreSQL transfer currently requires Linux")
}
#[cfg(all(test,target_os="linux"))]
mod tests {
    #[tokio::test]
    #[ignore="genuine frozen source and disposable fresh destination required"]
    async fn required_transfer_request_retired_child()->anyhow::Result<()> {
        let bundle=std::path::PathBuf::from(std::env::var("STARLANE_TRANSFER_REQUEST_BUNDLE").expect("bundle required"));
        let target=std::path::PathBuf::from(std::env::var("STARLANE_TRANSFER_REQUEST_TARGET").expect("target required"));
        anyhow::ensure!(bundle.is_absolute()&&target.is_absolute(),"Absolute disposable roles required");
        let source=super::super::backup_inventory::verify(&bundle).await?;
        let ready=super::super::backup_restore::prepare_ready(&source,&target,true).await?;
        ready.fence()?;source.fence()?;
        anyhow::ensure!(super::super::restoration::engine_startup_requested(&target).is_err(),"SQLite fallback admitted transfer request");
        println!("TRANSFER_REQUEST_RETIRED_PAUSE");
        use std::io::Write;std::io::stdout().flush()?;
        // Kept live until the owned host fixture sends SIGKILL.
        std::future::pending::<()>().await;Ok(())
    }
}
