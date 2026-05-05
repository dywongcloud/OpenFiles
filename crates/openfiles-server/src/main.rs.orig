use anyhow::{Context, Result};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use clap::Parser;
use openfiles_core::{
    sync::{spawn_background_sync, BackgroundSyncConfig},
    vendor::build_backend,
    OpenFilesConfig, OpenFilesEngine,
};
use serde::Deserialize;
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tower_http::trace::TraceLayer;

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long, default_value = "openfiles.toml")]
    config: PathBuf,
    #[arg(long, default_value = "127.0.0.1:8787")]
    listen: SocketAddr,
}

#[derive(Clone)]
struct AppState {
    engine: Arc<OpenFilesEngine>,
}

#[derive(Debug)]
struct ApiError(openfiles_core::OpenFilesError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            openfiles_core::OpenFilesError::NotFound(_) => StatusCode::NOT_FOUND,
            openfiles_core::OpenFilesError::Conflict(_) => StatusCode::CONFLICT,
            openfiles_core::OpenFilesError::InvalidPath(_) => StatusCode::BAD_REQUEST,
            openfiles_core::OpenFilesError::Unsupported(_) => StatusCode::NOT_IMPLEMENTED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(serde_json::json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

impl From<openfiles_core::OpenFilesError> for ApiError {
    fn from(value: openfiles_core::OpenFilesError) -> Self {
        Self(value)
    }
}

type ApiResult<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Deserialize)]
struct ReadQuery {
    offset: Option<u64>,
    len: Option<u64>,
}

async fn health() -> &'static str {
    "ok"
}

fn slash(path: String) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{path}")
    }
}

async fn stat(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> ApiResult<Json<openfiles_core::FileStat>> {
    Ok(Json(state.engine.stat(&slash(path)).await?))
}

async fn stat_root(State(state): State<AppState>) -> ApiResult<Json<openfiles_core::FileStat>> {
    Ok(Json(state.engine.stat("/").await?))
}

async fn list(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> ApiResult<Json<Vec<openfiles_core::DirEntry>>> {
    Ok(Json(state.engine.list_dir(&slash(path)).await?))
}

async fn list_root(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<openfiles_core::DirEntry>>> {
    Ok(Json(state.engine.list_dir("/").await?))
}

async fn read_file(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<ReadQuery>,
) -> ApiResult<Response> {
    let p = slash(path);
    let bytes = match (query.offset, query.len) {
        (Some(offset), Some(len)) => state.engine.read_range(&p, offset, len).await?,
        _ => state.engine.read_all(&p).await?,
    };
    Ok((StatusCode::OK, bytes).into_response())
}

async fn write_file(
    State(state): State<AppState>,
    Path(path): Path<String>,
    body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    let p = slash(path);
    state.engine.write_file(&p, body).await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": p })))
}

async fn delete_file(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let p = slash(path);
    state.engine.delete_path(&p).await?;
    Ok(Json(serde_json::json!({ "ok": true, "path": p })))
}

#[derive(Debug, Deserialize)]
struct RenameBody {
    from: String,
    to: String,
}

async fn rename(
    State(state): State<AppState>,
    Json(body): Json<RenameBody>,
) -> ApiResult<Json<serde_json::Value>> {
    state.engine.rename_path(&body.from, &body.to).await?;
    Ok(Json(
        serde_json::json!({ "ok": true, "from": body.from, "to": body.to }),
    ))
}

async fn flush(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let n = state.engine.flush().await?;
    Ok(Json(serde_json::json!({ "flushed": n })))
}

async fn expire(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let n = state.engine.expire_cache().await?;
    Ok(Json(serde_json::json!({ "expired": n })))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "openfiles_server=info,tower_http=info".to_string()),
        )
        .init();

    let args = Args::parse();
    let config = OpenFilesConfig::from_toml_file(&args.config)
        .with_context(|| format!("failed to load {}", args.config.display()))?;
    let backend = build_backend(&config.backend)?;
    let engine = OpenFilesEngine::new(config.clone(), backend).await?;
    let flush_interval = Duration::from_secs(config.sync.export_batch_window_secs.max(1));
    let _sync = spawn_background_sync(
        engine.clone(),
        BackgroundSyncConfig {
            flush_interval,
            ..Default::default()
        },
    );

    let state = AppState {
        engine: Arc::new(engine),
    };
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/v1/stat", get(stat_root))
        .route("/v1/stat/{*path}", get(stat))
        .route("/v1/list", get(list_root))
        .route("/v1/list/{*path}", get(list))
        .route("/v1/read/{*path}", get(read_file))
        .route("/v1/write/{*path}", put(write_file))
        .route("/v1/delete/{*path}", delete(delete_file))
        .route("/v1/rename", post(rename))
        .route("/v1/flush", post(flush))
        .route("/v1/expire", post(expire))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    tracing::info!(listen=%args.listen, "openfiles HTTP server listening");
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
