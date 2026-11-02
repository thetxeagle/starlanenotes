//! Ordered startup recovery and conservative legacy normalization fence.
use crate::auth::routes::AppError;
use sqlx::AnyPool;
pub async fn recover_and_normalize(pool: &AnyPool) -> Result<usize, AppError> {
    super::restoration::activate_in(&super::data_dir(), pool).await?;
    super::relocation::recover(pool).await?;
    super::journal::recover(pool).await?;
    super::canonical_rewrite::recover(pool).await?;
    super::publication::recover(pool).await?;
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM storage_journal_receipts WHERE state='pending'")
        .fetch_one(pool).await.map_err(AppError::internal)?;
    if pending != 0 || super::publication::has_unresolved_operations(pool).await? {
        tracing::warn!(pending, "Legacy filename normalization deferred while storage evidence needs review");
        return Ok(0);
    }
    super::migrate_document_filenames(pool).await.map_err(AppError::internal)
}
