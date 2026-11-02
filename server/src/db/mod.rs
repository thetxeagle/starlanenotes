mod migration_lineage;
use sqlx::AnyPool;

mod engine;
pub use engine::{DatabaseEngine, migrate_selected, selected_migrator};

use crate::storage;

pub fn database_url() -> anyhow::Result<String> {
    if sqlite_enabled()? {
        if let Ok(url) = std::env::var("DATABASE_URL") {
            if !url.trim().is_empty() {
                tracing::warn!(
                    "Ignoring DATABASE_URL because SQLITE_ENABLED=true. Set SQLITE_ENABLED=false to use PostgreSQL."
                );
            }
        }
        return Ok(sqlite_database_url());
    }

    postgres_database_url()
}

fn sqlite_database_url() -> String {
    let db_path = storage::data_dir().join("starlane-notes.db");
    format!("sqlite:{}?mode=rwc", db_path.display())
}

fn sqlite_enabled() -> anyhow::Result<bool> {
    match std::env::var("SQLITE_ENABLED") {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "" | "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => anyhow::bail!(
                "SQLITE_ENABLED must be true or false. Use SQLITE_ENABLED=true for local SQLite or SQLITE_ENABLED=false for PostgreSQL."
            ),
        },
        Err(_) => Ok(true),
    }
}

fn postgres_database_url() -> anyhow::Result<String> {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        let url = url.trim().to_string();
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            return Ok(url);
        }
        if !url.is_empty() {
            anyhow::bail!(
                "DATABASE_URL must start with postgres:// or postgresql:// when SQLITE_ENABLED=false."
            );
        }
    }

    let host = required_env("POSTGRES_HOST")?;
    let port = std::env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());
    port.parse::<u16>()
        .map_err(|_| anyhow::anyhow!("POSTGRES_PORT must be a valid TCP port."))?;
    let user = required_env("POSTGRES_USER")?;
    let password = required_env("POSTGRES_PASSWORD")?;
    let database = required_env("POSTGRES_DB")?;
    let sslmode = std::env::var("POSTGRES_SSLMODE").unwrap_or_else(|_| "disable".to_string());

    Ok(format!(
        "postgres://{}:{}@{}:{}/{}?sslmode={}",
        urlencoding::encode(&user),
        urlencoding::encode(&password),
        host,
        port,
        urlencoding::encode(&database),
        urlencoding::encode(&sslmode)
    ))
}

fn required_env(name: &str) -> anyhow::Result<String> {
    let value = std::env::var(name).unwrap_or_default();
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("{name} is required when SQLITE_ENABLED=false.");
    }
    Ok(value.to_string())
}

pub async fn init_pool() -> anyhow::Result<(AnyPool, DatabaseEngine)> {
    sqlx::any::install_default_drivers();

    let database_url = database_url()?;
    let engine = DatabaseEngine::from_url(&database_url)?;

    #[cfg(target_os = "linux")]
    let sqlite_file = if engine == DatabaseEngine::Sqlite {
        storage::private_space_layout::prepare_sqlite(&storage::data_dir())?
    } else { None };
    let pool = AnyPool::connect(&database_url).await?;
    #[cfg(target_os = "linux")]
    if let Some(file) = sqlite_file { file.fence()?; }

    #[cfg(target_os = "linux")]
    if engine == DatabaseEngine::Sqlite {
        storage::recovery_records::retain_pool_source(&pool, &storage::data_dir());
    }

    tracing::info!(
        "Database connected: {}",
        if database_url.starts_with("postgres") {
            "PostgreSQL"
        } else {
            "SQLite"
        }
    );

    Ok((pool, engine))
}

// These cases change process-wide server configuration. Run each ignored case
// alone in its own process against a newly created disposable DATA_DIR.
#[cfg(all(test, target_os = "linux"))]
mod sqlite_runtime_settings_acceptance {
    use super::*;
    use anyhow::ensure;

    async fn inspect(pool: &AnyPool, journal: &str, phase: &str) -> anyhow::Result<()> {
        // Simultaneous leases prove these are two initialized native connections.
        let mut first = pool.acquire().await?;
        let mut second = pool.acquire().await?;
        for (number, connection) in [(1, &mut first), (2, &mut second)] {
            let mut observed = serde_json::Map::new();
            for (name, expected) in [("foreign_keys", 1_i64), ("busy_timeout", 5000), ("synchronous", 2)] {
                let value: i64 = sqlx::query_scalar(&format!("PRAGMA {name}")).fetch_one(&mut **connection).await?;
                observed.insert(name.into(), value.into());
                ensure!(value == expected, "Actual initialized SQLite {name} differs: {value}");
            }
            for (name, expected) in [("journal_mode", journal), ("locking_mode", "normal")] {
                let value: String = sqlx::query_scalar(&format!("PRAGMA {name}")).fetch_one(&mut **connection).await?;
                observed.insert(name.into(), value.clone().into());
                ensure!(value == expected, "Actual initialized SQLite {name} differs: {value}");
            }
            println!("ACTUAL_SQLITE_INITIALIZER_SETTINGS {}", serde_json::json!({"phase": phase, "connection": number, "settings": observed}));
        }
        sqlx::query("CREATE TABLE IF NOT EXISTS runtime_settings_parent(id INTEGER PRIMARY KEY)").execute(&mut *first).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS runtime_settings_child(id INTEGER PRIMARY KEY,parent_id INTEGER NOT NULL REFERENCES runtime_settings_parent(id))").execute(&mut *first).await?;
        sqlx::query("INSERT INTO runtime_settings_parent(id) VALUES(1) ON CONFLICT(id) DO NOTHING").execute(&mut *first).await?;
        sqlx::query("INSERT INTO runtime_settings_child(id,parent_id) VALUES(1,1) ON CONFLICT(id) DO NOTHING").execute(&mut *second).await?;
        let rejected = sqlx::query("INSERT INTO runtime_settings_child(id,parent_id) VALUES(2,999)").execute(&mut *second).await;
        ensure!(rejected.as_ref().err().and_then(sqlx::Error::as_database_error).is_some_and(|e| e.is_foreign_key_violation()), "Actual initializer did not enforce the foreign-key constraint");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runtime_settings_child").fetch_one(&mut *first).await?;
        ensure!(count == 1, "Rejected actual foreign-key write changed retained rows");
        Ok(())
    }

    async fn actual_initializer(existing_wal: bool) -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!("sqlite-real-init-{}", uuid::Uuid::new_v4()));
        std::env::set_var("DATA_DIR", &root);
        std::env::set_var("SQLITE_ENABLED", "true");
        storage::init_data_dir().await?;
        ensure!(storage::private_space_layout::is_designated(&root)?, "Actual new server storage designation absent");
        if existing_wal {
            let (pool, engine) = init_pool().await?;
            ensure!(engine == DatabaseEngine::Sqlite, "Owned SQLite setup selected another engine");
            let mode: String = sqlx::query_scalar("PRAGMA journal_mode=WAL").fetch_one(&pool).await?;
            ensure!(mode == "wal", "Owned existing-WAL setup failed");
            pool.close().await;
        }
        let journal = if existing_wal { "wal" } else { "delete" };
        for phase in ["initial", "reopened"] {
            let (pool, engine) = init_pool().await?;
            ensure!(engine == DatabaseEngine::Sqlite, "Actual initializer selected another engine");
            inspect(&pool, journal, phase).await?;
            pool.close().await;
        }
        println!("ACTUAL_SQLITE_INITIALIZER_RESTART_FK_PASS existing_wal={existing_wal}");
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires an isolated process: actual server initializer uses process DATA_DIR"]
    async fn actual_fresh_sqlite_settings_and_reopen() -> anyhow::Result<()> {
        actual_initializer(false).await
    }

    #[tokio::test]
    #[ignore = "requires an isolated process: preserve an explicitly owned existing WAL database"]
    async fn actual_existing_wal_sqlite_settings_and_reopen() -> anyhow::Result<()> {
        actual_initializer(true).await
    }
}
