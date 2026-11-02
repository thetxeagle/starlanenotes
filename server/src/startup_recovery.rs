// Shared normal-startup recovery order. Verification observes the actual owners;
// it cannot fabricate an acknowledgment or convert copied metadata into custody.
#[cfg(target_os="linux")]
pub(crate) async fn recover_initialized<'a,'b>(
 pool:&sqlx::AnyPool,
 selected:Option<&'a crate::storage::restoration::linux::OriginalTargetRecoverySelection<'b>>,
)->anyhow::Result<Option<crate::storage::publication::PublicationRecoveryAck<'a,'b>>> {
 if let Some(selected)=selected {
  selected.fence()?;
  anyhow::ensure!(std::sync::Arc::ptr_eq(&pool.connect_options(),&selected.pool().connect_options()),"Normal startup observer requires original admitted pool");
 }
 macro_rules! fence {()=>{if let Some(selected)=selected {selected.fence()?;}}}
 fence!();crate::storage::relocation::recover(pool).await.map_err(|e|anyhow::anyhow!("Move relocation recovery: {e:?}"))?;fence!();
 crate::storage::journal::recover(pool).await.map_err(|e|anyhow::anyhow!("Storage journal recovery: {e:?}"))?;fence!();
 crate::storage::canonical_rewrite::recover(pool).await.map_err(|e|anyhow::anyhow!("Canonical revision recovery: {e:?}"))?;fence!();
 let publication=if let Some(selected)=selected {Some(crate::storage::publication::recover_for_verification(selected).await.map_err(|e|anyhow::anyhow!("Publication journal recovery: {e:?}"))?)}else{crate::storage::publication::recover(pool).await.map_err(|e|anyhow::anyhow!("Publication journal recovery: {e:?}"))?;None};fence!();
 crate::storage::attachment_upload::recover_in(&crate::storage::data_dir(),pool).await.map_err(|e|anyhow::anyhow!("Attachment upload recovery: {e}"))?;fence!();
 let pending:i64=sqlx::query_scalar("SELECT count(*) FROM storage_journal_receipts WHERE state='pending'").fetch_one(pool).await?;fence!();
 let unresolved=crate::storage::publication::has_unresolved_operations(pool).await.map_err(|e|anyhow::anyhow!("Publication evidence inspection: {e:?}"))?;
 fence!();
 if pending==0 && !unresolved {let renamed=crate::storage::migrate_document_filenames(pool).await?;fence!();if renamed>0 {tracing::info!(renamed_documents=renamed,"Renamed legacy document files to readable Markdown paths");}}
 else {tracing::warn!(pending_storage=pending,pending_publication=unresolved,"Legacy filename migration deferred while storage intents need review");}
 fence!();Ok(publication)
}
#[cfg(not(target_os="linux"))]
pub(crate) async fn recover_initialized(pool:&sqlx::AnyPool)->anyhow::Result<()> {
 crate::storage::relocation::recover(pool).await.map_err(|e|anyhow::anyhow!("Move relocation recovery: {e:?}"))?;
 crate::storage::journal::recover(pool).await.map_err(|e|anyhow::anyhow!("Storage journal recovery: {e:?}"))?;
 crate::storage::canonical_rewrite::recover(pool).await.map_err(|e|anyhow::anyhow!("Canonical revision recovery: {e:?}"))?;
 crate::storage::publication::recover(pool).await.map_err(|e|anyhow::anyhow!("Publication journal recovery: {e:?}"))?;
 let pending:i64=sqlx::query_scalar("SELECT count(*) FROM storage_journal_receipts WHERE state='pending'").fetch_one(pool).await?;
 let unresolved=crate::storage::publication::has_unresolved_operations(pool).await.map_err(|e|anyhow::anyhow!("Publication evidence inspection: {e:?}"))?;
 if pending==0 && !unresolved {let renamed=crate::storage::migrate_document_filenames(pool).await?;if renamed>0 {tracing::info!(renamed_documents=renamed,"Renamed legacy document files to readable Markdown paths");}}
 else {tracing::warn!(pending_storage=pending,pending_publication=unresolved,"Legacy filename migration deferred while storage intents need review");}Ok(())
}
