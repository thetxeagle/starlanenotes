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
pub mod audit;
mod auth;
mod bookmarks;
mod db;
mod logging;
pub mod mcp;
mod notifications;
mod openapi;
mod provisioning;
mod scim;
pub mod search;
mod spaces;
mod storage;
mod sync;
mod startup_recovery;
mod template_guidance;
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
    pub ws_rooms: sync::WsRooms,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Diagnostic commands run before dotenv, logging, migrations or service writes.
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments.next();
    #[cfg(target_os="linux")]
    if command.as_deref()==Some(std::ffi::OsStr::new("observe-sqlite-historical-readonly")) {
        let usage="Usage: observe-sqlite-historical-readonly <stopped-source> <existing-empty-private-workspace> --writers-stopped --plan <private-expectations> --credential-stdin";
        let source=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;let workspace=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--writers-stopped"))&&arguments.next().as_deref()==Some(std::ffi::OsStr::new("--plan")),usage);let plan=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--credential-stdin"))&&arguments.next().is_none(),usage);
        return storage::restoration::linux::observe_sqlite_historical_readonly(std::path::Path::new(&source),std::path::Path::new(&workspace),std::path::Path::new(&plan)).await;
    }
    #[cfg(target_os="linux")]
    if command.as_deref()==Some(std::ffi::OsStr::new("historical-readonly-worker")) {
        let fd=arguments.next().ok_or_else(||anyhow::anyhow!("Original historical channel descriptor missing"))?.to_string_lossy().parse::<i32>()?;
        anyhow::ensure!(arguments.next().is_none(),"Unexpected historical worker arguments");
        return storage::restoration::linux::run_historical_readonly_worker(fd).await;
    }
    #[cfg(target_os="linux")]
    if command.as_deref()==Some(std::ffi::OsStr::new("verification-normal-worker")) {
        let raw=arguments.next().ok_or_else(||anyhow::anyhow!("Private inherited worker channel absent"))?;
        anyhow::ensure!(arguments.next().is_none(),"Unexpected private worker arguments");
        let fd=raw.to_str().ok_or_else(||anyhow::anyhow!("Invalid worker channel"))?.parse::<i32>()?;
        return storage::restoration::linux::run_inherited_normal_verification_worker(fd).await;
    }
    #[cfg(target_os="linux")]
    if command.as_deref()==Some(std::ffi::OsStr::new("verify-sqlite-ordinary-application")) {
        let usage="Usage: starlane-notes-server verify-sqlite-ordinary-application <stopped-source> <new-private-workspace> --application-version <version> --writers-stopped (original producer/audience access credentials JSON on stdin; requires explicit private VERIFIED_RESTORE_RECORDS_DIR)";
        let source=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;let workspace=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--application-version")),usage);let version=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--writers-stopped"))&&arguments.next().is_none(),usage);
        return storage::restoration::linux::verify_sqlite_ordinary_application(std::path::Path::new(&source),std::path::Path::new(&workspace),version.to_str().ok_or_else(||anyhow::anyhow!(usage))?).await;
    }
    if command.as_deref()==Some(std::ffi::OsStr::new("verify-sqlite-application-startup")) {
        let usage="Usage: starlane-notes-server verify-sqlite-application-startup <stopped-source> <new-private-workspace> --application-version <version> --writers-stopped (supplied human bearer on stdin)";
        let source=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        let workspace=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--application-version")),usage);
        let version=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--writers-stopped"))&&arguments.next().is_none(),usage);
        return storage::restoration::linux::verify_sqlite_normal_startup(std::path::Path::new(&source),std::path::Path::new(&workspace),version.to_str().ok_or_else(||anyhow::anyhow!(usage))?).await;
    }
    #[cfg(target_os="linux")]
    if command.as_deref()==Some(std::ffi::OsStr::new("read-pg-application-verification-history")) {
        let source=arguments.next().ok_or_else(||anyhow::anyhow!("Supply original qualified source; genuine human bearer on stdin"))?;anyhow::ensure!(arguments.next().is_none(),"Unexpected verification history arguments");return storage::restoration::linux::engine_transfer::read_pg_verification_history(std::path::Path::new(&source)).await;
    }
    #[cfg(target_os="linux")]
    if command.as_deref()==Some(std::ffi::OsStr::new("verify-pg-ordinary-application")) {
        let usage="Usage: starlane-notes-server verify-pg-ordinary-application <qualified-stopped-source> <new-private-workspace> --writers-stopped (supplied original administrator/audience credentials JSON on stdin; application probes only, no verified record)";
        let source=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;let workspace=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--writers-stopped"))&&arguments.next().is_none(),usage);
        return storage::restoration::linux::engine_transfer::verify_pg_ordinary_application(std::path::Path::new(&source),std::path::Path::new(&workspace)).await;
    }    #[cfg(target_os="linux")]
    if command.as_deref()==Some(std::ffi::OsStr::new("verify-pg-ordinary-listener-startup")) {
        let usage="Usage: starlane-notes-server verify-pg-ordinary-listener-startup <qualified-stopped-source> <new-private-workspace> --writers-stopped (supplied human bearer on stdin; transport/startup only, no verified record)";
        let source=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;let workspace=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--writers-stopped"))&&arguments.next().is_none(),usage);
        return storage::restoration::linux::engine_transfer::verify_pg_ordinary_listener_startup(std::path::Path::new(&source),std::path::Path::new(&workspace)).await;
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("backup-sqlite-storage")) {
        #[cfg(target_os="linux")]
        {
            let usage="Usage: starlane-notes-server backup-sqlite-storage <source-root> <new-bundle> --application-version <recorded-version> --writers-stopped";
            let source=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;let target=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
            anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--application-version")),usage);let version=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
            anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--writers-stopped"))&&arguments.next().is_none(),usage);
            let bundle=storage::restoration::linux::capture_sqlite_certified(std::path::Path::new(&source),std::path::Path::new(&target),version.to_str().ok_or_else(||anyhow::anyhow!(usage))?,true).await?;bundle.fence()?;
            use sha2::Digest;let report=serde_json::json!({"backup_id":bundle.manifest.backup_id,"bundle":std::path::PathBuf::from(&target),"manifest_sha256":hex::encode(sha2::Sha256::digest(&bundle.raw_manifest)),"compatible_catalogue":bundle.manifest.compatible_catalogue(),"writers_stopped_asserted":true});
            println!("{}",serde_json::to_string_pretty(&report)?);bundle.db.close().await;return Ok(());
        }
        #[cfg(not(target_os="linux"))] anyhow::bail!("Certified SQLite capture currently requires Linux");
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("restore-storage-backup")) {
        let usage = "Usage: starlane-notes-server restore-storage-backup <bundle> <new-destination> --application-version <recorded-version>";
        let bundle = arguments.next().ok_or_else(|| anyhow::anyhow!(usage))?;
        let destination = arguments.next().ok_or_else(|| anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref() == Some(std::ffi::OsStr::new("--application-version")), usage);
        let version = arguments.next().ok_or_else(|| anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().is_none(), usage);
        let version = version.to_str().ok_or_else(|| anyhow::anyhow!(usage))?;
        let report = storage::backup_restore::run(std::path::Path::new(&bundle), std::path::Path::new(&destination), version).await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("transfer-storage-backup")) {
        let usage="Usage: starlane-notes-server transfer-storage-backup <bundle> <new-destination> --application-version <recorded-version> --writers-stopped --retained-artifacts <new-private-directory>";
        let bundle=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        let destination=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--application-version")),usage);
        let version=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--writers-stopped")),usage);
        anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--retained-artifacts")),usage);
        let artifacts=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
        anyhow::ensure!(arguments.next().is_none(),usage);
        let url=db::database_url()?;
        let report=storage::backup_transfer::run(std::path::Path::new(&bundle),std::path::Path::new(&destination),std::path::Path::new(&artifacts),version.to_str().ok_or_else(||anyhow::anyhow!(usage))?,&url).await?;
        println!("{}",serde_json::to_string_pretty(&report)?);return Ok(());
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("inspect-storage-backup")) {
        let bundle = arguments.next().ok_or_else(|| anyhow::anyhow!("Usage: starlane-notes-server inspect-storage-backup <bundle>"))?;
        anyhow::ensure!(arguments.next().is_none(), "Usage: starlane-notes-server inspect-storage-backup <bundle>");
        let report = storage::backup_inspect::run(std::path::Path::new(&bundle)).await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    if command.as_deref()==Some(std::ffi::OsStr::new("backup-postgres-storage")) || command.as_deref()==Some(std::ffi::OsStr::new("restore-postgres-storage")) {
        #[cfg(target_os="linux")]
        {
            let backup=command.as_deref()==Some(std::ffi::OsStr::new("backup-postgres-storage"));
            let usage=if backup{"Usage: starlane-notes-server backup-postgres-storage <qualified-source-root> <new-bundle> --writers-stopped"}else{"Usage: starlane-notes-server restore-postgres-storage <bundle> <new-destination> --writers-stopped"};
            let source=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
            let target=arguments.next().ok_or_else(||anyhow::anyhow!(usage))?;
            anyhow::ensure!(arguments.next().as_deref()==Some(std::ffi::OsStr::new("--writers-stopped"))&&arguments.next().is_none(),usage);
            let url=db::database_url()?;
            if backup {
                let report=storage::restoration::linux::engine_transfer::backup_qualified_postgres(std::path::Path::new(&source),std::path::Path::new(&target),&url,true).await?;
                println!("{}",serde_json::to_string_pretty(&report)?);
            }else{
                let report=storage::restoration::linux::engine_transfer::restore_qualified_postgres(std::path::Path::new(&source),std::path::Path::new(&target),&url,true).await?;
                println!("{}",serde_json::to_string_pretty(&report)?);
            }
            return Ok(());
        }
        #[cfg(not(target_os="linux"))]
        anyhow::bail!("Qualified PostgreSQL snapshot/restoration currently requires Linux");
    }
    // A mistyped operator command must never enter normal service startup.
    if let Some(unknown)=command {
        anyhow::bail!("Unknown command {:?}; start the server without positional arguments",unknown);
    }
    dotenvy::dotenv().ok();
    #[cfg(target_os="linux")]
    let ordinary_listener=storage::restoration::linux::take_inherited_ordinary_listener()?;
    #[cfg(target_os="linux")]
    let service_observer=storage::restoration::linux::ServiceObservationChild::receive_if_configured()?;
    #[cfg(target_os="linux")]
    anyhow::ensure!(ordinary_listener.is_none()||service_observer.is_none(),"Inherited listener and observation-only protocols must stay separate");

    // Refuse interrupted restoration before logging or data-directory writes.
    #[cfg(target_os="linux")]
    let engine_serving=if storage::restoration::engine_startup_requested(&storage::data_dir())? {
        let url=db::database_url()?;
        Some(std::sync::Arc::new(storage::restoration::open_current_qualified_pg_serving(&storage::data_dir(),&url).await?))
    } else {storage::restoration::bootstrap_gate(&storage::data_dir())?;None};
    #[cfg(not(target_os="linux"))]
    storage::restoration::bootstrap_gate(&storage::data_dir())?;

    // Initialize designated or markerless-compatible service directories before logging can create DATA_DIR.
    // Operator commands and restoration refusal above remain read-only early exits.
    storage::init_data_dir().await?;

    let file_log_writer = logging::init()?;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "starlane_notes_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(file_log_writer),
        )
        .init();
    tracing::info!(
        log_directory = %storage::data_dir().join("logging").display(),
        max_age_days = 30,
        max_directory_bytes = 1024_u64 * 1024 * 1024,
        "Durable file logging enabled"
    );

    tracing::info!(
        "Data directory initialized: {}",
        storage::data_dir().display()
    );

    #[cfg(target_os="linux")]
    let (pool,engine)=if let Some(serving)=&engine_serving {
        serving.fence().await?;(serving.pool().clone(),db::DatabaseEngine::Postgres)
    } else {
        let (pool,engine)=db::init_pool().await?;
        db::migrate_selected(&pool,engine).await?;
        storage::restoration::activate_in(&storage::data_dir(),&pool).await
            .map_err(|e|anyhow::anyhow!("Restoration custody activation: {e:?}"))?;
        (pool,engine)
    };
    #[cfg(not(target_os="linux"))]
    let (pool,engine)={
        let (pool,engine)=db::init_pool().await?;db::migrate_selected(&pool,engine).await?;
        storage::restoration::activate_in(&storage::data_dir(),&pool).await
            .map_err(|e|anyhow::anyhow!("Restoration custody activation: {e:?}"))?;
        (pool,engine)
    };
    #[cfg(target_os="linux")]
    startup_recovery::recover_initialized(&pool,None).await?;
    #[cfg(not(target_os="linux"))]
    startup_recovery::recover_initialized(&pool).await?;

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
        ws_rooms: crate::sync::WsRooms::default(),
    };
    #[cfg(target_os="linux")]
    if storage::restoration::linux::engine_transfer::retain_normal_application_history(engine_serving.clone(),&state).await.is_err(){tracing::warn!("Historical isolated application-test reader unavailable; ordinary service remains active");}
    #[cfg(target_os="linux")]
    if storage::restoration::linux::engine_transfer::retain_sqlite_ordinary_history(&state).await.is_err(){tracing::warn!("Historical SQLite application-test reader unavailable; ordinary service remains active");}
    #[cfg(target_os="linux")]
    if storage::restoration::linux::engine_transfer::retain_normal_pg_backup_history(engine_serving.clone(),&state).await.is_err(){tracing::warn!("PostgreSQL byte-backup history reader unavailable; ordinary service remains active");}
    spaces::workflows::maybe_spawn_deadline_escalation_worker(state.clone());

    let app = registered_application(state, &base_url)?;

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    #[cfg(target_os="linux")]
    let addr=if service_observer.is_some(){SocketAddr::from(([127,0,0,1],port))}else{addr};


    #[cfg(target_os="linux")]
    let (listener,_ordinary_listener_lifetime)=match ordinary_listener {
        Some(selected)=>{let(listener,lifetime)=selected.ready()?;(listener,Some(lifetime))},
        None=>(tokio::net::TcpListener::bind(addr).await?,None),
    };
    #[cfg(not(target_os="linux"))]
    let listener = tokio::net::TcpListener::bind(addr).await?;
    #[cfg(target_os="linux")]
    let service_observation_lifetime=match service_observer{Some(observer)=>Some(observer.ready(&listener)?),None=>None};
    tracing::info!("Starlane Notes server listening on {}", listener.local_addr()?);

    let serving=axum::serve(listener,app.into_make_service_with_connect_info::<SocketAddr>());
    #[cfg(target_os="linux")]
    if let Some(owner)=service_observation_lifetime{tokio::select!{result=std::future::IntoFuture::into_future(serving)=>{result?;},_=owner.parent_retired()=>anyhow::bail!("Original service observation parent retired")}}else{serving.await?;}
    #[cfg(not(target_os="linux"))]
    serving.await?;

    Ok(())
}

pub(crate) fn registered_application(state: AppState, base_url: &str) -> anyhow::Result<Router> {
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
        production_cors_layer(origins)
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

    Ok(app)
}

fn production_cors_layer(origins: Vec<HeaderValue>) -> CorsLayer {
    CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .expose_headers([axum::http::HeaderName::from_static("x-starlane-delete-operation")])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::header::ACCEPT,
                axum::http::HeaderName::from_static("x-starlane-delete-operation"),
                axum::http::HeaderName::from_static("x-starlane-upload-operation"),
                axum::http::HeaderName::from_static("x-starlane-upload-length"),
                axum::http::HeaderName::from_static("x-starlane-attachment-version"),
                axum::http::HeaderName::from_static("x-starlane-attachment-revision"),
            ])
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

    #[tokio::test]
    async fn production_cors_allows_native_attachment_preflight_and_refuses_foreign_origin() {
        use axum::{body::Body, http::Request, routing::post, Router};
        use tower::ServiceExt;
        let headers = "authorization,content-type,x-starlane-upload-operation,x-starlane-upload-length,x-starlane-attachment-version,x-starlane-attachment-revision";
        let app = Router::new().route("/attachments", post(|| async { "unused" }))
            .layer(production_cors_layer(production_cors_origins_with_extra("https://notes.example.com", None).unwrap()));
        for origin in ["tauri://localhost", "https://foreign.example.com"] {
            let response = app.clone().oneshot(Request::builder().method("OPTIONS").uri("/attachments")
                .header("origin", origin).header("access-control-request-method", "POST")
                .header("access-control-request-headers", headers).body(Body::empty()).unwrap()).await.unwrap();
            if origin == "tauri://localhost" {
                assert_eq!(response.headers().get("access-control-allow-origin").unwrap(), origin);
                let allowed = response.headers().get("access-control-allow-headers").unwrap().to_str().unwrap();
                for header in headers.split(',') { assert!(allowed.split(',').any(|value| value.trim() == header), "missing {header}"); }
            } else {
                assert!(response.headers().get("access-control-allow-origin").is_none());
            }
        }
    }

    #[tokio::test]
    async fn production_cors_original_delete_header_and_ack_exposure() {
      use axum::{body::Body,http::Request,routing::delete};use tower::ServiceExt;
      let app=Router::new().route("/trash",delete(||async{([("x-starlane-delete-operation","original")],axum::http::StatusCode::NO_CONTENT)})).layer(production_cors_layer(production_cors_origins_with_extra("https://notes.example.com",None).unwrap()));
      for origin in ["https://notes.example.com","tauri://localhost","https://foreign.example.com"] {
        let response=app.clone().oneshot(Request::builder().method("OPTIONS").uri("/trash").header("origin",origin).header("access-control-request-method","DELETE").header("access-control-request-headers","authorization,content-type,x-starlane-delete-operation").body(Body::empty()).unwrap()).await.unwrap();
        if origin=="https://foreign.example.com"{assert!(response.headers().get("access-control-allow-origin").is_none());continue}
        assert_eq!(response.headers()["access-control-allow-origin"],origin);assert!(response.headers()["access-control-allow-headers"].to_str().unwrap().split(',').any(|v|v.trim()=="x-starlane-delete-operation"));
        let response=app.clone().oneshot(Request::builder().method("DELETE").uri("/trash").header("origin",origin).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.headers()["access-control-expose-headers"],"x-starlane-delete-operation");assert_eq!(response.headers()["x-starlane-delete-operation"],"original");
      }
    }

    #[test]
    fn production_cors_rejects_wildcard_origin() {
        assert!(normalize_cors_origin("*").is_err());
    }
}

#[cfg(test)]
mod restoration_application_acceptance;

#[cfg(test)]
#[path="postgres_runtime_fixture.rs"]
mod postgres_runtime_fixture;

#[cfg(test)]
#[path = "postgres_core_fixture.rs"]
mod postgres_core_fixture;

#[cfg(test)]
#[path = "postgres_password_fixture.rs"]
mod postgres_password_fixture;

#[cfg(test)]
#[path = "postgres_session_rename_fixture.rs"]
mod postgres_session_rename_fixture;

#[cfg(all(test, target_os = "linux"))]
mod attachment_upload_engine_fixture;

#[cfg(all(test, target_os = "linux"))]
#[path = "postgres_reset_fixture.rs"]
mod postgres_reset_fixture;

#[cfg(all(test,target_os="linux"))]
#[path="postgres_api_key_fixture.rs"]
mod postgres_api_key_fixture;

#[cfg(all(test, target_os = "linux"))]
#[path = "postgres_signup_fixture.rs"]
mod postgres_signup_fixture;

#[cfg(all(test, target_os = "linux"))]
#[path = "postgres_native_fixture.rs"]
mod postgres_native_fixture;

#[cfg(all(test, target_os = "linux"))]
mod postgres_totp_fixture;

#[cfg(all(test, target_os = "linux"))]
mod storage_health_fixture;

#[cfg(all(test, target_os = "linux"))]
mod postgres_passkey_fixture;

#[cfg(all(test, target_os = "linux"))]
mod postgres_passkey_crypto_fixture;

#[cfg(all(test, target_os = "linux"))]
mod postgres_passkey_atomic_fixture;

#[cfg(all(test, target_os = "linux"))]
mod postgres_admin_reset_fixture;

#[cfg(all(test, target_os = "linux"))]
mod attachment_version_move_fixture;

#[cfg(test)]
#[path = "postgres_lifecycle_fixture.rs"]
mod postgres_lifecycle_fixture;

#[cfg(test)]
mod deep_folder_fixture;

#[cfg(test)]
mod deep_page_fixture;

#[cfg(test)]
mod mixed_hierarchy_fixture;

#[cfg(test)]
mod draft_transfer_engine_fixture;
