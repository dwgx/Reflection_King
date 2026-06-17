mod state;

use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::{ConnectInfo, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use reflection_core::{
    models::{
        normalize_bitrate, normalize_outputs, normalize_profile_id, ApiKeyRecord, ApiKeyRole,
        ApiKeyView, ArchiveTreeView, AuthMode, CacheCleanupRequest, CacheCleanupView,
        CacheInventoryView, ClearJobsResponse, CreateJobRequest, CreateUserKeyRequest,
        CreatedUserKeyResponse, DiscoveryMode, HiddenJobBatchView, JobCreateOptions, JobRecord,
        JobView, OutputKind, PlatformHint, RestoreJobsResponse, SelectCandidatesRequest,
        UpdateRuntimeSettingsRequest,
    },
    url_policy::parse_and_validate_url,
    AppConfig, RkError,
};
use serde::Deserialize;
use state::AppState;
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
    net::TcpListener,
};
use tokio_util::io::ReaderStream;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = AppConfig::from_env()?;
    let bind_address = config.bind_address;
    let state = Arc::new(AppState::new(config).await?);
    state.spawn_workers();

    let app = build_router(state);
    let listener = TcpListener::bind(bind_address).await?;
    info!("Reflection King API listening on http://{bind_address}");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/assets/{*path}", get(dashboard_asset))
        .route("/api/health", get(health))
        .route("/api/capabilities", get(capabilities))
        .route("/api/jobs", get(list_jobs).post(create_job))
        .route("/api/jobs/clear", post(clear_jobs))
        .route("/api/jobs/restore", post(restore_jobs))
        .route("/api/jobs/hidden-batches", get(list_hidden_batches))
        .route(
            "/api/jobs/hidden-batches/{id}/restore",
            post(restore_hidden_batch),
        )
        .route("/api/jobs/{id}", get(get_job))
        .route("/api/jobs/{id}/candidates", get(list_candidates))
        .route("/api/jobs/{id}/select-candidates", post(select_candidates))
        .route("/api/jobs/{id}/artifacts", get(list_artifacts))
        .route("/api/jobs/{id}/archive/tree", get(get_archive_tree))
        .route("/api/jobs/{id}/archive/file", get(get_archive_file))
        .route("/api/jobs/{id}/trace", get(get_trace))
        .route(
            "/api/jobs/{id}/browser-login-session",
            post(start_job_browser_login_session),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/snapshot",
            get(job_browser_login_session_snapshot),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/click",
            post(job_browser_login_session_click),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/move",
            post(job_browser_login_session_move),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/mouse-down",
            post(job_browser_login_session_mouse_down),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/mouse-up",
            post(job_browser_login_session_mouse_up),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/type",
            post(job_browser_login_session_type),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/insert-text",
            post(job_browser_login_session_insert_text),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/press",
            post(job_browser_login_session_press),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/navigate",
            post(job_browser_login_session_navigate),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/wheel",
            post(job_browser_login_session_wheel),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/resize",
            post(job_browser_login_session_resize),
        )
        .route(
            "/api/jobs/{id}/browser-login-session/{session_id}/close",
            post(job_close_browser_login_session),
        )
        .route(
            "/api/jobs/{id}/resume-with-profile",
            post(resume_job_with_profile),
        )
        .route(
            "/api/jobs/{id}/force-page-archive",
            post(force_page_archive),
        )
        .route(
            "/api/admin/user-keys",
            get(list_user_keys).post(create_user_key),
        )
        .route("/api/admin/user-keys/{id}/revoke", post(revoke_user_key))
        .route("/api/admin/admin-key/rotate", post(rotate_admin_key))
        .route(
            "/api/admin/settings",
            get(get_admin_settings).patch(update_admin_settings),
        )
        .route("/api/admin/cache", get(get_admin_cache))
        .route(
            "/api/admin/cache/cleanup-preview",
            post(preview_admin_cache_cleanup),
        )
        .route("/api/admin/cache/cleanup", post(run_admin_cache_cleanup))
        .route(
            "/api/admin/browser-profiles/{profile_id}/cookies/import",
            post(import_profile_cookies),
        )
        .route(
            "/api/admin/browser-profiles/{profile_id}/login-sessions",
            post(start_browser_login_session),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/snapshot",
            get(browser_login_session_snapshot),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/click",
            post(browser_login_session_click),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/move",
            post(browser_login_session_move),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/mouse-down",
            post(browser_login_session_mouse_down),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/mouse-up",
            post(browser_login_session_mouse_up),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/type",
            post(browser_login_session_type),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/insert-text",
            post(browser_login_session_insert_text),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/press",
            post(browser_login_session_press),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/navigate",
            post(browser_login_session_navigate),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/wheel",
            post(browser_login_session_wheel),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/resize",
            post(browser_login_session_resize),
        )
        .route(
            "/api/admin/browser-login-sessions/{session_id}/close",
            post(close_browser_login_session),
        )
        .route("/media/{id}/{filename}", get(get_media).head(head_media))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn index() -> Response {
    match fs::read_to_string("crates/reflection-api/dashboard-dist/index.html").await {
        Ok(html) => Html(html).into_response(),
        Err(_) => Html(include_str!("../../../docs/static/index.html")).into_response(),
    }
}

async fn dashboard_asset(Path(path): Path<String>) -> Result<Response, ApiError> {
    if path.contains("..") || path.contains('\\') {
        return Err(RkError::BadRequest("invalid asset path".to_string()).into());
    }
    let full_path = std::path::Path::new("crates/reflection-api/dashboard-dist/assets").join(path);
    let bytes = fs::read(&full_path).await?;
    let content_type = full_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(content_type_for)
        .unwrap_or("application/octet-stream");
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .map_err(|error| RkError::Source(format!("failed to build response: {error}")))?)
}

async fn health(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, ApiError> {
    let settings = state.runtime_settings_view().await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "service": "reflection-king",
        "version": env!("CARGO_PKG_VERSION"),
        "public_base_url": settings.public_base_url,
        "storage_dir": state.paths.root().display().to_string(),
        "database_path": state.paths.database_path().display().to_string(),
        "ffmpeg_path": state.config.ffmpeg_path.display().to_string(),
        "max_download_bytes": settings.max_download_bytes,
    })))
}

async fn capabilities(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    let settings = state.runtime_settings_view().await?;
    let effective_max_download_bytes = principal
        .max_download_bytes
        .map(|limit| limit.min(settings.max_download_bytes))
        .unwrap_or(settings.max_download_bytes);
    let allow_browser_probe = principal.allow_browser_probe && state.browser_probe_configured();
    let allow_ytdlp = principal.allow_ytdlp && state.yt_dlp_configured();
    let allow_external_adapters = principal.allow_external_adapters
        && (state.yt_dlp_configured() || state.external_tools_configured());

    Ok(Json(serde_json::json!({
        "service": "reflection-king",
        "version": env!("CARGO_PKG_VERSION"),
        "browser_probe_configured": allow_browser_probe,
        "yt_dlp_configured": allow_ytdlp,
        "external_adapters_configured": allow_external_adapters,
        "external_tools": state.configured_external_tool_names(),
        "ffmpeg_path": state.config.ffmpeg_path.display().to_string(),
        "yt_dlp_path": state.config.yt_dlp_path.as_ref().map(|path| path.display().to_string()),
        "you_get_path": state.config.you_get_path.as_ref().map(|path| path.display().to_string()),
        "lux_path": state.config.lux_path.as_ref().map(|path| path.display().to_string()),
        "streamlink_path": state.config.streamlink_path.as_ref().map(|path| path.display().to_string()),
        "public_base_url": settings.public_base_url,
        "max_download_bytes": effective_max_download_bytes,
        "system_max_download_bytes": settings.max_download_bytes,
        "max_concurrent_jobs": state.config.max_concurrent_jobs,
        "browser_probe_timeout_seconds": state.config.browser_probe_timeout.as_secs(),
        "yt_dlp_timeout_seconds": settings.yt_dlp_timeout_seconds,
        "yt_dlp_max_json_bytes": settings.yt_dlp_max_json_bytes,
        "download_timeout_seconds": settings.download_timeout_seconds,
        "job_ttl_hours": settings.job_ttl_hours,
        "supported_discovery": supported_discovery_modes(&principal),
        "supported_platform_hints": [
            "auto", "bilibili", "youtube", "soundcloud", "douyin", "kuaishou", "pornhub",
            "acfun", "iqiyi", "youku", "tiktok", "vimeo", "live", "generic"
        ],
        "supported_outputs": ["audio", "video", "image", "page_html"],
        "auth": {
            "role": principal.role.as_str(),
            "label": principal.label,
            "allow_browser_probe": principal.allow_browser_probe,
            "allow_ytdlp": principal.allow_ytdlp,
            "allow_external_adapters": principal.allow_external_adapters,
            "allow_login_profile": principal.allow_login_profile,
            "max_download_bytes": principal.max_download_bytes,
        },
    })))
}

#[derive(Debug, Deserialize)]
struct ListJobsQuery {
    limit: Option<usize>,
}

async fn list_jobs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<Vec<JobView>>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    if principal.role == ApiKeyRole::Admin {
        Ok(Json(state.list_jobs(limit).await?))
    } else if let Some(key_id) = principal.key_id {
        Ok(Json(state.list_jobs_for_key(key_id, limit).await?))
    } else {
        Err(RkError::Unauthorized.into())
    }
}

async fn clear_jobs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ClearJobsResponse>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    let response = if principal.role == ApiKeyRole::Admin {
        state
            .hide_visible_jobs(principal.key_id, Some(&principal.label))
            .await?
    } else if let Some(key_id) = principal.key_id {
        state
            .hide_visible_jobs_for_key(key_id, Some(&principal.label))
            .await?
    } else {
        return Err(RkError::Unauthorized.into());
    };

    Ok(Json(response))
}

async fn restore_jobs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<RestoreJobsResponse>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    let response = if principal.role == ApiKeyRole::Admin {
        state.restore_latest_hidden_job_batch(None).await?
    } else if let Some(key_id) = principal.key_id {
        state.restore_latest_hidden_job_batch(Some(key_id)).await?
    } else {
        return Err(RkError::Unauthorized.into());
    };

    Ok(Json(response))
}

async fn list_hidden_batches(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<Vec<HiddenJobBatchView>>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    if principal.role == ApiKeyRole::Admin {
        Ok(Json(state.list_hidden_job_batches(None, limit).await?))
    } else if let Some(key_id) = principal.key_id {
        Ok(Json(
            state.list_hidden_job_batches(Some(key_id), limit).await?,
        ))
    } else {
        Err(RkError::Unauthorized.into())
    }
}

async fn restore_hidden_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<RestoreJobsResponse>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    let response = if principal.role == ApiKeyRole::Admin {
        state.restore_hidden_job_batch(None, id).await?
    } else if let Some(key_id) = principal.key_id {
        state.restore_hidden_job_batch(Some(key_id), id).await?
    } else {
        return Err(RkError::Unauthorized.into());
    };

    Ok(Json(response))
}

async fn create_job(
    State(state): State<Arc<AppState>>,
    ConnectInfo(remote_addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<CreateJobRequest>,
) -> Result<(StatusCode, Json<JobView>), ApiError> {
    let principal = authorize(&state, &headers).await?;

    let source_url = request.url.trim();
    if source_url.is_empty() {
        return Err(RkError::BadRequest("missing url".to_string()).into());
    }
    let source_url: String = parse_and_validate_url(source_url)?.into();

    // Requester provenance ("who / what IP / what browser"): prefer a proxy's
    // forwarded client IP, fall back to the socket peer address.
    let requester_ip = header_str(&headers, "x-forwarded-for")
        .and_then(|value| value.split(',').next().map(|ip| ip.trim().to_string()))
        .unwrap_or_else(|| remote_addr.ip().to_string());
    let requester_user_agent = header_str(&headers, header::USER_AGENT.as_str());

    let bitrate = normalize_bitrate(request.bitrate.as_deref());
    let outputs = normalize_outputs(request.outputs);
    let requested_discovery = request.discovery.unwrap_or_else(|| {
        if outputs.contains(&reflection_core::models::OutputKind::PageHtml) {
            DiscoveryMode::Browser
        } else {
            DiscoveryMode::Direct
        }
    });
    let discovery = authorized_discovery(requested_discovery, &outputs, &principal)?;
    let platform_hint = request
        .platform_hint
        .unwrap_or_else(|| infer_platform(&source_url));
    let profile_id = normalize_profile_id(request.profile_id);
    let auth_mode = normalize_auth_mode_for_outputs(request.auth_mode, &outputs);
    let settings = state.runtime_settings_view().await?;
    let record = JobRecord::new_with_options(
        source_url,
        bitrate,
        settings.public_base_url.as_str(),
        JobCreateOptions {
            discovery,
            platform_hint,
            outputs,
            profile_id,
            auth_mode,
        },
    )
    .with_requester(
        Some(requester_ip),
        requester_user_agent,
        Some(principal.label.clone()),
        principal.key_id,
    );
    let view = state.insert_and_enqueue(record).await?;

    Ok((StatusCode::ACCEPTED, Json(view)))
}

async fn get_job(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<JobView>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_job_access(&state, &principal, id).await?;
    let job = state
        .get_job_view(id)
        .await
        .and_then(|job| job.ok_or_else(|| RkError::NotFound(format!("job {id}"))))?;
    Ok(Json(job))
}

async fn list_candidates(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<reflection_core::models::MediaCandidate>>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(state.list_candidates(id).await?))
}

async fn select_candidates(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(request): Json<SelectCandidatesRequest>,
) -> Result<Json<JobView>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state.select_candidates(id, request.candidate_ids).await?,
    ))
}

async fn list_artifacts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<reflection_core::models::ArtifactView>>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(state.list_artifacts(id).await?))
}

#[derive(Debug, Deserialize)]
struct ArchiveFileQuery {
    path: String,
}

async fn get_archive_tree(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<ArchiveTreeView>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_job_access(&state, &principal, id).await?;
    let settings = state.runtime_settings_view().await?;
    Ok(Json(
        state.archive_tree(id, &settings.public_base_url).await?,
    ))
}

async fn get_archive_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<ArchiveFileQuery>,
) -> Result<Response, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_job_access(&state, &principal, id).await?;
    let path = state.archive_file_path(id, &query.path).await?;
    let file = File::open(&path).await?;
    let file_len = file.metadata().await?.len();
    if file_len == 0 {
        return Err(RkError::Source("archive file is empty".to_string()).into());
    }
    let content_type = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(content_type_for)
        .unwrap_or("application/octet-stream");
    let stream = ReaderStream::new(file);
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, file_len.to_string())
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::from_stream(stream))
        .map_err(|error| RkError::Source(format!("failed to build response: {error}")))?)
}

/// Full observability timeline for a job: every step, outbound request, browser
/// session, ffprobe, and ffmpeg run.
async fn get_trace(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<reflection_core::observability::JobTrace>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_job_access(&state, &principal, id).await?;
    state
        .get_job(id)
        .await
        .and_then(|job| job.ok_or_else(|| RkError::NotFound(format!("job {id}"))))?;
    Ok(Json(state.get_trace(id).await?))
}

async fn start_job_browser_login_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    let job = state
        .get_job(id)
        .await?
        .ok_or_else(|| RkError::NotFound(format!("job {id}")))?;
    Ok(Json(
        state
            .start_job_browser_login_session(&job, principal.key_id)
            .await?,
    ))
}

async fn job_browser_login_session_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state.browser_login_session_snapshot(&session_id).await?,
    ))
}

async fn job_browser_login_session_click(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<BrowserLoginClickRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state
            .browser_login_session_click(
                &session_id,
                request.x,
                request.y,
                request.button.as_deref(),
                request.click_count,
            )
            .await?,
    ))
}

async fn job_browser_login_session_move(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<BrowserLoginMoveRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state
            .browser_login_session_move(&session_id, request.x, request.y)
            .await?,
    ))
}

async fn job_browser_login_session_mouse_down(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<BrowserLoginMouseButtonRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state
            .browser_login_session_mouse_down(
                &session_id,
                request.x,
                request.y,
                request.button.as_deref(),
            )
            .await?,
    ))
}

async fn job_browser_login_session_mouse_up(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<BrowserLoginMouseButtonRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state
            .browser_login_session_mouse_up(
                &session_id,
                request.x,
                request.y,
                request.button.as_deref(),
            )
            .await?,
    ))
}

async fn job_browser_login_session_type(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<BrowserLoginTypeRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state
            .browser_login_session_type(&session_id, &request.text)
            .await?,
    ))
}

async fn job_browser_login_session_insert_text(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<BrowserLoginTypeRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state
            .browser_login_session_insert_text(&session_id, &request.text)
            .await?,
    ))
}

async fn job_browser_login_session_press(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<BrowserLoginPressRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state
            .browser_login_session_press(&session_id, &request.key)
            .await?,
    ))
}

async fn job_browser_login_session_navigate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<BrowserLoginNavigateRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state
            .browser_login_session_navigate(&session_id, request.url.trim())
            .await?,
    ))
}

async fn job_browser_login_session_wheel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<BrowserLoginWheelRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state
            .browser_login_session_wheel(
                &session_id,
                request.delta_x.unwrap_or_default(),
                request.delta_y.unwrap_or_default(),
                request.x,
                request.y,
            )
            .await?,
    ))
}

async fn job_browser_login_session_resize(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
    Json(request): Json<BrowserLoginResizeRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(
        state
            .browser_login_session_resize(&session_id, request.width, request.height)
            .await?,
    ))
}

async fn job_close_browser_login_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, session_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(state.close_browser_login_session(&session_id).await?))
}

async fn resume_job_with_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<JobView>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(state.resume_job_with_profile(id).await?))
}

async fn force_page_archive(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<JobView>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    if !principal.allow_browser_probe {
        return Err(RkError::Unauthorized.into());
    }
    ensure_job_access(&state, &principal, id).await?;
    Ok(Json(state.force_page_archive(id).await?))
}

async fn list_user_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<ApiKeyView>>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_admin(&principal)?;
    Ok(Json(state.list_api_keys().await?))
}

async fn create_user_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CreateUserKeyRequest>,
) -> Result<(StatusCode, Json<CreatedUserKeyResponse>), ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_admin(&principal)?;
    Ok((
        StatusCode::CREATED,
        Json(state.create_user_key(request).await?),
    ))
}

async fn revoke_user_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_admin(&principal)?;
    if state.revoke_api_key(id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(RkError::NotFound(format!("api key {id}")).into())
    }
}

async fn rotate_admin_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<reflection_core::models::RotatedAdminKeyResponse>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_admin(&principal)?;
    let request = if body.is_empty() {
        None
    } else {
        Some(serde_json::from_slice::<RotateAdminKeyRequest>(&body).map_err(RkError::Json)?)
    };
    Ok(Json(
        state
            .rotate_admin_key(request.as_ref().and_then(|body| body.key.as_deref()))
            .await?,
    ))
}

async fn get_admin_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<reflection_core::models::RuntimeSettingsView>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_admin(&principal)?;
    Ok(Json(state.runtime_settings_view().await?))
}

async fn update_admin_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<UpdateRuntimeSettingsRequest>,
) -> Result<Json<reflection_core::models::RuntimeSettingsView>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_admin(&principal)?;
    Ok(Json(state.update_runtime_settings(request).await?))
}

async fn get_admin_cache(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<CacheInventoryView>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_admin(&principal)?;
    Ok(Json(state.cache_inventory().await?))
}

async fn preview_admin_cache_cleanup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CacheCleanupRequest>,
) -> Result<Json<CacheCleanupView>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_admin(&principal)?;
    Ok(Json(state.cache_cleanup_preview(request).await?))
}

async fn run_admin_cache_cleanup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CacheCleanupRequest>,
) -> Result<Json<CacheCleanupView>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_admin(&principal)?;
    Ok(Json(state.cache_cleanup(request).await?))
}

#[derive(Debug, Deserialize)]
struct RotateAdminKeyRequest {
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImportCookiesRequest {
    cookies: Vec<serde_json::Value>,
}

async fn import_profile_cookies(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
    Json(request): Json<ImportCookiesRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_admin(&principal)?;
    let profile_id = normalize_profile_id(Some(profile_id));
    Ok(Json(
        state
            .import_browser_profile_cookies(&profile_id, request.cookies)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
struct BrowserLoginStartRequest {
    url: String,
}

#[derive(Debug, Deserialize)]
struct BrowserLoginClickRequest {
    x: f64,
    y: f64,
    button: Option<String>,
    #[serde(alias = "clickCount")]
    click_count: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct BrowserLoginMoveRequest {
    x: f64,
    y: f64,
}

#[derive(Debug, Deserialize)]
struct BrowserLoginMouseButtonRequest {
    x: f64,
    y: f64,
    button: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BrowserLoginTypeRequest {
    text: String,
}

#[derive(Debug, Deserialize)]
struct BrowserLoginPressRequest {
    key: String,
}

#[derive(Debug, Deserialize)]
struct BrowserLoginNavigateRequest {
    url: String,
}

#[derive(Debug, Deserialize)]
struct BrowserLoginWheelRequest {
    #[serde(alias = "deltaX")]
    delta_x: Option<f64>,
    #[serde(alias = "deltaY")]
    delta_y: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct BrowserLoginResizeRequest {
    width: u32,
    height: u32,
}

async fn start_browser_login_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
    Json(request): Json<BrowserLoginStartRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    let profile_id = normalize_profile_id(Some(profile_id));
    Ok(Json(
        state
            .start_browser_login_session(&profile_id, request.url.trim())
            .await?,
    ))
}

async fn browser_login_session_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state.browser_login_session_snapshot(&session_id).await?,
    ))
}

async fn browser_login_session_click(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<BrowserLoginClickRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state
            .browser_login_session_click(
                &session_id,
                request.x,
                request.y,
                request.button.as_deref(),
                request.click_count,
            )
            .await?,
    ))
}

async fn browser_login_session_move(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<BrowserLoginMoveRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state
            .browser_login_session_move(&session_id, request.x, request.y)
            .await?,
    ))
}

async fn browser_login_session_mouse_down(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<BrowserLoginMouseButtonRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state
            .browser_login_session_mouse_down(
                &session_id,
                request.x,
                request.y,
                request.button.as_deref(),
            )
            .await?,
    ))
}

async fn browser_login_session_mouse_up(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<BrowserLoginMouseButtonRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state
            .browser_login_session_mouse_up(
                &session_id,
                request.x,
                request.y,
                request.button.as_deref(),
            )
            .await?,
    ))
}

async fn browser_login_session_type(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<BrowserLoginTypeRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state
            .browser_login_session_type(&session_id, &request.text)
            .await?,
    ))
}

async fn browser_login_session_insert_text(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<BrowserLoginTypeRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state
            .browser_login_session_insert_text(&session_id, &request.text)
            .await?,
    ))
}

async fn browser_login_session_press(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<BrowserLoginPressRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state
            .browser_login_session_press(&session_id, &request.key)
            .await?,
    ))
}

async fn browser_login_session_navigate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<BrowserLoginNavigateRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state
            .browser_login_session_navigate(&session_id, request.url.trim())
            .await?,
    ))
}

async fn browser_login_session_wheel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<BrowserLoginWheelRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state
            .browser_login_session_wheel(
                &session_id,
                request.delta_x.unwrap_or_default(),
                request.delta_y.unwrap_or_default(),
                request.x,
                request.y,
            )
            .await?,
    ))
}

async fn browser_login_session_resize(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<BrowserLoginResizeRequest>,
) -> Result<Json<reflection_core::browser_probe::LoginSessionSnapshot>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(
        state
            .browser_login_session_resize(&session_id, request.width, request.height)
            .await?,
    ))
}

async fn close_browser_login_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = authorize(&state, &headers).await?;
    ensure_login_profile(&principal)?;
    Ok(Json(state.close_browser_login_session(&session_id).await?))
}

async fn get_media(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, filename)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    if filename.contains('/') || filename.contains('\\') || filename == "." || filename == ".." {
        return Err(RkError::BadRequest("invalid media filename".to_string()).into());
    }
    let path = state.paths.public_job_dir(id).join(&filename);
    let mut file = File::open(path).await?;
    let file_len = file.metadata().await?.len();
    let content_type = content_type_for(&filename);

    if file_len == 0 {
        return Err(RkError::Source("media file is empty".to_string()).into());
    }

    let range = parse_range(headers.get(header::RANGE), file_len)?;

    if let Some(range) = range {
        file.seek(SeekFrom::Start(range.start)).await?;
        let stream = ReaderStream::new(file.take(range.len()));
        let body = Body::from_stream(stream);

        return Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_LENGTH, range.len().to_string())
            .header(
                header::CONTENT_RANGE,
                format!("bytes {}-{}/{}", range.start, range.end, file_len),
            )
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(body)
            .map_err(|error| RkError::Source(format!("failed to build response: {error}")).into());
    }

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, file_len.to_string())
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(body)
        .map_err(|error| RkError::Source(format!("failed to build response: {error}")))?)
}

async fn head_media(
    State(state): State<Arc<AppState>>,
    Path((id, filename)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    if filename.contains('/') || filename.contains('\\') || filename == "." || filename == ".." {
        return Err(RkError::BadRequest("invalid media filename".to_string()).into());
    }
    let path = state.paths.public_job_dir(id).join(&filename);
    let file_len = File::open(path).await?.metadata().await?.len();
    let content_type = content_type_for(&filename);

    if file_len == 0 {
        return Err(RkError::Source("media file is empty".to_string()).into());
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, file_len.to_string())
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::empty())
        .map_err(|error| RkError::Source(format!("failed to build response: {error}")))?)
}

#[derive(Debug, Clone)]
struct AuthPrincipal {
    key_id: Option<Uuid>,
    label: String,
    role: ApiKeyRole,
    max_download_bytes: Option<u64>,
    allow_browser_probe: bool,
    allow_ytdlp: bool,
    allow_external_adapters: bool,
    allow_login_profile: bool,
}

async fn authorize(state: &AppState, headers: &HeaderMap) -> Result<AuthPrincipal, RkError> {
    let provided = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .or_else(|| {
            headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
        });

    if let Some(provided) = provided {
        if let Some(record) = state.find_api_key(provided).await? {
            return Ok(principal_from_record(record));
        }
    }

    Err(RkError::Unauthorized)
}

fn principal_from_record(record: ApiKeyRecord) -> AuthPrincipal {
    AuthPrincipal {
        key_id: Some(record.id),
        label: record.label,
        role: record.role,
        max_download_bytes: record.max_download_bytes,
        allow_browser_probe: record.role == ApiKeyRole::Admin || record.allow_browser_probe,
        allow_ytdlp: record.role == ApiKeyRole::Admin || record.allow_ytdlp,
        allow_external_adapters: record.role == ApiKeyRole::Admin || record.allow_external_adapters,
        allow_login_profile: record.role == ApiKeyRole::Admin || record.allow_login_profile,
    }
}

fn ensure_admin(principal: &AuthPrincipal) -> Result<(), RkError> {
    if principal.role == ApiKeyRole::Admin {
        Ok(())
    } else {
        Err(RkError::Unauthorized)
    }
}

fn ensure_login_profile(principal: &AuthPrincipal) -> Result<(), RkError> {
    if principal.allow_login_profile {
        Ok(())
    } else {
        Err(RkError::Unauthorized)
    }
}

async fn ensure_job_access(
    state: &AppState,
    principal: &AuthPrincipal,
    id: Uuid,
) -> Result<(), RkError> {
    if principal.role == ApiKeyRole::Admin {
        return Ok(());
    }
    let Some(key_id) = principal.key_id else {
        return Err(RkError::Unauthorized);
    };
    if state.job_belongs_to_key(id, key_id).await? {
        Ok(())
    } else {
        Err(RkError::NotFound(format!("job {id}")))
    }
}

fn authorized_discovery(
    requested: DiscoveryMode,
    outputs: &[OutputKind],
    principal: &AuthPrincipal,
) -> Result<DiscoveryMode, RkError> {
    if outputs.contains(&OutputKind::PageHtml) {
        return match requested {
            DiscoveryMode::Browser | DiscoveryMode::Auto | DiscoveryMode::Direct
                if principal.allow_browser_probe =>
            {
                Ok(DiscoveryMode::Browser)
            }
            DiscoveryMode::Browser | DiscoveryMode::Auto | DiscoveryMode::Direct => {
                Err(RkError::Unauthorized)
            }
            DiscoveryMode::External => Err(RkError::BadRequest(
                "page_html requires browser discovery".to_string(),
            )),
        };
    }
    match requested {
        DiscoveryMode::Direct => Ok(DiscoveryMode::Direct),
        DiscoveryMode::External if principal.allow_ytdlp || principal.allow_external_adapters => {
            Ok(DiscoveryMode::External)
        }
        DiscoveryMode::Browser if principal.allow_browser_probe => Ok(DiscoveryMode::Browser),
        DiscoveryMode::Auto
            if principal.allow_browser_probe
                && (principal.allow_ytdlp || principal.allow_external_adapters) =>
        {
            Ok(DiscoveryMode::Auto)
        }
        DiscoveryMode::Auto if principal.allow_browser_probe => Ok(DiscoveryMode::Browser),
        DiscoveryMode::Auto if principal.allow_ytdlp || principal.allow_external_adapters => {
            Ok(DiscoveryMode::External)
        }
        DiscoveryMode::Auto => Ok(DiscoveryMode::Direct),
        DiscoveryMode::External => Err(RkError::Unauthorized),
        DiscoveryMode::Browser => Err(RkError::Unauthorized),
    }
}

fn normalize_auth_mode_for_outputs(
    requested: Option<AuthMode>,
    outputs: &[OutputKind],
) -> AuthMode {
    let auth_mode = requested.unwrap_or(AuthMode::Auto);
    if outputs.contains(&OutputKind::PageHtml) && auth_mode == AuthMode::Auto {
        AuthMode::None
    } else {
        auth_mode
    }
}

fn supported_discovery_modes(principal: &AuthPrincipal) -> Vec<&'static str> {
    let mut modes = vec!["direct"];
    if principal.allow_ytdlp || principal.allow_external_adapters {
        modes.push("external");
    }
    if principal.allow_browser_probe {
        modes.push("browser");
    }
    if principal.allow_ytdlp || principal.allow_external_adapters || principal.allow_browser_probe {
        modes.push("auto");
    }
    modes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_html_auto_auth_defaults_to_none() {
        assert_eq!(
            normalize_auth_mode_for_outputs(None, &[OutputKind::PageHtml]),
            AuthMode::None
        );
        assert_eq!(
            normalize_auth_mode_for_outputs(Some(AuthMode::Auto), &[OutputKind::PageHtml]),
            AuthMode::None
        );
    }

    #[test]
    fn page_html_explicit_auth_is_preserved() {
        assert_eq!(
            normalize_auth_mode_for_outputs(Some(AuthMode::Profile), &[OutputKind::PageHtml]),
            AuthMode::Profile
        );
        assert_eq!(
            normalize_auth_mode_for_outputs(Some(AuthMode::Cookies), &[OutputKind::PageHtml]),
            AuthMode::Cookies
        );
    }

    #[test]
    fn media_auto_auth_stays_auto() {
        assert_eq!(
            normalize_auth_mode_for_outputs(None, &[OutputKind::Video, OutputKind::Audio]),
            AuthMode::Auto
        );
        assert_eq!(
            normalize_auth_mode_for_outputs(
                Some(AuthMode::Auto),
                &[OutputKind::Video, OutputKind::Audio],
            ),
            AuthMode::Auto
        );
    }
}

#[derive(Debug)]
struct ApiError(RkError);

impl From<RkError> for ApiError {
    fn from(value: RkError) -> Self {
        Self(value)
    }
}

impl From<std::io::Error> for ApiError {
    fn from(value: std::io::Error) -> Self {
        Self(RkError::Io(value))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            RkError::BadRequest(_) => StatusCode::BAD_REQUEST,
            RkError::Unauthorized => StatusCode::UNAUTHORIZED,
            RkError::NotFound(_) => StatusCode::NOT_FOUND,
            RkError::UrlPolicy(_) => StatusCode::BAD_REQUEST,
            RkError::UrlParse(_) => StatusCode::BAD_REQUEST,
            RkError::Source(_) => StatusCode::BAD_GATEWAY,
            RkError::Browser(_) => StatusCode::BAD_GATEWAY,
            RkError::DownloadTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            RkError::RangeNotSatisfiable { .. } => StatusCode::RANGE_NOT_SATISFIABLE,
            RkError::Transcode(_) => StatusCode::UNPROCESSABLE_ENTITY,
            RkError::Io(_) | RkError::Http(_) | RkError::Json(_) | RkError::Database(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        let body = Json(serde_json::json!({
            "error": self.0.to_string()
        }));

        if let RkError::RangeNotSatisfiable { file_len } = self.0 {
            return (
                status,
                [(header::CONTENT_RANGE, format!("bytes */{file_len}"))],
                body,
            )
                .into_response();
        }

        (status, body).into_response()
    }
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

fn infer_platform(source_url: &str) -> PlatformHint {
    let Ok(url) = url::Url::parse(source_url) else {
        return PlatformHint::Auto;
    };
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host.contains("bilibili.com") || host.contains("b23.tv") {
        PlatformHint::Bilibili
    } else if host.contains("youtube.com") || host.contains("youtu.be") {
        PlatformHint::Youtube
    } else if host.contains("soundcloud.com") {
        PlatformHint::Soundcloud
    } else if host.contains("douyin.com") || host.contains("iesdouyin.com") {
        PlatformHint::Douyin
    } else if host.contains("kuaishou.com")
        || host.contains("v.kuaishou.com")
        || host.contains("gifshow.com")
    {
        PlatformHint::Kuaishou
    } else if host.contains("pornhub.") || host.contains("phncdn.com") {
        PlatformHint::Pornhub
    } else if host.contains("acfun.cn") {
        PlatformHint::Acfun
    } else if host.contains("iqiyi.com") || host.contains("qiyi.com") {
        PlatformHint::Iqiyi
    } else if host.contains("youku.com") {
        PlatformHint::Youku
    } else if host.contains("tiktok.com") || host.contains("tiktokcdn.com") {
        PlatformHint::Tiktok
    } else if host.contains("vimeo.com") || host.contains("vimeocdn.com") {
        PlatformHint::Vimeo
    } else if host.contains("m3u8") || source_url.contains(".m3u8") || source_url.contains(".mpd") {
        PlatformHint::Live
    } else {
        PlatformHint::Auto
    }
}

fn content_type_for(filename: &str) -> &'static str {
    let filename = filename.to_ascii_lowercase();
    if filename.ends_with(".mp3") {
        "audio/mpeg"
    } else if filename.ends_with(".mp4") {
        "video/mp4"
    } else if filename.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if filename.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
        "image/jpeg"
    } else if filename.ends_with(".png") {
        "image/png"
    } else if filename.ends_with(".webp") {
        "image/webp"
    } else if filename.ends_with(".gif") {
        "image/gif"
    } else if filename.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if filename.ends_with(".txt") {
        "text/plain; charset=utf-8"
    } else if filename.ends_with(".json") {
        "application/json"
    } else if filename.ends_with(".zip") {
        "application/zip"
    } else if filename.ends_with(".har") {
        "application/json"
    } else if filename.ends_with(".mhtml") || filename.ends_with(".mht") {
        "multipart/related"
    } else if filename.ends_with(".warc") {
        "application/warc"
    } else if filename.ends_with(".svg") {
        "image/svg+xml"
    } else if filename.ends_with(".ico") {
        "image/x-icon"
    } else if filename.ends_with(".avif") {
        "image/avif"
    } else if filename.ends_with(".webmanifest") {
        "application/manifest+json"
    } else if filename.ends_with(".woff2") {
        "font/woff2"
    } else if filename.ends_with(".woff") {
        "font/woff"
    } else if filename.ends_with(".ttf") {
        "font/ttf"
    } else if filename.ends_with(".otf") {
        "font/otf"
    } else if filename.ends_with(".wasm") {
        "application/wasm"
    } else {
        "application/octet-stream"
    }
}

#[derive(Debug, Clone, Copy)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    fn len(self) -> u64 {
        self.end - self.start + 1
    }
}

fn parse_range(value: Option<&HeaderValue>, file_len: u64) -> Result<Option<ByteRange>, RkError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| RkError::BadRequest("invalid Range header".to_string()))?;
    let Some(spec) = value.strip_prefix("bytes=") else {
        return Err(RkError::BadRequest("unsupported Range unit".to_string()));
    };
    if spec.contains(',') {
        return Err(RkError::BadRequest(
            "multiple byte ranges are not supported".to_string(),
        ));
    }

    let Some((start, end)) = spec.split_once('-') else {
        return Err(RkError::BadRequest("invalid byte range".to_string()));
    };

    if start.is_empty() {
        let suffix_len = end
            .parse::<u64>()
            .map_err(|_| RkError::BadRequest("invalid byte range suffix".to_string()))?;
        if suffix_len == 0 {
            return Err(RkError::BadRequest("invalid byte range suffix".to_string()));
        }
        let start = file_len.saturating_sub(suffix_len);
        return Ok(Some(ByteRange {
            start,
            end: file_len - 1,
        }));
    }

    let start = start
        .parse::<u64>()
        .map_err(|_| RkError::BadRequest("invalid byte range start".to_string()))?;
    if start >= file_len {
        return Err(RkError::RangeNotSatisfiable { file_len });
    }
    let end = if end.is_empty() {
        file_len - 1
    } else {
        end.parse::<u64>()
            .map_err(|_| RkError::BadRequest("invalid byte range end".to_string()))?
            .min(file_len - 1)
    };

    if end < start {
        return Err(RkError::RangeNotSatisfiable { file_len });
    }

    Ok(Some(ByteRange { start, end }))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            error!("failed to install Ctrl+C handler: {error}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => error!("failed to install signal handler: {error}"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

fn init_tracing() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| {
        "reflection_api=debug,reflection_core=debug,tower_http=info".to_string()
    });

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
