use sqlx::AnyPool;

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

pub async fn init_pool() -> anyhow::Result<AnyPool> {
    sqlx::any::install_default_drivers();

    let database_url = database_url()?;

    let pool = AnyPool::connect(&database_url).await?;

    tracing::info!(
        "Database connected: {}",
        if database_url.starts_with("postgres") {
            "PostgreSQL"
        } else {
            "SQLite"
        }
    );

    Ok(pool)
}
