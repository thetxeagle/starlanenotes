use sqlx::AnyPool;

/// Schema lineage is selected explicitly; SQLx Any is not a SQL dialect adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatabaseEngine {
    Sqlite,
    Postgres,
}

impl DatabaseEngine {
    pub fn from_url(url: &str) -> anyhow::Result<Self> {
        if url.starts_with("sqlite:") {
            Ok(Self::Sqlite)
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Ok(Self::Postgres)
        } else {
            anyhow::bail!("Unsupported database engine")
        }
    }
}

static SQLITE_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
static POSTGRES_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations-postgres");

pub fn selected_migrator(engine: DatabaseEngine) -> &'static sqlx::migrate::Migrator {
    match engine {
        DatabaseEngine::Sqlite => &SQLITE_MIGRATOR,
        DatabaseEngine::Postgres => &POSTGRES_MIGRATOR,
    }
}

pub async fn migrate_selected(pool: &AnyPool, engine: DatabaseEngine) -> anyhow::Result<()> {
    super::migration_lineage::preflight(pool, engine).await?;
    selected_migrator(engine).run(pool).await?;
    if engine == DatabaseEngine::Postgres {
        verify_postgres_table_presence(pool).await?;
    }
    Ok(())
}

/// Limited schema-presence fence, not a column/constraint equivalence certificate.
/// SQLx validates applied checksums but does not rerun applied DDL or repair tables.
const EXPECTED_POSTGRES_TABLES: &[&str] = &[
    "api_keys",
    "asset_delete_replay_receipts",
    "asset_replay_receipts",
    "attachments",
    "attachment_audience_origins",
    "attachment_audience_bindings",
    "attachment_upload_receipts",
    "attachment_families",
    "attachment_version_records",
    "attachment_version_changes",
    "attachment_retirements",
    "audience_mutation_fence",
    "audit_log",
    "bookmark_cache",
    "canonical_revision_receipts",
    "comments",
    "doc_subscriptions",
    "doc_tags",
    "doc_versions",
    "document_access_allowlist",
    "document_access_policies",
    "document_create_replay_receipts",
    "document_delete_replay_receipts",
    "document_draft_access",
    "document_drafts",
    "draft_discussions",
    "draft_discussion_participants",
    "draft_discussion_comments",
    "draft_discussion_subscriptions",
    "document_identifier_receipts",
    "document_maintenance_receipts",
    "document_metadata_replay_receipts",
    "document_viewer_grants",
    "document_workflow_events",
    "document_workflows",
    "documents",
    "draft_birth_receipts",
    "draft_transfer_receipts",
    "draft_save_replay_receipts",
    "excalidraw_files",
    "folders",
    "global_roles",
    "group_global_roles",
    "group_members",
    "groups",
    "move_asset_path_aliases",
    "move_commit_receipts",
    "move_metadata_escrow",
    "move_previews",
    "native_auth_handoffs",
    "notification_prefs",
    "notifications",
    "oauth_accounts",
    "passkey_challenges",
    "passkeys",
    "password_reset_tokens",
    "properties_schema",
    "property_values",
    "publication_receipts",
    "draft_publication_receipts",
    "document_publication_snapshots",
    "snapshot_recovery_receipts",
    "reactions",
    "sessions",
    "space_group_permissions",
    "space_members",
    "space_roles",
    "space_user_blocks",
    "spaces",
    "starred_documents",
    "storage_journal_receipts",
    "storage_restore_current",
    "storage_restore_generations",
    "sync_replay_checkpoints",
    "sync_replay_manifests",
    "sync_replay_operations",
    "system_config",
    "tags",
    "template_drafts",
    "template_replay_receipts",
    "template_revisions",
    "templates",
    "users",
    "webhook_deliveries",
    "webhooks",
    "workflow_templates",
];

async fn verify_postgres_table_presence(pool: &AnyPool) -> anyhow::Result<()> {
    let actual: std::collections::BTreeSet<String> = sqlx::query_scalar(
        "SELECT tablename::text FROM pg_catalog.pg_tables WHERE schemaname=current_schema()",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    if EXPECTED_POSTGRES_TABLES
        .iter()
        .any(|name| !actual.contains(*name))
    {
        anyhow::bail!(
            "PostgreSQL application schema is incomplete; migration history is not a repair permit"
        );
    }
    Ok(())
}

#[cfg(test)]
mod audience_schema_tests {
    use super::*;
    #[tokio::test]
    async fn sqlite57_upgrade_seeds_only_declared_legacy_and_enforces_complete_scope() {
        sqlx::any::install_default_drivers();
        let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        let migrations=selected_migrator(DatabaseEngine::Sqlite).iter().take(54).cloned().collect();
        sqlx::migrate::Migrator{migrations:std::borrow::Cow::Owned(migrations),ignore_missing:false,locking:true,no_tx:false}.run(&pool).await.unwrap();
        for q in ["INSERT INTO users(id,email,display_name) VALUES('actor','schema@example.invalid','Fixture')", "INSERT INTO spaces(id,name,slug,created_by) VALUES('space','Fixture','fixture','actor')", "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('page','space','Fixture','spaces/fixture/page.md','actor','actor')", "INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES('live','page','space','a.bin','A.bin','application/octet-stream',3,'spaces/fixture/a.bin','actor')"] {sqlx::query(q).execute(&pool).await.unwrap();}
        for id in ["live","retained"] {
            sqlx::query("INSERT INTO attachment_version_records(version_id,logical_attachment_id,version_number,original_space_id,original_document_id,uploader_id,original_name,mime_type,byte_length,binary_sha256,original_created_at,original_storage_path,original_filename,upload_operation_id,upload_attempt_id,raw_receipt_sha256,intent_sha256,install_proof_sha256,provenance) VALUES($1,$1,1,'original-space','original-page','actor','Original.bin','application/octet-stream',3,$2,'original-time','spaces/original/a.bin','original.bin',$1,$1,'receipt','intent','install','direct_upload_v1')").bind(id).bind("a".repeat(64)).execute(&pool).await.unwrap();
        }
        let original:Vec<(i64,Vec<u8>)>=sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version").fetch_all(&pool).await.unwrap();
        let before:Vec<(String,String,String)>=sqlx::query_as("SELECT version_id,original_space_id,original_document_id FROM attachment_version_records ORDER BY version_id").fetch_all(&pool).await.unwrap();
        migrate_selected(&pool,DatabaseEngine::Sqlite).await.unwrap();
        let after:Vec<(i64,Vec<u8>)>=sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version").fetch_all(&pool).await.unwrap();
        assert_eq!(after.len(),57);assert_eq!(&after[..original.len()],original.as_slice());
        assert_eq!(sqlx::query_as::<_,(String,String,String)>("SELECT version_id,original_space_id,original_document_id FROM attachment_version_records ORDER BY version_id").fetch_all(&pool).await.unwrap(),before);
        let origins:Vec<(String,String,String,String)>=sqlx::query_as("SELECT asset_id,kind,original_space_id,original_document_id FROM attachment_audience_origins ORDER BY asset_id").fetch_all(&pool).await.unwrap();
        assert_eq!(origins,vec![("live".into(),"legacy_page_v1".into(),"space".into(),"page".into()),("retained".into(),"legacy_page_v1".into(),"original-space".into(),"original-page".into())]);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_audience_origins WHERE draft_id IS NOT NULL OR draft_created_at IS NOT NULL OR actor_id IS NOT NULL OR operation_id IS NOT NULL OR scope_json IS NOT NULL OR scope_sha256 IS NOT NULL").fetch_one(&pool).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_audience_bindings").fetch_one(&pool).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name<>'_sqlx_migrations'").fetch_one(&pool).await.unwrap(),91);
        for (kind,masks) in [("legacy_page_v1",1..64),("draft_upload_v2",0..63),("draft_replay_v2",0..63)] {
            for mask in masks {
                let value=|bit|if mask&(1<<bit)!=0{Some("present")}else{None::<&str>};
                assert!(sqlx::query("INSERT INTO attachment_audience_origins VALUES($1,$2,'space','page',$3,$4,$5,$6,$7,$8)").bind(format!("{kind}-{mask}")).bind(kind).bind(value(0)).bind(value(1)).bind(value(2)).bind(value(3)).bind(value(4)).bind(value(5)).execute(&pool).await.is_err(),"Partial scope admitted {kind}/{mask}");
            }
        }
        sqlx::query("INSERT INTO attachment_audience_origins VALUES('private','draft_upload_v2','space','page','draft','stamp','actor','op','{ raw original }','digest')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO attachment_audience_bindings VALUES('private','publication','snapshot','stamp','pub-op','{ raw proof }','digest')").execute(&pool).await.unwrap();
        assert!(sqlx::query("INSERT INTO attachment_audience_bindings VALUES('private','guessed','other','stamp','op','{}','digest')").execute(&pool).await.is_err());
        for table in ["attachment_audience_origins","attachment_audience_bindings"] {assert!(sqlx::query(&format!("PRAGMA foreign_key_list({table})")).fetch_all(&pool).await.unwrap().is_empty());}
        pool.close().await;
    }
}
