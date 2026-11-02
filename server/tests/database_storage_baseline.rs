//! Disposable SQLite baseline for the unchanged server migrations and SQLx Any driver.
//! This does not establish PostgreSQL, route-level, backup, or save durability guarantees.

use std::{borrow::Cow, path::PathBuf};

use sqlx::{any::AnyPoolOptions, migrate::Migrator, AnyConnection, AnyPool, Row};
use uuid::Uuid;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
fn assert_current_catalog() {
    let versions: Vec<i64> = MIGRATOR.iter().map(|migration| migration.version).collect();
    assert_eq!(versions, (1..=57).collect::<Vec<_>>(), "SQLite acceptance must compile the complete current catalog");
    println!("CURRENT_SQLITE_MIGRATION_CATALOG 57");
}

const PAGE_PATH: &str = "spaces/baseline/docs/Preserved page-page.md";
const ASSET_PATH: &str = "spaces/baseline/attachments/page/asset.bin";
const PAGE_BYTES: &[u8] = b"# Preserved page\n\nReadable Markdown.\n";
const ASSET_BYTES: &[u8] = &[0, 1, 127, 128, 255];
const CREATED_AT: &str = "2020-01-02T03:04:05Z";

struct Fixture {
    root: PathBuf,
    url: String,
}

impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("starlanenotes-db-baseline-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let url = format!("sqlite:{}?mode=rwc", root.join("baseline.db").display());
        Self { root, url }
    }

    async fn pool(&self) -> AnyPool {
        sqlx::any::install_default_drivers();
        AnyPoolOptions::new()
            .max_connections(2)
            .connect(&self.url)
            .await
            .unwrap()
    }

    fn write_owned_files(&self) {
        for (path, bytes) in [(PAGE_PATH, PAGE_BYTES), (ASSET_PATH, ASSET_BYTES)] {
            let destination = self.root.join(path);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::write(destination, bytes).unwrap();
        }
    }

    fn assert_owned_files(&self) {
        assert_eq!(
            std::fs::read(self.root.join(PAGE_PATH)).unwrap(),
            PAGE_BYTES
        );
        assert_eq!(
            std::fs::read(self.root.join(ASSET_PATH)).unwrap(),
            ASSET_BYTES
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only the uniquely created fixture directory is removed, including on test failure.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn assert_connection_settings(connection: &mut AnyConnection) {
    for (pragma, expected) in [
        ("PRAGMA foreign_keys", 1_i64),
        ("PRAGMA busy_timeout", 5000),
        ("PRAGMA synchronous", 2),
    ] {
        let actual: i64 = sqlx::query_scalar(pragma)
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        assert_eq!(actual, expected, "{pragma}");
    }
    for (pragma, expected) in [
        ("PRAGMA journal_mode", "delete"),
        ("PRAGMA locking_mode", "normal"),
    ] {
        let actual: String = sqlx::query_scalar(pragma)
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        assert_eq!(actual, expected, "{pragma}");
    }
}

async fn assert_pool_settings(pool: &AnyPool) {
    // Hold both leases simultaneously so this cannot accidentally inspect one connection twice.
    let mut first = pool.acquire().await.unwrap();
    let mut second = pool.acquire().await.unwrap();
    assert_connection_settings(&mut first).await;
    assert_connection_settings(&mut second).await;
}

async fn assert_history(pool: &AnyPool, migrator: &Migrator) {
    let rows = sqlx::query("SELECT version, checksum, CASE WHEN success THEN 1 ELSE 0 END AS succeeded FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool).await.unwrap();
    assert_eq!(rows.len(), migrator.iter().count());
    for (row, migration) in rows.iter().zip(migrator.iter()) {
        assert_eq!(row.get::<i64, _>("version"), migration.version);
        assert_eq!(
            row.get::<Vec<u8>, _>("checksum"),
            migration.checksum.as_ref()
        );
        assert_eq!(row.get::<i64, _>("succeeded"), 1);
    }
}

async fn seed(pool: &AnyPool) {
    let mut transaction = pool.begin().await.unwrap();
    for statement in [
        "INSERT INTO users(id,email,display_name,created_at,updated_at) VALUES ('user','baseline@example.invalid','Baseline','2020-01-02T03:04:05Z','2020-01-02T03:04:05Z')",
        "INSERT INTO spaces(id,name,slug,created_by) VALUES ('space','Baseline','baseline','user')",
        "INSERT INTO space_members(id,space_id,user_id,role) VALUES ('membership','space','user','owner')",
        "INSERT INTO folders(id,space_id,name) VALUES ('folder','space','Folder')",
        "INSERT INTO documents(id,space_id,folder_id,title,content_path,created_by,last_updated_by,created_at,updated_at) VALUES ('page','space','folder','Preserved page','spaces/baseline/docs/Preserved page-page.md','user','user','2020-01-02T03:04:05Z','2020-01-02T03:04:05Z')",
        "INSERT INTO properties_schema(id,space_id,name,property_type) VALUES ('property','space','Status','text')",
        "INSERT INTO property_values(id,document_id,property_id,value) VALUES ('value','page','property','Preserved')",
        "INSERT INTO templates(id,space_id,name,content,created_by) VALUES ('template','space','Baseline','# Template','user')",
    ] {
        sqlx::query(statement).execute(&mut *transaction).await.unwrap();
    }
    sqlx::query("UPDATE documents SET automerge_state = $1 WHERE id = 'page'")
        .bind(ASSET_BYTES)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("INSERT INTO doc_versions(id,document_id,version_number,change_set,created_by) VALUES ('version','page',1,$1,'user')")
        .bind(ASSET_BYTES).execute(&mut *transaction).await.unwrap();
    sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES ('attachment','page','space','asset.bin','original.bin','application/octet-stream',$1,$2,'user')")
        .bind(ASSET_BYTES.len() as i64).bind(ASSET_PATH).execute(&mut *transaction).await.unwrap();
    transaction.commit().await.unwrap();
}

async fn assert_seed(pool: &AnyPool) {
    let page: (String, String, String, String, String, String, Vec<u8>, String, String) = sqlx::query_as(
        "SELECT id,space_id,folder_id,content_path,created_by,last_updated_by,automerge_state,created_at,updated_at FROM documents WHERE id = 'page'",
    ).fetch_one(pool).await.unwrap();
    assert_eq!(
        page,
        (
            "page".into(),
            "space".into(),
            "folder".into(),
            PAGE_PATH.into(),
            "user".into(),
            "user".into(),
            ASSET_BYTES.to_vec(),
            CREATED_AT.into(),
            CREATED_AT.into()
        )
    );
    let attachment: (String, String, String, String, i64, String) = sqlx::query_as(
        "SELECT id,document_id,space_id,storage_path,size_bytes,created_by FROM attachments WHERE id = 'attachment'",
    ).fetch_one(pool).await.unwrap();
    assert_eq!(
        attachment,
        (
            "attachment".into(),
            "page".into(),
            "space".into(),
            ASSET_PATH.into(),
            ASSET_BYTES.len() as i64,
            "user".into()
        )
    );
    let version: (String, Vec<u8>, String) = sqlx::query_as(
        "SELECT document_id,change_set,created_by FROM doc_versions WHERE id = 'version'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        version,
        ("page".into(), ASSET_BYTES.to_vec(), "user".into())
    );
    let property: (String, String, String) = sqlx::query_as(
        "SELECT document_id,property_id,value FROM property_values WHERE id = 'value'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        property,
        ("page".into(), "property".into(), "Preserved".into())
    );
    let membership: (String, String, String) =
        sqlx::query_as("SELECT space_id,user_id,role FROM space_members WHERE id = 'membership'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(membership, ("space".into(), "user".into(), "owner".into()));
    let template: (String, String, String) =
        sqlx::query_as("SELECT space_id,content,created_by FROM templates WHERE id = 'template'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        template,
        ("space".into(), "# Template".into(), "user".into())
    );
    let timestamp: String =
        sqlx::query_scalar("SELECT created_at FROM attachments WHERE id = 'attachment'")
            .fetch_one(pool)
            .await
            .unwrap();
    chrono::DateTime::parse_from_rfc3339(&timestamp).unwrap();
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(pool)
        .await
        .unwrap();
    assert!(
        violations.is_empty(),
        "foreign key violation count: {}",
        violations.len()
    );
}

#[tokio::test]
async fn sqlite_fresh_install_preserves_original_migration_history_and_files() {
    assert_current_catalog();
    let fixture = Fixture::new();
    let pool = fixture.pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    assert_history(&pool, &MIGRATOR).await;
    assert_pool_settings(&pool).await;
    fixture.write_owned_files();
    seed(&pool).await;
    assert_seed(&pool).await;
    // Exercise actual constraint enforcement and transaction rollback on the migrated schema.
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("UPDATE property_values SET value = 'rolled back' WHERE id = 'value'")
        .execute(&mut *transaction)
        .await
        .unwrap();
    assert!(sqlx::query(
        "UPDATE attachments SET document_id = 'missing-page' WHERE id = 'attachment'"
    )
    .execute(&mut *transaction)
    .await
    .is_err());
    transaction.rollback().await.unwrap();
    pool.close().await;
    let reopened = fixture.pool().await;
    MIGRATOR.run(&reopened).await.unwrap();
    assert_history(&reopened, &MIGRATOR).await;
    assert_seed(&reopened).await;
    fixture.assert_owned_files();
    reopened.close().await;
}

#[tokio::test]
async fn sqlite_upgrade_from_21_preserves_identity_relationships_attribution_and_bytes() {
    assert_current_catalog();
    let fixture = Fixture::new();
    let pool = fixture.pool().await;
    // Clone resolved metadata rather than changing any checked-in migration SQL or checksum.
    // SQLx 0.8.6 exposes this metadata for its own migrate! macro.
    let prior = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 21)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    assert!(prior.version_exists(21));
    prior.run(&pool).await.unwrap();
    assert_history(&pool, &prior).await;
    fixture.write_owned_files();
    seed(&pool).await;
    assert_seed(&pool).await;
    pool.close().await;
    let upgraded = fixture.pool().await;
    MIGRATOR.run(&upgraded).await.unwrap();
    assert_history(&upgraded, &MIGRATOR).await;
    assert_pool_settings(&upgraded).await;
    assert_seed(&upgraded).await;
    let published: String =
        sqlx::query_scalar("SELECT published_at FROM documents WHERE id = 'page'")
            .fetch_one(&upgraded)
            .await
            .unwrap();
    assert_eq!(published, CREATED_AT);
    fixture.assert_owned_files();
    upgraded.close().await;
}

#[tokio::test]
async fn sqlite_rejects_changed_applied_checksum_without_changing_owned_content() {
    assert_current_catalog();
    let fixture = Fixture::new();
    let pool = fixture.pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    seed(&pool).await;
    fixture.write_owned_files();
    sqlx::query("UPDATE _sqlx_migrations SET checksum = $1 WHERE version = 1")
        .bind(vec![0_u8; 48])
        .execute(&pool)
        .await
        .unwrap();
    let error = MIGRATOR.run(&pool).await.unwrap_err();
    assert!(
        matches!(error, sqlx::migrate::MigrateError::VersionMismatch(1)),
        "{error}"
    );
    assert_seed(&pool).await;
    fixture.assert_owned_files();
    pool.close().await;
}
