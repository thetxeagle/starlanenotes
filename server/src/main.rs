#![recursion_limit = "1024"]

use axum::{
    extract::State,
    http::{HeaderValue, Method, StatusCode},
    middleware,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use sqlx::AnyPool;
use std::net::SocketAddr;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod ai;
mod bookmarks;
pub mod audit;
mod auth;
mod db;
pub mod mcp;
mod notifications;
mod openapi;
mod provisioning;
mod scim;
pub mod search;
mod spaces;
mod storage;
mod sync;
pub mod webhooks;

const STARLANE_CORS_ORIGINS_ENV: &str = "STARLANE_CORS_ORIGINS";
const NATIVE_APP_CORS_ORIGINS: &[&str] = &[
    "http://tauri.localhost",
    "https://tauri.localhost",
    "tauri://localhost",
];

#[derive(Clone)]
pub struct AppState {
    pub db: AnyPool,
    pub jwt_secret: String,
    pub base_url: String,
    pub search_index: search::SearchIndex,
    pub oauth_cache: auth::routes::OAuthStateCache,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "starlane_notes_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    storage::init_data_dir().await?;
    tracing::info!(
        "Data directory initialized: {}",
        storage::data_dir().display()
    );

    let pool = db::init_pool().await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let is_production = std::env::var("STARLANE_ENV").unwrap_or_default() == "production";

    // JWT secret: required in production, insecure default only in dev
    let jwt_secret = match std::env::var("JWT_SECRET") {
        Ok(secret) if secret.len() >= 32 => secret,
        Ok(secret) => {
            if is_production {
                anyhow::bail!(
                    "JWT_SECRET must be at least 32 characters in production. Generate one with `openssl rand -hex 32`."
                );
            }
            tracing::warn!(
                "JWT_SECRET is too short ({}). Use at least 32 characters.",
                secret.len()
            );
            secret
        }
        Err(_) => {
            if is_production {
                anyhow::bail!("JWT_SECRET is required in production. Set it in your environment or .env file.");
            }
            tracing::warn!(
                "JWT_SECRET not set — using insecure default. This is NOT safe for production!"
            );
            "starlane-dev-secret-do-not-use-in-production".to_string()
        }
    };

    let base_url =
        std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    // Initialize tantivy search index
    let index_path = storage::data_dir().join("search_index");
    let search_index = search::SearchIndex::new(index_path)?;
    tracing::info!("Search index initialized");

    let state = AppState {
        db: pool,
        jwt_secret,
        base_url: base_url.clone(),
        search_index,
        oauth_cache: auth::routes::OAuthStateCache::new(),
    };
    spaces::workflows::maybe_spawn_deadline_escalation_worker(state.clone());

    // CORS: restrict production to the public web origin plus known native app origins.
    let cors = if std::env::var("STARLANE_ENV").unwrap_or_default() == "production" {
        let origins = production_cors_origins(&base_url)?;
        tracing::info!(
            allowed_origins = ?origins
                .iter()
                .filter_map(|origin| origin.to_str().ok())
                .collect::<Vec<_>>(),
            "Production CORS origins configured",
        );
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::header::ACCEPT,
            ])
    } else {
        CorsLayer::permissive()
    };

    let web_dist_dir = std::env::var("WEB_DIST_DIR").unwrap_or_else(|_| "web-dist".to_string());
    let index_file = format!("{web_dist_dir}/index.html");
    let apple_app_site_association =
        format!("{web_dist_dir}/.well-known/apple-app-site-association.json");

    let app = Router::new()
        .route("/api/docs", get(openapi::scalar_docs))
        .route("/api/openapi.json", get(openapi::openapi_json))
        .route("/api/openapi.yaml", get(openapi::openapi_yaml))
        .nest("/api/v1", api_routes())
        .nest("/scim/v2", scim::router())
        .merge(sync::router().with_state(state.clone()))
        .route_service(
            "/.well-known/apple-app-site-association",
            ServeFile::new(apple_app_site_association),
        )
        .fallback_service(ServeDir::new(web_dist_dir).fallback(ServeFile::new(index_file)))
        .with_state(state)
        .layer(middleware::from_fn(csp_middleware))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starlane Notes server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

fn production_cors_origins(base_url: &str) -> anyhow::Result<Vec<HeaderValue>> {
    production_cors_origins_with_extra(base_url, std::env::var(STARLANE_CORS_ORIGINS_ENV).ok())
}

fn production_cors_origins_with_extra(
    base_url: &str,
    extra_origins: Option<String>,
) -> anyhow::Result<Vec<HeaderValue>> {
    let mut origins = Vec::new();
    push_cors_origin(&mut origins, base_url)?;
    for origin in NATIVE_APP_CORS_ORIGINS {
        push_cors_origin(&mut origins, origin)?;
    }
    if let Some(extra_origins) = extra_origins {
        for origin in extra_origins.split(',') {
            let origin = origin.trim();
            if origin.is_empty() {
                continue;
            }
            push_cors_origin(&mut origins, origin)?;
        }
    }
    Ok(origins)
}

fn push_cors_origin(origins: &mut Vec<HeaderValue>, value: &str) -> anyhow::Result<()> {
    let origin = normalize_cors_origin(value)?;
    if origins.iter().any(|existing| existing == &origin) {
        return Ok(());
    }
    origins.push(origin);
    Ok(())
}

fn normalize_cors_origin(value: &str) -> anyhow::Result<HeaderValue> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        anyhow::bail!("CORS origin cannot be empty");
    }
    if trimmed == "*" {
        anyhow::bail!(
            "{STARLANE_CORS_ORIGINS_ENV} must list explicit origins; wildcard '*' is not allowed in production"
        );
    }
    trimmed
        .parse::<HeaderValue>()
        .map_err(|err| anyhow::anyhow!("Invalid CORS origin '{trimmed}': {err}"))
}

// ============================================================
// Health Check
// ============================================================

async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").fetch_one(&state.db).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "version": env!("CARGO_PKG_VERSION"),
            })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "error",
                "message": "database unavailable",
            })),
        )
            .into_response(),
    }
}

async fn api_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": "not_found",
            "message": "API route not found",
        })),
    )
}

// ============================================================
// CSP Middleware
// ============================================================

async fn csp_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> axum::response::Response {
    let is_api_docs = matches!(request.uri().path(), "/api/docs" | "/api/v1/docs");
    let mut response = next.run(request).await;
    let policy = if is_api_docs {
        concat!(
            "default-src 'self'; ",
            "script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; ",
            "style-src 'self' 'unsafe-inline'; ",
            "img-src 'self' data: blob:; ",
            "font-src 'self'; ",
            "connect-src 'self'; ",
            "worker-src 'self' blob:; ",
            "frame-src 'none'; ",
            "object-src 'none'; ",
            "base-uri 'self'"
        )
    } else {
        concat!(
            "default-src 'self'; ",
            "script-src 'self' 'unsafe-inline'; ",
            "style-src 'self' 'unsafe-inline'; ",
            "img-src 'self' data: blob:; ",
            "font-src 'self'; ",
            "connect-src 'self' ws: wss:; ",
            "worker-src 'self' blob:; ",
            "frame-src 'none'; ",
            "object-src 'none'; ",
            "base-uri 'self'"
        )
    };
    response
        .headers_mut()
        .insert("content-security-policy", policy.parse().unwrap());
    response
}

// ============================================================
// API Routes
// ============================================================

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/docs", get(openapi::scalar_docs))
        .route("/openapi.json", get(openapi::openapi_json))
        .route("/openapi.yaml", get(openapi::openapi_yaml))
        .merge(auth::router())
        .merge(bookmarks::router())
        .merge(provisioning::router())
        .merge(spaces::router())
        .merge(search::router())
        .merge(webhooks::router())
        .merge(audit::router())
        .merge(notifications::router())
        .merge(mcp::router())
        .merge(ai::router())
        .merge(sync::api_router())
        .fallback(api_not_found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin_values(origins: Vec<HeaderValue>) -> Vec<String> {
        origins
            .into_iter()
            .map(|origin| origin.to_str().expect("origin header").to_string())
            .collect()
    }

    #[test]
    fn production_cors_includes_web_and_native_app_origins() {
        let origins = origin_values(
            production_cors_origins_with_extra("https://notes.example.com/", None)
                .expect("cors origins"),
        );

        assert!(origins.contains(&"https://notes.example.com".to_string()));
        assert!(origins.contains(&"http://tauri.localhost".to_string()));
        assert!(origins.contains(&"https://tauri.localhost".to_string()));
        assert!(origins.contains(&"tauri://localhost".to_string()));
    }

    #[test]
    fn production_cors_adds_deduped_env_origins() {
        let origins = origin_values(
            production_cors_origins_with_extra(
                "https://notes.example.com",
                Some("https://mobile.example.com/, http://tauri.localhost".to_string()),
            )
            .expect("cors origins"),
        );

        assert_eq!(
            origins
                .iter()
                .filter(|origin| origin.as_str() == "http://tauri.localhost")
                .count(),
            1,
        );
        assert!(origins.contains(&"https://mobile.example.com".to_string()));
    }

    #[test]
    fn production_cors_rejects_wildcard_origin() {
        assert!(normalize_cors_origin("*").is_err());
    }
}
