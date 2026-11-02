//! Candidate migration boundary only: actual engine module and engine-specific migrators.
#[path = "../src/db/engine.rs"]
mod engine;
#[path = "../src/db/migration_lineage.rs"]
mod migration_lineage;
use engine::DatabaseEngine;
use sqlx::{any::AnyPoolOptions, migrate::Migrator, Row};
use std::borrow::Cow;

async fn owned_pool(label: &str) -> sqlx::AnyPool {
    sqlx::any::install_default_drivers();
    let url = std::env::var("STARLANE_PG_BOUNDARY_URL")
        .expect("explicit owned disposable PG URL required");
    assert_eq!(
        DatabaseEngine::from_url(&url).unwrap(),
        DatabaseEngine::Postgres
    );
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("boundary_{}_{}", label, uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("SET search_path TO {schema}"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}
#[tokio::test]
#[ignore = "requires explicitly owned disposable PostgreSQL fixture"]
async fn postgres_fresh_actual_migrator_and_lossless_any_types() {
    let pool = owned_pool("fresh").await;
    engine::migrate_selected(&pool, DatabaseEngine::Postgres)
        .await
        .unwrap();
    engine::migrate_selected(&pool, DatabaseEngine::Postgres)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, engine::selected_migrator(DatabaseEngine::Postgres).iter().count() as i64);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM global_roles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 5);
    let n: i64 = sqlx::query_scalar("SELECT id FROM audience_mutation_fence")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let row=sqlx::query("SELECT CAST(9223372036854775806 AS BIGINT) AS integer_value, CAST('2020-01-02T03:04:05.123456Z' AS TEXT) AS timestamp_value, CAST('{  \"raw\": [1,2] }' AS TEXT) AS raw_json, decode('00017f80ff','hex') AS bytes_value, CAST(NULL AS TEXT) AS nullable_value").fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<i64, _>("integer_value"), 9223372036854775806);
    assert_eq!(
        row.get::<String, _>("timestamp_value"),
        "2020-01-02T03:04:05.123456Z"
    );
    assert_eq!(row.get::<String, _>("raw_json"), "{  \"raw\": [1,2] }");
    assert_eq!(
        row.get::<Vec<u8>, _>("bytes_value"),
        vec![0, 1, 127, 128, 255]
    );
    assert_eq!(row.get::<Option<String>, _>("nullable_value"), None);
    let bad = sqlx::query(
        "INSERT INTO storage_restore_current(singleton,generation_id) VALUES(2,'missing')",
    )
    .execute(&pool)
    .await;
    assert!(bad.is_err());
    let bad = sqlx::query(
        "INSERT INTO storage_restore_current(singleton,generation_id) VALUES(1,'missing')",
    )
    .execute(&pool)
    .await;
    assert!(bad.is_err());
    serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("raw_json")).unwrap();
    assert!(sqlx::query("SELECT CAST($1 AS TEXT)")
        .bind("prefix\0tail")
        .fetch_one(&pool)
        .await
        .is_err());
    assert!(sqlx::query("SELECT convert_from($1,'UTF8')")
        .bind(vec![255_u8])
        .fetch_one(&pool)
        .await
        .is_err());
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    pool.close().await;
}
#[tokio::test]
#[ignore = "requires explicitly owned disposable PostgreSQL fixture"]
async fn postgres_baseline_upgrade_preserves_rows_and_checksum_refuses() {
    let pool = owned_pool("upgrade").await;
    let final_m = engine::selected_migrator(DatabaseEngine::Postgres);
    let prior = Migrator {
        migrations: Cow::Owned(final_m.iter().filter(|m| m.version == 1).cloned().collect()),
        ..Migrator::DEFAULT
    };
    prior.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name,created_at) VALUES($1,$2,$3,$4)")
        .bind("kept-id")
        .bind("kept@example.invalid")
        .bind("Kept")
        .bind("2020-01-02T03:04:05.123456Z")
        .execute(&pool)
        .await
        .unwrap();
    final_m.run(&pool).await.unwrap();
    let row: (String, String) =
        sqlx::query_as("SELECT id,created_at FROM users WHERE id='kept-id'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        row,
        ("kept-id".into(), "2020-01-02T03:04:05.123456Z".into())
    );
    let original: Vec<u8> =
        sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version=1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(original, final_m.iter().next().unwrap().checksum.as_ref());
    sqlx::query("UPDATE _sqlx_migrations SET checksum=$1 WHERE version=1")
        .bind(vec![0_u8; 48])
        .execute(&pool)
        .await
        .unwrap();
    let error = final_m.run(&pool).await.unwrap_err();
    assert!(matches!(
        error,
        sqlx::migrate::MigrateError::VersionMismatch(1)
    ));
    let row: (String, String) =
        sqlx::query_as("SELECT id,created_at FROM users WHERE id='kept-id'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        row,
        ("kept-id".into(), "2020-01-02T03:04:05.123456Z".into())
    );
    pool.close().await;
}
#[test]
fn engine_selection_rejects_unknown_urls_and_preserves_sqlite_history() {
    assert_eq!(
        DatabaseEngine::from_url("sqlite:x").unwrap(),
        DatabaseEngine::Sqlite
    );
    assert_eq!(
        DatabaseEngine::from_url("postgres://fixture/x").unwrap(),
        DatabaseEngine::Postgres
    );
    assert_eq!(
        DatabaseEngine::from_url("postgresql://fixture/x").unwrap(),
        DatabaseEngine::Postgres
    );
    assert!(DatabaseEngine::from_url("mysql://fixture/x").is_err());
    let sqlite_versions: Vec<i64> = engine::selected_migrator(DatabaseEngine::Sqlite)
        .iter().map(|migration| migration.version).collect();
    let postgres_versions: Vec<i64> = engine::selected_migrator(DatabaseEngine::Postgres)
        .iter().map(|migration| migration.version).collect();
    assert_eq!(&sqlite_versions[..51], &(1..=51).collect::<Vec<_>>());
    assert_eq!(&postgres_versions[..5], &(1..=5).collect::<Vec<_>>());
    assert_eq!(sqlite_versions, (1..=57).collect::<Vec<_>>());
    assert_eq!(postgres_versions, (1..=11).collect::<Vec<_>>());
}

#[tokio::test]
#[ignore = "requires explicitly owned disposable PostgreSQL fixture"]
async fn postgres_timestamp_defaults_preserve_both_sqlite_families_in_alternate_timezone() {
    let pool = owned_pool("time").await;
    engine::migrate_selected(&pool, DatabaseEngine::Postgres)
        .await
        .unwrap();
    sqlx::query("SET TIME ZONE 'Pacific/Auckland'")
        .execute(&pool)
        .await
        .unwrap();
    let before: String = sqlx::query_scalar(
        "SELECT to_char(statement_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) SELECT 'time-'||n::text,'time-'||n::text||'@example.invalid','User '||n::text FROM generate_series(1,2) n CROSS JOIN LATERAL pg_sleep((1.1+n*0)::double precision)").execute(&pool).await.unwrap();
    let after: String = sqlx::query_scalar(
        "SELECT to_char(statement_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT created_at,updated_at FROM users ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], rows[1]);
    assert_eq!(rows[0].0, rows[0].1);
    assert_eq!(rows[0].0.len(), 20);
    chrono::DateTime::parse_from_rfc3339(&rows[0].0).unwrap();
    assert!(before <= rows[0].0 && rows[0].0 <= after);
    sqlx::query(
        "INSERT INTO spaces(id,name,slug,created_by) VALUES('time-space','Time','time','time-1')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let before: String = sqlx::query_scalar(
        "SELECT to_char(statement_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD HH24:MI:SS')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) SELECT 'role-'||n::text,'time-space','Role '||n::text,'[]','time-1' FROM generate_series(1,2) n CROSS JOIN LATERAL pg_sleep((1.1+n*0)::double precision)").execute(&pool).await.unwrap();
    let after: String = sqlx::query_scalar(
        "SELECT to_char(statement_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD HH24:MI:SS')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT created_at,updated_at FROM space_roles ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], rows[1]);
    assert_eq!(rows[0].0, rows[0].1);
    assert_eq!(rows[0].0.len(), 19);
    chrono::NaiveDateTime::parse_from_str(&rows[0].0, "%Y-%m-%d %H:%M:%S").unwrap();
    assert!(before <= rows[0].0 && rows[0].0 <= after);
    sqlx::query("INSERT INTO users(id,email,display_name,created_at,updated_at) VALUES('explicit','explicit@example.invalid','Explicit',$1,$1)").bind("2020-01-02T03:04:05.123456+03:00").execute(&pool).await.unwrap();
    let explicit: (String, String) =
        sqlx::query_as("SELECT created_at,updated_at FROM users WHERE id='explicit'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        explicit,
        (
            "2020-01-02T03:04:05.123456+03:00".into(),
            "2020-01-02T03:04:05.123456+03:00".into()
        )
    );
    pool.close().await;
}
#[tokio::test]
#[ignore = "requires explicitly owned disposable PostgreSQL fixture"]
async fn postgres_missing_table_and_legacy_checksum_refuse_without_repair() {
    let pool = owned_pool("missing").await;
    engine::migrate_selected(&pool, DatabaseEngine::Postgres)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES('guard-user','guard@example.invalid','Guard')").execute(&pool).await.unwrap();
    let rows: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT id,email,display_name,created_at FROM users ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let history: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    sqlx::query("DROP TABLE bookmark_cache")
        .execute(&pool)
        .await
        .unwrap();
    let error = engine::migrate_selected(&pool, DatabaseEngine::Postgres)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("schema is incomplete"));
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_catalog.pg_tables WHERE schemaname=current_schema() AND tablename='bookmark_cache'").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
    let now: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT id,email,display_name,created_at FROM users ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows, now);
    let now: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(history, now);
    let sqlite_hash = engine::selected_migrator(DatabaseEngine::Sqlite)
        .iter()
        .next()
        .unwrap()
        .checksum
        .to_vec();
    sqlx::query("UPDATE _sqlx_migrations SET checksum=$1 WHERE version=1")
        .bind(&sqlite_hash)
        .execute(&pool)
        .await
        .unwrap();
    assert!(engine::migrate_selected(&pool, DatabaseEngine::Postgres)
        .await
        .is_err());
    let now: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT id,email,display_name,created_at FROM users ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows, now);
    let retained: Vec<u8> =
        sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version=1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(retained, sqlite_hash);
    pool.close().await;
}


#[tokio::test]
#[ignore = "requires explicitly owned disposable PostgreSQL fixture"]
async fn postgres_missing_upload_receipts_refuses_without_repair() {
    let pool = owned_pool("missing_upload").await;
    engine::migrate_selected(&pool, DatabaseEngine::Postgres).await.unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name) VALUES('kept-upload-user','kept-upload@example.invalid','Kept upload user')")
        .execute(&pool).await.unwrap();
    let users: Vec<(String,String,String,String)> = sqlx::query_as("SELECT id,email,display_name,created_at FROM users ORDER BY id")
        .fetch_all(&pool).await.unwrap();
    let history: Vec<(i64,Vec<u8>)> = sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
        .fetch_all(&pool).await.unwrap();
    sqlx::query("DROP TABLE attachment_upload_receipts").execute(&pool).await.unwrap();
    let error = engine::migrate_selected(&pool, DatabaseEngine::Postgres).await.unwrap_err();
    assert!(error.to_string().contains("schema is incomplete"));
    let present: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_catalog.pg_tables WHERE schemaname=current_schema() AND tablename='attachment_upload_receipts'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(present,0);
    let after: Vec<(String,String,String,String)> = sqlx::query_as("SELECT id,email,display_name,created_at FROM users ORDER BY id")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(after,users);
    let after: Vec<(i64,Vec<u8>)> = sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(after,history);
    pool.close().await;
}
