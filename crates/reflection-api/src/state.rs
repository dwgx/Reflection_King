use std::{
    collections::{HashMap, HashSet},
    io::{Seek, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use base64::Engine;
use reflection_core::{
    browser_probe::{BrowserCookie, BrowserProbeClient, LoginSessionSnapshot, PageSnapshot},
    download::Downloader,
    external_probe::YtDlpProbe,
    external_tools::{ExternalToolKind, ExternalToolProbe},
    extractors::{ExtractContext, SourceResolver},
    job_store::JobStore,
    models::{
        ApiKeyRecord, ApiKeyView, ArtifactView, CandidateKind, CandidateProtection,
        CandidateValidationState, ClearJobsResponse, CreateUserKeyRequest, CreatedUserKeyResponse,
        DiscoveryMode, HiddenJobBatchView, JobRecord, JobStatus, JobView, MediaCandidate,
        OutputKind, RestoreJobsResponse, RotatedAdminKeyResponse, RuntimeSettingsView,
        UpdateRuntimeSettingsRequest,
    },
    observability::{ErrorClass, JobTrace, PipelineEvent, PipelineEventType},
    paths::StoragePaths,
    transcode::{concat_demuxer_file, Transcoder},
    AppConfig, Result, RkError,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use time::OffsetDateTime;
use tokio::{
    process::Command,
    sync::{mpsc, Mutex, Semaphore},
    time as tokio_time,
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Map a backend error to its observability classification so retry decisions
/// and the `error_class` columns stay consistent.
pub fn classify_error(error: &RkError) -> ErrorClass {
    match error {
        RkError::UrlPolicy(_) => ErrorClass::Blocked,
        RkError::DownloadTooLarge { .. } => ErrorClass::TooLarge,
        RkError::Browser(_) => ErrorClass::Blocked,
        RkError::Transcode(_) => ErrorClass::Parse,
        RkError::Json(_) => ErrorClass::Parse,
        RkError::Http(http_error) => {
            if http_error.is_timeout() {
                ErrorClass::Timeout
            } else if http_error.is_connect() {
                ErrorClass::Dns
            } else if let Some(status) = http_error.status() {
                if status.is_server_error() {
                    ErrorClass::Http5xx
                } else if status.is_client_error() {
                    ErrorClass::Http4xx
                } else {
                    ErrorClass::Internal
                }
            } else {
                ErrorClass::Internal
            }
        }
        RkError::Source(message) => {
            let lowered = message.to_ascii_lowercase();
            if lowered.contains("timed out") {
                ErrorClass::Timeout
            } else if lowered.contains("drm") {
                ErrorClass::DrmBlocked
            } else if lowered.contains("too large") {
                ErrorClass::TooLarge
            } else if lowered.contains("did not find") || lowered.contains("no ") {
                ErrorClass::Blocked
            } else {
                ErrorClass::Internal
            }
        }
        _ => ErrorClass::Internal,
    }
}

pub struct AppState {
    pub config: AppConfig,
    pub paths: StoragePaths,
    job_store: JobStore,
    browser_probe: Option<BrowserProbeClient>,
    yt_dlp_probe: Option<YtDlpProbe>,
    external_tool_probes: Vec<ExternalToolProbe>,
    queue_tx: mpsc::Sender<Uuid>,
    queue_rx: Mutex<mpsc::Receiver<Uuid>>,
    worker_slots: Arc<Semaphore>,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self> {
        let paths = StoragePaths::new(config.storage_dir.clone());
        paths.ensure().await?;
        let job_store = JobStore::connect(&paths.database_path()).await?;
        if let Some(api_key) = config.api_key.as_deref() {
            job_store.ensure_admin_key_from_secret(api_key).await?;
        }
        let browser_probe = config
            .browser_probe_url
            .as_ref()
            .map(|url| {
                BrowserProbeClient::new(
                    url,
                    config.browser_probe_timeout,
                    config.browser_internal_token.clone(),
                )
            })
            .transpose()?;
        let yt_dlp_probe = config
            .yt_dlp_path
            .clone()
            .map(|path| YtDlpProbe::new(path, config.yt_dlp_timeout, config.yt_dlp_max_json_bytes));
        let mut external_tool_probes = Vec::new();
        if let Some(path) = config.you_get_path.clone() {
            external_tool_probes.push(ExternalToolProbe::new(
                ExternalToolKind::YouGet,
                path,
                config.external_probe_timeout,
                config.yt_dlp_max_json_bytes,
            ));
        }
        if let Some(path) = config.lux_path.clone() {
            external_tool_probes.push(ExternalToolProbe::new(
                ExternalToolKind::Lux,
                path,
                config.external_probe_timeout,
                config.yt_dlp_max_json_bytes,
            ));
        }
        if let Some(path) = config.streamlink_path.clone() {
            external_tool_probes.push(ExternalToolProbe::new(
                ExternalToolKind::Streamlink,
                path,
                config.external_probe_timeout,
                config.yt_dlp_max_json_bytes,
            ));
        }

        let pending_jobs = job_store.recover_pending().await?;
        let queue_capacity = pending_jobs.len().max(512);
        let (queue_tx, queue_rx) = mpsc::channel(queue_capacity);
        let worker_slots = Arc::new(Semaphore::new(config.max_concurrent_jobs));

        let state = Self {
            config,
            paths,
            job_store,
            browser_probe,
            yt_dlp_probe,
            external_tool_probes,
            queue_tx,
            queue_rx: Mutex::new(queue_rx),
            worker_slots,
        };

        for job_id in pending_jobs {
            state.enqueue(job_id).await?;
        }

        Ok(state)
    }

    pub fn spawn_workers(self: &Arc<Self>) {
        let state = self.clone();
        tokio::spawn(async move {
            state.worker_loop().await;
        });
        let state = self.clone();
        tokio::spawn(async move {
            state.retention_loop().await;
        });
    }

    async fn runtime_settings(&self) -> Result<EffectiveRuntimeSettings> {
        let values = self.job_store.runtime_setting_values().await?;
        Ok(EffectiveRuntimeSettings::from_config(&self.config, &values))
    }

    pub async fn runtime_settings_view(&self) -> Result<RuntimeSettingsView> {
        Ok(self.runtime_settings().await?.to_view(&self.config))
    }

    pub async fn update_runtime_settings(
        &self,
        request: UpdateRuntimeSettingsRequest,
    ) -> Result<RuntimeSettingsView> {
        self.job_store.update_runtime_settings(request).await?;
        self.runtime_settings_view().await
    }

    async fn effective_limits_for_job(&self, job: &JobRecord) -> Result<EffectiveRuntimeSettings> {
        let mut settings = self.runtime_settings().await?;
        if let Some(key_id) = job.requester_key_id {
            if let Some(record) = self.job_store.get_api_key(key_id).await? {
                if let Some(limit) = record.max_download_bytes {
                    settings.max_download_bytes = settings.max_download_bytes.min(limit);
                }
            }
        }
        Ok(settings)
    }

    fn transcoder(&self, settings: &EffectiveRuntimeSettings) -> Transcoder {
        Transcoder::with_timeout(self.config.ffmpeg_path.clone(), settings.download_timeout)
    }

    fn yt_dlp_probe_for_settings(&self, settings: &EffectiveRuntimeSettings) -> Option<YtDlpProbe> {
        self.config.yt_dlp_path.clone().map(|path| {
            YtDlpProbe::new(
                path,
                settings.yt_dlp_timeout,
                settings.yt_dlp_max_json_bytes,
            )
        })
    }

    fn external_tool_probes_for_settings(
        &self,
        settings: &EffectiveRuntimeSettings,
    ) -> Vec<ExternalToolProbe> {
        let mut probes = Vec::new();
        if let Some(path) = self.config.you_get_path.clone() {
            probes.push(ExternalToolProbe::new(
                ExternalToolKind::YouGet,
                path,
                self.config.external_probe_timeout,
                settings.yt_dlp_max_json_bytes,
            ));
        }
        if let Some(path) = self.config.lux_path.clone() {
            probes.push(ExternalToolProbe::new(
                ExternalToolKind::Lux,
                path,
                self.config.external_probe_timeout,
                settings.yt_dlp_max_json_bytes,
            ));
        }
        if let Some(path) = self.config.streamlink_path.clone() {
            probes.push(ExternalToolProbe::new(
                ExternalToolKind::Streamlink,
                path,
                self.config.external_probe_timeout,
                settings.yt_dlp_max_json_bytes,
            ));
        }
        probes
    }

    pub async fn insert_and_enqueue(&self, record: JobRecord) -> Result<JobView> {
        let id = record.id;
        let view = JobView::from(record.clone());
        self.job_store.insert(&record).await?;
        self.enqueue(id).await?;
        Ok(view)
    }

    pub async fn get_job(&self, id: Uuid) -> Result<Option<JobRecord>> {
        self.job_store.get(id).await
    }

    pub async fn list_jobs(&self, limit: usize) -> Result<Vec<JobView>> {
        self.job_store
            .list_recent(limit)
            .await
            .map(|jobs| jobs.into_iter().map(JobView::from).collect())
    }

    pub async fn list_jobs_for_key(
        &self,
        requester_key_id: Uuid,
        limit: usize,
    ) -> Result<Vec<JobView>> {
        self.job_store
            .list_recent_for_key(requester_key_id, limit)
            .await
            .map(|jobs| jobs.into_iter().map(JobView::from).collect())
    }

    pub async fn hide_visible_jobs(
        &self,
        actor_key_id: Option<Uuid>,
        actor_label: Option<&str>,
    ) -> Result<ClearJobsResponse> {
        self.job_store
            .hide_visible_jobs(actor_key_id, actor_label)
            .await
    }

    pub async fn hide_visible_jobs_for_key(
        &self,
        requester_key_id: Uuid,
        actor_label: Option<&str>,
    ) -> Result<ClearJobsResponse> {
        self.job_store
            .hide_visible_jobs_for_key(requester_key_id, actor_label)
            .await
    }

    pub async fn list_hidden_job_batches(
        &self,
        actor_key_id: Option<Uuid>,
        limit: usize,
    ) -> Result<Vec<HiddenJobBatchView>> {
        self.job_store
            .list_hidden_job_batches(actor_key_id, limit)
            .await
    }

    pub async fn restore_latest_hidden_job_batch(
        &self,
        actor_key_id: Option<Uuid>,
    ) -> Result<RestoreJobsResponse> {
        self.job_store
            .restore_latest_hidden_job_batch(actor_key_id)
            .await
    }

    pub async fn restore_hidden_job_batch(
        &self,
        actor_key_id: Option<Uuid>,
        batch_id: Uuid,
    ) -> Result<RestoreJobsResponse> {
        self.job_store
            .restore_hidden_job_batch(actor_key_id, batch_id)
            .await
    }

    pub async fn job_belongs_to_key(&self, id: Uuid, requester_key_id: Uuid) -> Result<bool> {
        self.job_store
            .job_belongs_to_key(id, requester_key_id)
            .await
    }

    pub async fn find_api_key(&self, key: &str) -> Result<Option<ApiKeyRecord>> {
        self.job_store.find_api_key(key).await
    }

    pub async fn list_api_keys(&self) -> Result<Vec<ApiKeyView>> {
        self.job_store.list_api_keys().await
    }

    pub async fn create_user_key(
        &self,
        request: CreateUserKeyRequest,
    ) -> Result<CreatedUserKeyResponse> {
        self.job_store.create_user_key(request).await
    }

    pub async fn rotate_admin_key(
        &self,
        custom_key: Option<&str>,
    ) -> Result<RotatedAdminKeyResponse> {
        self.job_store.rotate_admin_key(custom_key).await
    }

    pub async fn revoke_api_key(&self, id: Uuid) -> Result<bool> {
        self.job_store.revoke_api_key(id).await
    }

    pub async fn list_candidates(&self, id: Uuid) -> Result<Vec<MediaCandidate>> {
        self.job_store.list_candidates(id).await
    }

    pub fn browser_probe_configured(&self) -> bool {
        self.browser_probe.is_some()
    }

    pub fn yt_dlp_configured(&self) -> bool {
        self.yt_dlp_probe.is_some()
    }

    pub fn external_tools_configured(&self) -> bool {
        !self.external_tool_probes.is_empty()
    }

    pub fn configured_external_tool_names(&self) -> Vec<&'static str> {
        self.external_tool_probes
            .iter()
            .map(|probe| probe.kind().as_str())
            .collect()
    }

    pub async fn import_browser_profile_cookies(
        &self,
        profile_id: &str,
        cookies: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser profile management".to_string(),
            ));
        };
        browser_probe.import_cookies(profile_id, cookies).await
    }

    pub async fn start_browser_login_session(
        &self,
        profile_id: &str,
        url: &str,
    ) -> Result<LoginSessionSnapshot> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser profile management".to_string(),
            ));
        };
        browser_probe.start_login_session(profile_id, url).await
    }

    pub async fn start_job_browser_login_session(
        &self,
        job: &JobRecord,
        requester_key_id: Option<Uuid>,
    ) -> Result<LoginSessionSnapshot> {
        let profile_id = job_scoped_profile_id(job.id, requester_key_id);
        self.job_store
            .attach_profile_for_job(job.id, &profile_id)
            .await?;
        self.start_browser_login_session(&profile_id, &job.source_url)
            .await
    }

    pub async fn resume_job_with_profile(&self, job_id: Uuid) -> Result<JobView> {
        let job = self
            .job_store
            .get(job_id)
            .await?
            .ok_or_else(|| RkError::NotFound(format!("job {job_id}")))?;
        let profile_id = job.profile_id.clone();
        self.job_store
            .reset_for_profile_resume(job_id, &profile_id)
            .await?;
        self.record_event(PipelineEvent::new(
            job_id,
            "profile_resume",
            "api",
            PipelineEventType::Note,
            serde_json::json!({ "profile_id": profile_id }),
        ))
        .await;
        self.enqueue(job_id).await?;
        self.get_job(job_id)
            .await?
            .map(JobView::from)
            .ok_or_else(|| RkError::NotFound(format!("job {job_id}")))
    }

    pub async fn browser_login_session_snapshot(
        &self,
        session_id: &str,
    ) -> Result<LoginSessionSnapshot> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser profile management".to_string(),
            ));
        };
        browser_probe.login_session_snapshot(session_id).await
    }

    pub async fn browser_login_session_click(
        &self,
        session_id: &str,
        x: f64,
        y: f64,
        button: Option<&str>,
        click_count: Option<u8>,
    ) -> Result<LoginSessionSnapshot> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser profile management".to_string(),
            ));
        };
        browser_probe
            .login_session_click(session_id, x, y, button, click_count)
            .await
    }

    pub async fn browser_login_session_type(
        &self,
        session_id: &str,
        text: &str,
    ) -> Result<LoginSessionSnapshot> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser profile management".to_string(),
            ));
        };
        browser_probe.login_session_type(session_id, text).await
    }

    pub async fn browser_login_session_press(
        &self,
        session_id: &str,
        key: &str,
    ) -> Result<LoginSessionSnapshot> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser profile management".to_string(),
            ));
        };
        browser_probe.login_session_press(session_id, key).await
    }

    pub async fn browser_login_session_navigate(
        &self,
        session_id: &str,
        url: &str,
    ) -> Result<LoginSessionSnapshot> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser profile management".to_string(),
            ));
        };
        browser_probe.login_session_navigate(session_id, url).await
    }

    pub async fn browser_login_session_wheel(
        &self,
        session_id: &str,
        delta_x: f64,
        delta_y: f64,
        x: Option<f64>,
        y: Option<f64>,
    ) -> Result<LoginSessionSnapshot> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser profile management".to_string(),
            ));
        };
        browser_probe
            .login_session_wheel(session_id, delta_x, delta_y, x, y)
            .await
    }

    pub async fn browser_login_session_resize(
        &self,
        session_id: &str,
        width: u32,
        height: u32,
    ) -> Result<LoginSessionSnapshot> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser profile management".to_string(),
            ));
        };
        browser_probe
            .login_session_resize(session_id, width, height)
            .await
    }

    pub async fn close_browser_login_session(&self, session_id: &str) -> Result<serde_json::Value> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser profile management".to_string(),
            ));
        };
        browser_probe.close_login_session(session_id).await
    }

    pub async fn list_artifacts(&self, id: Uuid) -> Result<Vec<ArtifactView>> {
        self.job_store.list_artifacts(id).await
    }

    pub async fn select_candidates(
        &self,
        job_id: Uuid,
        candidate_ids: Vec<Uuid>,
    ) -> Result<JobView> {
        if candidate_ids.is_empty() {
            return Err(RkError::BadRequest(
                "select at least one candidate".to_string(),
            ));
        }

        for candidate_id in &candidate_ids {
            let candidate = self
                .job_store
                .get_candidate(job_id, *candidate_id)
                .await?
                .ok_or_else(|| RkError::NotFound(format!("candidate {candidate_id}")))?;
            if let Some(reason) = candidate_not_selectable_reason(&candidate) {
                return Err(RkError::BadRequest(format!(
                    "candidate {candidate_id} is not selectable: {reason}"
                )));
            }
        }

        self.job_store
            .set_selected_candidates(job_id, &candidate_ids)
            .await?;
        for candidate_id in &candidate_ids {
            self.job_store
                .set_candidate_selection(*candidate_id, true, Some("selected via API"))
                .await
                .ok();
        }
        self.record_event(PipelineEvent::new(
            job_id,
            "candidate_selected",
            "api",
            PipelineEventType::CandidateSelected,
            serde_json::json!({ "candidate_ids": candidate_ids }),
        ))
        .await;
        self.update_status(job_id, JobStatus::CandidateSelected)
            .await?;
        self.enqueue(job_id).await?;
        self.get_job(job_id)
            .await?
            .map(JobView::from)
            .ok_or_else(|| RkError::NotFound(format!("job {job_id}")))
    }

    async fn worker_loop(self: Arc<Self>) {
        loop {
            let next_job = {
                let mut rx = self.queue_rx.lock().await;
                rx.recv().await
            };

            let Some(job_id) = next_job else {
                info!("job queue closed");
                break;
            };

            let permit = match self.worker_slots.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(error) => {
                    error!("failed to acquire worker slot: {error}");
                    continue;
                }
            };

            let state = self.clone();
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(error) = state.process_job(job_id).await {
                    error!(%job_id, "job failed: {error}");
                    let class = classify_error(&error);
                    if is_profile_required_error(&error) {
                        state
                            .mark_needs_profile(job_id, error.to_string(), class)
                            .await;
                    } else {
                        state.mark_error(job_id, error.to_string(), class).await;
                    }
                }
            });
        }
    }

    async fn retention_loop(self: Arc<Self>) {
        loop {
            if let Err(error) = self.prune_expired_jobs().await {
                warn!("job retention cleanup failed: {error}");
            }
            tokio_time::sleep(Duration::from_secs(3600)).await;
        }
    }

    async fn prune_expired_jobs(&self) -> Result<()> {
        let settings = self.runtime_settings().await?;
        let ids = self
            .job_store
            .expired_job_ids(settings.job_ttl_hours)
            .await?;
        for id in &ids {
            tokio::fs::remove_dir_all(self.paths.public_job_dir(*id))
                .await
                .ok();
            tokio::fs::remove_dir_all(self.paths.tmp_dir().join(id.to_string()))
                .await
                .ok();
        }
        let hidden = self.job_store.hide_expired_jobs(&ids).await?;
        if hidden > 0 {
            info!(hidden, "expired jobs pruned by retention worker");
        }
        Ok(())
    }

    async fn process_job(&self, job_id: Uuid) -> Result<()> {
        debug!(%job_id, "starting job");
        self.job_store.mark_started(job_id).await.ok();
        let attempt = self.job_store.increment_attempt(job_id).await.unwrap_or(1);
        self.record_event(PipelineEvent::new(
            job_id,
            "dispatch",
            "worker",
            PipelineEventType::Note,
            serde_json::json!({ "attempt": attempt }),
        ))
        .await;

        let job = self
            .get_job(job_id)
            .await
            .and_then(|job| job.ok_or_else(|| RkError::NotFound(format!("job {job_id}"))))?;

        if job.status == JobStatus::CandidateSelected {
            return self.process_selected_candidates(job).await;
        }

        self.resolve_candidates(job.clone()).await?;
        if job.discovery == DiscoveryMode::Direct {
            self.auto_select_direct_candidate(&job).await?;
        }
        Ok(())
    }

    /// Unified candidate discovery: build the extractor chain for the job's
    /// discovery mode, run it, and persist every attempt, browser session, and
    /// candidate. The first extractor that yields candidates wins.
    async fn resolve_candidates(&self, job: JobRecord) -> Result<()> {
        // Explicit modes require their backing service.
        match job.discovery {
            DiscoveryMode::External
                if self.yt_dlp_probe.is_none() && self.external_tool_probes.is_empty() =>
            {
                return Err(RkError::Source(
                    "configure RK_YTDLP_PATH, RK_YOU_GET_PATH, RK_LUX_PATH, or RK_STREAMLINK_PATH for external discovery".to_string(),
                ));
            }
            DiscoveryMode::Browser if self.browser_probe.is_none() => {
                return Err(RkError::Browser(
                    "RK_BROWSER_PROBE_URL is required for browser discovery".to_string(),
                ));
            }
            _ => {}
        }

        self.update_status(job.id, JobStatus::Resolving).await?;
        let settings = self.effective_limits_for_job(&job).await?;

        let url = reflection_core::url_policy::parse_and_validate_url(&job.source_url)?;
        let ctx = ExtractContext {
            job_id: job.id,
            source_url: job.source_url.clone(),
            url,
            outputs: job.outputs.clone(),
            profile_id: job.profile_id.clone(),
            discovery: job.discovery,
            platform_hint: job.platform_hint,
            auth_mode: job.auth_mode,
            yt_dlp: self.yt_dlp_probe_for_settings(&settings),
            external_tools: self.external_tool_probes_for_settings(&settings),
            browser: self.browser_probe.clone(),
        };

        let resolver = SourceResolver::for_discovery(job.discovery);
        let outcome = resolver.resolve(&ctx).await;

        // Persist browser sessions and record each extractor attempt.
        for session in &outcome.browser_sessions {
            self.job_store.record_browser_session(session).await.ok();
        }
        for attempt in &outcome.attempts {
            self.record_event(PipelineEvent::new(
                job.id,
                "resolving",
                attempt.extractor.clone(),
                PipelineEventType::ExtractorAttempt,
                serde_json::json!({
                    "candidate_count": attempt.candidate_count,
                    "error": attempt.error,
                    "error_class": attempt.error_class.as_str(),
                    "duration_ms": attempt.duration_ms,
                    "warnings": attempt.warnings,
                }),
            ))
            .await;
        }

        let snapshot_count = outcome.page_snapshots.len();
        if job.outputs.contains(&OutputKind::PageHtml) {
            for snapshot in &outcome.page_snapshots {
                self.persist_page_snapshot(&job, snapshot, &settings)
                    .await?;
            }
        }

        if outcome.candidates.is_empty() {
            if job.outputs.contains(&OutputKind::PageHtml) && snapshot_count > 0 {
                let artifacts = self.job_store.list_artifacts(job.id).await?;
                let media_url = artifacts
                    .iter()
                    .find(|artifact| artifact.media_url.ends_with("/archive.zip"))
                    .or_else(|| artifacts.first())
                    .map(|artifact| artifact.media_url.clone())
                    .unwrap_or_else(|| {
                        format!("{}/api/jobs/{}/artifacts", settings.public_base_url, job.id)
                    });
                self.mark_ready(job.id, media_url).await?;
                return Ok(());
            }
            let detail = if outcome.chain.is_empty() {
                "no extractor matched this source".to_string()
            } else if let Some(errors) = resolver_error_summary(&outcome.attempts) {
                format!(
                    "no media candidates from chain [{}]: {errors}",
                    outcome.chain.join(", ")
                )
            } else {
                format!(
                    "no media candidates from chain [{}]",
                    outcome.chain.join(", ")
                )
            };
            return Err(RkError::Source(detail));
        }

        let chain_label = outcome.chain_label();
        self.job_store
            .set_resolved_extractor(job.id, &chain_label)
            .await
            .ok();
        let winner = outcome.winner.clone().unwrap_or(chain_label);
        if !outcome.candidates.is_empty() {
            self.record_candidate_summary(job.id, &winner, &outcome.candidates)
                .await;
            self.job_store
                .replace_candidates(job.id, &outcome.candidates)
                .await?;
            self.update_status(job.id, JobStatus::CandidatesReady)
                .await?;
        }
        Ok(())
    }

    async fn auto_select_direct_candidate(&self, job: &JobRecord) -> Result<()> {
        let job_id = job.id;
        let candidates = self.job_store.list_candidates(job_id).await?;
        let candidate = candidates
            .iter()
            .filter(|candidate| candidate_not_selectable_reason(candidate).is_none())
            .max_by_key(|candidate| candidate_attempt_rank(job, candidate))
            .ok_or_else(|| {
                RkError::Source("direct URL did not produce a reusable media candidate".to_string())
            })?;
        self.job_store
            .set_selected_candidates(job_id, &[candidate.id])
            .await?;
        self.job_store
            .set_candidate_selection(candidate.id, true, Some("auto-selected direct candidate"))
            .await
            .ok();
        self.record_event(PipelineEvent::new(
            job_id,
            "candidate_selected",
            "direct",
            PipelineEventType::CandidateSelected,
            serde_json::json!({
                "candidate_ids": vec![candidate.id],
                "reason": "auto-selected direct candidate",
            }),
        ))
        .await;
        self.update_status(job_id, JobStatus::CandidateSelected)
            .await?;
        self.enqueue(job_id).await
    }

    /// Emit a `candidate_found` event summarizing what an extractor surfaced
    /// (auditability: who extracted, which links, how scored).
    async fn record_candidate_summary(
        &self,
        job_id: Uuid,
        extractor: &str,
        candidates: &[MediaCandidate],
    ) {
        let top = candidates
            .iter()
            .take(10)
            .map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "kind": c.kind.as_str(),
                    "url": c.url,
                    "score": c.score,
                    "quality": c.quality_label,
                    "score_breakdown": c.score_breakdown_json,
                })
            })
            .collect::<Vec<_>>();
        self.record_event(PipelineEvent::new(
            job_id,
            "candidates_ready",
            extractor,
            PipelineEventType::CandidateFound,
            serde_json::json!({ "count": candidates.len(), "top": top }),
        ))
        .await;
    }

    async fn process_selected_candidates(&self, job: JobRecord) -> Result<()> {
        let candidates = self.job_store.list_candidates(job.id).await?;
        if candidates.is_empty() {
            return Err(RkError::Source(
                "job has no selected candidates to process".to_string(),
            ));
        }

        let selected = candidates
            .iter()
            .filter(|candidate| job.selected_candidate_ids.contains(&candidate.id))
            .cloned()
            .collect::<Vec<_>>();

        if selected.is_empty() {
            return Err(RkError::Source(
                "no candidates match requested outputs".to_string(),
            ));
        }
        if let Some((candidate, reason)) = selected.iter().find_map(|candidate| {
            candidate_not_selectable_reason(candidate).map(|reason| (candidate, reason))
        }) {
            return Err(RkError::Source(format!(
                "selected candidate {} is not reusable: {reason}",
                candidate.id
            )));
        }

        let settings = self.effective_limits_for_job(&job).await?;
        if should_build_image_slideshow(&job, &selected) {
            self.process_image_slideshow(&job, &selected, &settings)
                .await?;
        } else {
            let attempts = candidate_attempt_order(&job, &selected, &candidates);
            let mut failures = Vec::new();
            let mut success_count = 0usize;

            for candidate in attempts {
                match self
                    .process_candidate(&job, candidate, &candidates, &settings)
                    .await
                {
                    Ok(()) => {
                        success_count += 1;
                        self.job_store
                            .set_candidate_validation_status(candidate.id, "ok")
                            .await
                            .ok();
                        if job.outputs.contains(&OutputKind::Audio)
                            || job.outputs.contains(&OutputKind::Video)
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        let message = friendly_candidate_failure(&error);
                        failures.push(format!(
                            "{} {}: {}",
                            candidate.kind.as_str(),
                            candidate.quality_label.as_deref().unwrap_or("-"),
                            message
                        ));
                        self.job_store
                            .set_candidate_validation_status(
                                candidate.id,
                                &format!("failed: {message}"),
                            )
                            .await
                            .ok();
                        self.record_event(PipelineEvent::new(
                            job.id,
                            "candidate_failed",
                            candidate.extractor.clone(),
                            PipelineEventType::Error,
                            serde_json::json!({
                                "candidate_id": candidate.id,
                                "url": candidate.url,
                                "kind": candidate.kind.as_str(),
                                "quality": candidate.quality_label,
                                "error": error.to_string(),
                            }),
                        ))
                        .await;
                    }
                }
            }

            if success_count == 0 {
                return Err(RkError::Source(format!(
                    "all selected candidates failed: {}",
                    failures.join(" | ")
                )));
            }
        }

        let artifacts = self.job_store.list_artifacts(job.id).await?;
        let media_url = artifacts
            .first()
            .map(|artifact| artifact.media_url.clone())
            .unwrap_or_else(|| {
                format!("{}/api/jobs/{}/artifacts", settings.public_base_url, job.id)
            });
        self.mark_ready(job.id, media_url).await?;
        Ok(())
    }

    async fn process_image_slideshow(
        &self,
        job: &JobRecord,
        candidates: &[MediaCandidate],
        settings: &EffectiveRuntimeSettings,
    ) -> Result<()> {
        let mut image_candidates = candidates
            .iter()
            .filter(|candidate| candidate.kind == CandidateKind::Image)
            .collect::<Vec<_>>();
        image_candidates.sort_by_key(|candidate| image_candidate_order(candidate));
        if image_candidates.is_empty() {
            return Err(RkError::Source(
                "image slideshow requires at least one image candidate".to_string(),
            ));
        }

        let job_dir = self.paths.public_job_dir(job.id);
        tokio::fs::create_dir_all(&job_dir).await?;
        let temp_dir = self.paths.tmp_dir().join(job.id.to_string());
        tokio::fs::create_dir_all(&temp_dir).await?;

        self.update_status(job.id, JobStatus::Downloading).await?;
        let downloader = Downloader::new(settings.download_timeout, settings.max_download_bytes)?;
        let mut image_paths = Vec::with_capacity(image_candidates.len());
        for (index, candidate) in image_candidates.iter().enumerate() {
            validate_candidate_url(&candidate.url)?;
            let input_path = temp_dir.join(format!(
                "slide-{index:04}.{}",
                image_extension(candidate.content_type.as_deref(), &candidate.url)
            ));
            let headers = self.download_headers(job, candidate).await?;
            downloader
                .download_to_file_with_headers(&candidate.url, &input_path, headers)
                .await?;
            image_paths.push(input_path);
        }

        self.update_status(job.id, JobStatus::Remuxing).await?;
        let list_path = temp_dir.join("slideshow.ffconcat");
        let concat = concat_demuxer_file(&image_paths, 2.75)?;
        tokio::fs::write(&list_path, concat).await?;
        let output_path = job_dir.join("slideshow.mp4");
        self.transcoder(settings)
            .images_to_mp4(&list_path, &output_path, 1920, 1080)
            .await?;
        self.insert_artifact(
            job.id,
            OutputKind::Video,
            output_path,
            "video/mp4",
            settings,
        )
        .await?;

        tokio::fs::remove_dir_all(&temp_dir).await.ok();
        Ok(())
    }

    async fn process_candidate(
        &self,
        job: &JobRecord,
        candidate: &MediaCandidate,
        available_candidates: &[MediaCandidate],
        settings: &EffectiveRuntimeSettings,
    ) -> Result<()> {
        if !is_yt_dlp_inline_manifest_candidate(candidate) {
            validate_candidate_url(&candidate.url)?;
        }
        if candidate.extractor == "yt_dlp"
            && candidate.requires_authorization
            && !is_yt_dlp_inline_manifest_candidate(candidate)
        {
            return Err(RkError::Source(
                "yt-dlp candidate requires headers that are not persisted".to_string(),
            ));
        }
        let job_dir = self.paths.public_job_dir(job.id);
        tokio::fs::create_dir_all(&job_dir).await?;
        let temp_dir = self.paths.tmp_dir().join(job.id.to_string());
        tokio::fs::create_dir_all(&temp_dir).await?;

        match candidate.kind {
            CandidateKind::Audio => {
                self.update_status(job.id, JobStatus::Downloading).await?;
                let input_path = temp_dir.join(format!("{}.input", candidate.id));
                let headers = self.download_headers(job, candidate).await?;
                let downloader =
                    Downloader::new(settings.download_timeout, settings.max_download_bytes)?;
                downloader
                    .download_to_file_with_headers(&candidate.url, &input_path, headers)
                    .await?;

                self.update_status(job.id, JobStatus::Transcoding).await?;
                let output_path = job_dir.join(format!("audio-{}.mp3", candidate.id));
                self.transcoder(settings)
                    .audio_to_mp3(&input_path, &output_path, &job.bitrate)
                    .await?;
                self.insert_artifact(
                    job.id,
                    OutputKind::Audio,
                    output_path,
                    "audio/mpeg",
                    settings,
                )
                .await?;
            }
            CandidateKind::Video | CandidateKind::Manifest => {
                let headers = self.download_headers(job, candidate).await?;
                if candidate.kind == CandidateKind::Manifest
                    && is_dash_manifest_candidate(candidate)
                {
                    return Err(RkError::Source(
                        "DASH/MPD manifest is not supported yet; queued for adapter work"
                            .to_string(),
                    ));
                }
                if candidate.kind == CandidateKind::Manifest
                    && !is_yt_dlp_inline_manifest_candidate(candidate)
                {
                    self.validate_manifest_candidate(job.id, candidate, headers.clone(), settings)
                        .await?;
                }
                if job.outputs.contains(&OutputKind::Audio)
                    && !job.outputs.contains(&OutputKind::Video)
                {
                    let output_path = job_dir.join(format!("audio-{}.mp3", candidate.id));
                    if is_yt_dlp_inline_manifest_candidate(candidate) {
                        self.process_yt_dlp_delegated_download(
                            job,
                            candidate,
                            &temp_dir,
                            &output_path,
                            OutputKind::Audio,
                            settings,
                        )
                        .await?;
                    } else if candidate.kind == CandidateKind::Manifest {
                        let raw_result = async {
                            let transcoder = self.transcoder(settings);
                            let stream_info = transcoder
                                .probe_url_with_headers(&candidate.url, &headers)
                                .await?;
                            if !stream_info.has_audio {
                                return Err(RkError::Source(
                                    "candidate has no audio stream".to_string(),
                                ));
                            }
                            self.update_status(job.id, JobStatus::Transcoding).await?;
                            transcoder
                                .media_url_to_mp3_with_headers(
                                    &candidate.url,
                                    &output_path,
                                    &job.bitrate,
                                    &headers,
                                )
                                .await
                        }
                        .await;
                        if let Err(raw_error) = raw_result {
                            if self.should_try_yt_dlp_delegated_download(candidate) {
                                self.process_yt_dlp_delegated_download(
                                    job,
                                    candidate,
                                    &temp_dir,
                                    &output_path,
                                    OutputKind::Audio,
                                    settings,
                                )
                                .await
                                .map_err(|fallback_error| {
                                    RkError::Source(format!(
                                        "raw candidate failed: {raw_error}; yt-dlp delegated download failed: {fallback_error}"
                                    ))
                                })?;
                            } else {
                                return Err(raw_error);
                            }
                        }
                    } else {
                        self.update_status(job.id, JobStatus::Downloading).await?;
                        let input_path = temp_dir.join(format!("{}.input", candidate.id));
                        Downloader::new(settings.download_timeout, settings.max_download_bytes)?
                            .download_to_file_with_headers(&candidate.url, &input_path, headers)
                            .await?;
                        let stream_info =
                            self.transcoder(settings).probe_media(&input_path).await?;
                        if !stream_info.has_audio {
                            return Err(RkError::Source(
                                "candidate has no audio stream".to_string(),
                            ));
                        }
                        self.update_status(job.id, JobStatus::Transcoding).await?;
                        self.transcoder(settings)
                            .audio_to_mp3(&input_path, &output_path, &job.bitrate)
                            .await?;
                    }
                    self.insert_artifact(
                        job.id,
                        OutputKind::Audio,
                        output_path,
                        "audio/mpeg",
                        settings,
                    )
                    .await?;
                } else {
                    let output_path = job_dir.join(format!("video-{}.mp4", candidate.id));
                    if is_yt_dlp_inline_manifest_candidate(candidate) {
                        self.process_yt_dlp_delegated_download(
                            job,
                            candidate,
                            &temp_dir,
                            &output_path,
                            OutputKind::Video,
                            settings,
                        )
                        .await?;
                    } else if candidate.kind == CandidateKind::Manifest {
                        let raw_result = async {
                            let transcoder = self.transcoder(settings);
                            let stream_info = transcoder
                                .probe_url_with_headers(&candidate.url, &headers)
                                .await?;
                            if !stream_info.has_video {
                                return Err(RkError::Source(
                                    "candidate has no video stream".to_string(),
                                ));
                            }
                            self.update_status(job.id, JobStatus::Remuxing).await?;
                            let audio_candidate =
                                best_companion_audio(candidate, available_candidates);
                            if job.outputs.contains(&OutputKind::Audio)
                                && !stream_info.has_audio
                                && audio_candidate.is_none()
                            {
                                return Err(RkError::Source(
                                    "candidate has no audio stream and no companion audio candidate"
                                        .to_string(),
                                ));
                            }
                            if let Some(audio_candidate) = audio_candidate {
                                let audio_headers =
                                    self.download_headers(job, audio_candidate).await?;
                                transcoder
                                    .media_urls_to_mp4_with_headers(
                                        &candidate.url,
                                        &headers,
                                        &audio_candidate.url,
                                        &audio_headers,
                                        &output_path,
                                    )
                                    .await
                            } else {
                                transcoder
                                    .media_url_to_mp4_with_headers(
                                        &candidate.url,
                                        &output_path,
                                        &headers,
                                    )
                                    .await
                            }
                        }
                        .await;
                        if let Err(raw_error) = raw_result {
                            if self.should_try_yt_dlp_delegated_download(candidate) {
                                self.process_yt_dlp_delegated_download(
                                    job,
                                    candidate,
                                    &temp_dir,
                                    &output_path,
                                    OutputKind::Video,
                                    settings,
                                )
                                .await
                                .map_err(|fallback_error| {
                                    RkError::Source(format!(
                                        "raw candidate failed: {raw_error}; yt-dlp delegated download failed: {fallback_error}"
                                    ))
                                })?;
                            } else {
                                return Err(raw_error);
                            }
                        }
                    } else {
                        self.update_status(job.id, JobStatus::Downloading).await?;
                        let input_path = temp_dir.join(format!("{}.input", candidate.id));
                        Downloader::new(settings.download_timeout, settings.max_download_bytes)?
                            .download_to_file_with_headers(&candidate.url, &input_path, headers)
                            .await?;
                        let stream_info =
                            self.transcoder(settings).probe_media(&input_path).await?;
                        if !stream_info.has_video {
                            return Err(RkError::Source(
                                "candidate has no video stream".to_string(),
                            ));
                        }
                        self.update_status(job.id, JobStatus::Remuxing).await?;
                        self.transcoder(settings)
                            .media_to_mp4(&input_path, &output_path)
                            .await?;
                    }
                    self.insert_artifact(
                        job.id,
                        OutputKind::Video,
                        output_path,
                        "video/mp4",
                        settings,
                    )
                    .await?;
                }
            }
            CandidateKind::Image => {
                self.update_status(job.id, JobStatus::Downloading).await?;
                let output_path = job_dir.join(format!("image-{}.bin", candidate.id));
                let headers = self.download_headers(job, candidate).await?;
                Downloader::new(settings.download_timeout, settings.max_download_bytes)?
                    .download_to_file_with_headers(&candidate.url, &output_path, headers)
                    .await?;
                self.insert_artifact(
                    job.id,
                    OutputKind::Image,
                    output_path,
                    candidate
                        .content_type
                        .as_deref()
                        .unwrap_or("application/octet-stream"),
                    settings,
                )
                .await?;
            }
            CandidateKind::Html => {
                let artifacts = self.job_store.list_artifacts(job.id).await?;
                if !artifacts
                    .iter()
                    .any(|artifact| artifact.kind == OutputKind::PageHtml)
                {
                    return Err(RkError::Source(
                        "HTML candidate requires page_html browser snapshot output".to_string(),
                    ));
                }
            }
            CandidateKind::Unknown => {}
        }

        tokio::fs::remove_dir_all(&temp_dir).await.ok();
        Ok(())
    }

    async fn download_headers(
        &self,
        job: &JobRecord,
        candidate: &MediaCandidate,
    ) -> Result<HeaderMap> {
        let mut headers = safe_candidate_download_headers(candidate);
        if !candidate.requires_authorization && candidate.extractor != "browser_probe" {
            return Ok(headers);
        }
        let Some(browser_probe) = &self.browser_probe else {
            return Ok(headers);
        };
        let header_url = if is_yt_dlp_inline_manifest_candidate(candidate) {
            job.source_url.as_str()
        } else {
            candidate.url.as_str()
        };
        let initiator_url = if is_yt_dlp_inline_manifest_candidate(candidate) {
            candidate
                .initiator_url
                .as_deref()
                .or(Some(job.source_url.as_str()))
        } else {
            candidate.initiator_url.as_deref()
        };
        let profile_headers = browser_probe
            .headers_for_url(&job.profile_id, header_url, initiator_url)
            .await?;
        headers.extend(profile_headers);
        Ok(headers)
    }

    async fn validate_manifest_candidate(
        &self,
        job_id: Uuid,
        candidate: &MediaCandidate,
        headers: HeaderMap,
        settings: &EffectiveRuntimeSettings,
    ) -> Result<()> {
        self.record_event(PipelineEvent::new(
            job_id,
            "manifest_validate",
            candidate.extractor.clone(),
            PipelineEventType::Probe,
            serde_json::json!({
                "candidate_id": candidate.id,
                "url": candidate.url,
                "kind": candidate.kind.as_str(),
            }),
        ))
        .await;
        let client = reqwest::Client::builder()
            .timeout(settings.download_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("ReflectionKing/0.1")
            .build()?;
        reflection_core::manifest::validate_manifest_url(&client, &candidate.url, headers).await
    }

    fn should_try_yt_dlp_delegated_download(&self, candidate: &MediaCandidate) -> bool {
        if is_yt_dlp_inline_manifest_candidate(candidate) {
            return self.config.yt_dlp_path.is_some();
        }
        candidate.extractor == "yt_dlp"
            && !candidate.requires_authorization
            && candidate
                .initiator_url
                .as_deref()
                .and_then(|url| reflection_core::url_policy::parse_and_validate_url(url).ok())
                .is_some()
            && self.config.yt_dlp_path.is_some()
    }

    async fn process_yt_dlp_delegated_download(
        &self,
        job: &JobRecord,
        candidate: &MediaCandidate,
        temp_dir: &Path,
        output_path: &Path,
        output_kind: OutputKind,
        settings: &EffectiveRuntimeSettings,
    ) -> Result<()> {
        let source_url = if is_yt_dlp_inline_manifest_candidate(candidate) {
            job.source_url.as_str()
        } else {
            candidate
                .initiator_url
                .as_deref()
                .unwrap_or(job.source_url.as_str())
        };
        reflection_core::url_policy::parse_and_validate_url(source_url)?;
        let Some(yt_dlp_path) = self.config.yt_dlp_path.as_ref() else {
            return Err(RkError::Source("yt-dlp is not configured".to_string()));
        };

        self.update_status(
            job.id,
            if output_kind == OutputKind::Audio {
                JobStatus::Downloading
            } else {
                JobStatus::Remuxing
            },
        )
        .await?;

        let output_template = temp_dir.join(format!("yt-dlp-{}-download.%(ext)s", candidate.id));
        let mut command = Command::new(yt_dlp_path);
        command
            .arg("--no-playlist")
            .arg("--no-cache-dir")
            .arg("--max-filesize")
            .arg(yt_dlp_max_filesize(settings.max_download_bytes))
            .arg("-o")
            .arg(&output_template);
        let headers = self.download_headers(job, candidate).await?;
        add_yt_dlp_header_args(&mut command, &headers);
        let cookies_file = self
            .yt_dlp_cookies_file(job, candidate, source_url, temp_dir)
            .await?;
        if let Some(path) = cookies_file.as_deref() {
            command.arg("--cookies").arg(path);
        }
        if let Some(format_id) = candidate_metadata_text(candidate, "format_id") {
            command.arg("-f").arg(format_id);
        }
        command.arg(source_url);

        command.kill_on_drop(true);
        let output_result = tokio_time::timeout(settings.download_timeout, command.output())
            .await
            .map_err(|_| RkError::Source("yt-dlp delegated download timed out".to_string()))
            .and_then(|result| result.map_err(RkError::Io));
        let output = match output_result {
            Ok(output) => output,
            Err(error) => {
                if let Some(path) = cookies_file {
                    tokio::fs::remove_file(path).await.ok();
                }
                return Err(error);
            }
        };
        if !output.status.success() {
            if let Some(path) = cookies_file {
                tokio::fs::remove_file(path).await.ok();
            }
            return Err(RkError::Source(format!(
                "yt-dlp delegated download exited with {}: {}",
                output
                    .status
                    .code()
                    .map_or_else(|| "signal".to_string(), |code| code.to_string()),
                limited_process_stderr(&output.stderr)
            )));
        }
        if let Some(path) = cookies_file {
            tokio::fs::remove_file(path).await.ok();
        }

        let downloaded = find_delegated_download(temp_dir, candidate.id)?;
        let bytes = tokio::fs::metadata(&downloaded).await?.len();
        if bytes > settings.max_download_bytes {
            return Err(RkError::DownloadTooLarge {
                max_bytes: settings.max_download_bytes,
            });
        }

        match output_kind {
            OutputKind::Audio => {
                self.update_status(job.id, JobStatus::Transcoding).await?;
                self.transcoder(settings)
                    .audio_to_mp3(&downloaded, output_path, &job.bitrate)
                    .await
            }
            OutputKind::Video => {
                self.update_status(job.id, JobStatus::Remuxing).await?;
                self.transcoder(settings)
                    .media_to_mp4(&downloaded, output_path)
                    .await
            }
            _ => Err(RkError::Source(
                "yt-dlp delegated download only supports audio or video outputs".to_string(),
            )),
        }
    }

    async fn yt_dlp_cookies_file(
        &self,
        job: &JobRecord,
        candidate: &MediaCandidate,
        source_url: &str,
        temp_dir: &Path,
    ) -> Result<Option<PathBuf>> {
        if !matches!(
            job.auth_mode,
            reflection_core::models::AuthMode::Auto
                | reflection_core::models::AuthMode::Profile
                | reflection_core::models::AuthMode::Cookies
        ) {
            return Ok(None);
        }
        let Some(browser_probe) = &self.browser_probe else {
            return Ok(None);
        };
        let cookie_url = if is_yt_dlp_inline_manifest_candidate(candidate) {
            source_url
        } else {
            candidate.initiator_url.as_deref().unwrap_or(source_url)
        };
        let cookies = browser_probe
            .cookies_for_url(&job.profile_id, cookie_url)
            .await
            .unwrap_or_default();
        write_temp_cookies_file(temp_dir, job.id, &cookies).await
    }

    async fn insert_artifact(
        &self,
        job_id: Uuid,
        kind: OutputKind,
        path: std::path::PathBuf,
        content_type: &str,
        settings: &EffectiveRuntimeSettings,
    ) -> Result<()> {
        let raw_bytes = tokio::fs::metadata(&path).await?.len();
        if raw_bytes > settings.max_download_bytes {
            tokio::fs::remove_file(&path).await.ok();
            return Err(RkError::DownloadTooLarge {
                max_bytes: settings.max_download_bytes,
            });
        }
        let bytes = raw_bytes as i64;
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| RkError::Source("invalid artifact filename".to_string()))?;
        let artifact = ArtifactView {
            id: Uuid::new_v4(),
            job_id,
            kind,
            path: path.display().to_string(),
            media_url: format!("{}/media/{job_id}/{filename}", settings.public_base_url),
            content_type: content_type.to_string(),
            bytes,
            created_at: OffsetDateTime::now_utc(),
        };
        self.job_store.insert_artifact(&artifact).await
    }

    async fn persist_page_snapshot(
        &self,
        job: &JobRecord,
        snapshot: &PageSnapshot,
        settings: &EffectiveRuntimeSettings,
    ) -> Result<()> {
        let job_dir = self.paths.public_job_dir(job.id);
        let page_dir = job_dir.join("page");
        let assets_dir = page_dir.join("assets");
        let metadata_dir = page_dir.join("metadata");
        let preview_dir = page_dir.join("preview");
        tokio::fs::create_dir_all(&assets_dir).await?;
        tokio::fs::create_dir_all(&metadata_dir).await?;
        tokio::fs::create_dir_all(&preview_dir).await?;

        let downloader = Downloader::new(
            settings.download_timeout,
            settings
                .page_archive_max_resource_bytes
                .min(settings.max_download_bytes),
        )?;
        let mut used_asset_paths = HashSet::new();
        let mut rewrites = HashMap::new();
        let mut resource_records = Vec::new();
        let mut total_bytes = snapshot.html.len() as u64;

        for resource in snapshot
            .resources
            .iter()
            .filter(|resource| page_resource_is_archivable(resource))
            .take(settings.page_archive_max_resources)
        {
            if total_bytes >= settings.page_archive_max_total_bytes {
                resource_records.push(serde_json::json!({
                    "url": &resource.url,
                    "source": &resource.source,
                    "skipped": true,
                    "reason": "archive_total_limit_reached",
                }));
                continue;
            }
            let remaining = settings
                .page_archive_max_total_bytes
                .saturating_sub(total_bytes);
            if let Some(length) = resource
                .content_length
                .and_then(|value| u64::try_from(value).ok())
            {
                if length > settings.page_archive_max_resource_bytes || length > remaining {
                    resource_records.push(page_resource_record(
                        resource,
                        None,
                        0,
                        Some("too_large"),
                    ));
                    continue;
                }
            }

            let asset_name = unique_asset_name(resource, &mut used_asset_paths);
            let asset_path = assets_dir.join(&asset_name);
            let download_result = async {
                validate_candidate_url(&resource.url)?;
                downloader
                    .download_to_file_with_headers(&resource.url, &asset_path, HeaderMap::new())
                    .await?;
                let bytes = tokio::fs::metadata(&asset_path).await?.len();
                if bytes > remaining {
                    tokio::fs::remove_file(&asset_path).await.ok();
                    return Err(RkError::DownloadTooLarge {
                        max_bytes: remaining,
                    });
                }
                Ok::<u64, RkError>(bytes)
            }
            .await;

            match download_result {
                Ok(bytes) => {
                    total_bytes = total_bytes.saturating_add(bytes);
                    let relative_path = format!("assets/{asset_name}");
                    rewrites.insert(resource.url.clone(), relative_path.clone());
                    resource_records.push(page_resource_record(
                        resource,
                        Some(&relative_path),
                        bytes,
                        None,
                    ));
                }
                Err(error) => {
                    tokio::fs::remove_file(&asset_path).await.ok();
                    resource_records.push(page_resource_record(
                        resource,
                        None,
                        0,
                        Some(&friendly_candidate_failure(&error)),
                    ));
                }
            }
        }

        let html = rewrite_page_html(&snapshot.html, &rewrites);
        let html_path = job_dir.join("page.html");
        tokio::fs::write(&html_path, html).await?;
        tokio::fs::copy(&html_path, page_dir.join("index.html")).await?;
        self.insert_artifact(
            job.id,
            OutputKind::PageHtml,
            html_path.clone(),
            "text/html; charset=utf-8",
            settings,
        )
        .await?;

        let text_path = job_dir.join("page.txt");
        tokio::fs::write(&text_path, &snapshot.text).await?;
        tokio::fs::copy(&text_path, page_dir.join("page.txt")).await?;
        self.insert_artifact(
            job.id,
            OutputKind::PageHtml,
            text_path.clone(),
            "text/plain; charset=utf-8",
            settings,
        )
        .await?;

        if let Some(bytes) = decode_data_url(&snapshot.screenshot)? {
            let screenshot_path = job_dir.join("screenshot.png");
            tokio::fs::write(&screenshot_path, bytes).await?;
            tokio::fs::copy(&screenshot_path, preview_dir.join("screenshot.png")).await?;
            self.insert_artifact(
                job.id,
                OutputKind::PageHtml,
                screenshot_path,
                "image/png",
                settings,
            )
            .await?;
        }

        let metadata_path = job_dir.join("resources.json");
        tokio::fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "final_url": &snapshot.final_url,
                "title": &snapshot.title,
                "captured_at": &snapshot.captured_at,
                "resources": resource_records,
            }))?,
        )
        .await?;
        tokio::fs::copy(&metadata_path, metadata_dir.join("resources.json")).await?;
        self.insert_artifact(
            job.id,
            OutputKind::PageHtml,
            metadata_path.clone(),
            "application/json",
            settings,
        )
        .await?;

        let archive_path = job_dir.join("archive.zip");
        write_zip_archive(&page_dir, &archive_path).await?;
        self.insert_artifact(
            job.id,
            OutputKind::PageHtml,
            archive_path,
            "application/zip",
            settings,
        )
        .await?;
        Ok(())
    }

    async fn enqueue(&self, job_id: Uuid) -> Result<()> {
        self.queue_tx
            .send(job_id)
            .await
            .map_err(|error| RkError::Source(format!("failed to enqueue job: {error}")))
    }

    async fn update_status(&self, job_id: Uuid, status: JobStatus) -> Result<()> {
        self.job_store.update_status(job_id, status).await?;
        self.record_event(PipelineEvent::new(
            job_id,
            status.as_str(),
            "worker",
            PipelineEventType::StatusChange,
            serde_json::json!({ "status": status.as_str() }),
        ))
        .await;
        Ok(())
    }

    async fn mark_ready(&self, job_id: Uuid, media_url: String) -> Result<()> {
        self.job_store.mark_ready(job_id, &media_url).await?;
        self.record_event(PipelineEvent::new(
            job_id,
            "ready",
            "worker",
            PipelineEventType::StatusChange,
            serde_json::json!({ "status": "ready", "media_url": media_url }),
        ))
        .await;
        Ok(())
    }

    async fn mark_error(&self, job_id: Uuid, error: String, class: ErrorClass) {
        if let Err(store_error) = self.job_store.mark_error(job_id, &error, class).await {
            error!(%job_id, "failed to persist job error: {store_error}");
        }
        self.record_event(PipelineEvent::new(
            job_id,
            "error",
            "worker",
            PipelineEventType::Error,
            serde_json::json!({ "error": error, "error_class": class.as_str() }),
        ))
        .await;
    }

    async fn mark_needs_profile(&self, job_id: Uuid, error: String, class: ErrorClass) {
        if let Err(store_error) = self
            .job_store
            .mark_needs_profile(job_id, &error, class)
            .await
        {
            error!(%job_id, "failed to persist needs-profile state: {store_error}");
        }
        self.record_event(PipelineEvent::new(
            job_id,
            "needs_profile",
            "worker",
            PipelineEventType::StatusChange,
            serde_json::json!({ "error": error, "error_class": class.as_str() }),
        ))
        .await;
    }

    /// Best-effort pipeline event recording: a logging failure must never fail
    /// the job it is describing.
    pub async fn record_event(&self, event: PipelineEvent) {
        if let Err(error) = self.job_store.log_event(&event).await {
            warn!(job_id = %event.job_id, "failed to record pipeline event: {error}");
        }
    }

    pub async fn get_trace(&self, job_id: Uuid) -> Result<JobTrace> {
        self.job_store.get_trace(job_id).await
    }
}

fn resolver_error_summary(attempts: &[reflection_core::extractors::AttemptLog]) -> Option<String> {
    let errors = attempts
        .iter()
        .filter_map(|attempt| {
            attempt
                .error
                .as_deref()
                .filter(|error| !error.trim().is_empty())
                .map(|error| format!("{}: {}", attempt.extractor, error.trim()))
        })
        .rev()
        .take(3)
        .collect::<Vec<_>>();

    if errors.is_empty() {
        None
    } else {
        Some(errors.into_iter().rev().collect::<Vec<_>>().join(" | "))
    }
}

fn is_profile_required_error(error: &RkError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("fresh cookies")
        || message.contains("sign in")
        || message.contains("login required")
        || message.contains("requires authorization")
        || message.contains("requires headers")
        || message.contains("needs profile")
        || message.contains("profile required")
        || message.contains("401")
        || message.contains("403")
}

fn validate_candidate_url(url: &str) -> Result<()> {
    reflection_core::url_policy::parse_and_validate_url(url).map(|_| ())
}

fn page_resource_is_archivable(resource: &reflection_core::browser_probe::PageResource) -> bool {
    let method = resource.method.as_deref().unwrap_or("GET");
    if method != "GET" && method != "HEAD" {
        return false;
    }
    if resource.url.starts_with("blob:") || resource.url.starts_with("data:") {
        return false;
    }
    let content_type = resource
        .content_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kind = resource
        .resource_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        kind.as_str(),
        "stylesheet" | "script" | "image" | "font" | "media" | "video" | "audio" | "manifest"
    ) || content_type.starts_with("text/css")
        || content_type.contains("javascript")
        || content_type.starts_with("image/")
        || content_type.starts_with("font/")
        || content_type.starts_with("audio/")
        || content_type.starts_with("video/")
}

fn unique_asset_name(
    resource: &reflection_core::browser_probe::PageResource,
    used: &mut HashSet<String>,
) -> String {
    let parsed = url::Url::parse(&resource.url).ok();
    let host = parsed
        .as_ref()
        .and_then(|url| url.host_str())
        .unwrap_or("resource");
    let path_name = parsed
        .as_ref()
        .and_then(|url| {
            url.path_segments()
                .and_then(|mut segments| segments.next_back())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or("index");
    let mut base = sanitize_filename(&format!("{host}-{path_name}"));
    if !base.contains('.') {
        base.push_str(extension_for_resource(resource));
    }
    if base.len() > 120 {
        base.truncate(120);
    }
    let original = base.clone();
    let mut index = 1usize;
    while !used.insert(base.clone()) {
        base = format!("{index}-{original}");
        index += 1;
    }
    base
}

fn sanitize_filename(value: &str) -> String {
    let out = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches(['.', '_'])
        .to_string();
    if out.is_empty() {
        "resource".to_string()
    } else {
        out
    }
}

fn extension_for_resource(resource: &reflection_core::browser_probe::PageResource) -> &'static str {
    let content_type = resource
        .content_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if content_type.starts_with("text/css") {
        ".css"
    } else if content_type.contains("javascript") {
        ".js"
    } else if content_type.contains("png") {
        ".png"
    } else if content_type.contains("jpeg") || content_type.contains("jpg") {
        ".jpg"
    } else if content_type.contains("webp") {
        ".webp"
    } else if content_type.contains("gif") {
        ".gif"
    } else if content_type.contains("woff2") {
        ".woff2"
    } else if content_type.starts_with("font/") {
        ".font"
    } else if content_type.contains("mp4") {
        ".mp4"
    } else if content_type.contains("mpegurl") {
        ".m3u8"
    } else {
        ".bin"
    }
}

fn rewrite_page_html(html: &str, rewrites: &HashMap<String, String>) -> String {
    let mut out = html.to_string();
    for (url, relative) in rewrites {
        out = out.replace(url, relative);
        out = out.replace(&html_escape_attribute(url), relative);
    }
    out
}

fn html_escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn page_resource_record(
    resource: &reflection_core::browser_probe::PageResource,
    local_path: Option<&str>,
    bytes: u64,
    skipped_reason: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "url": &resource.url,
        "method": &resource.method,
        "status": resource.status,
        "content_type": &resource.content_type,
        "content_length": resource.content_length,
        "resource_type": &resource.resource_type,
        "initiator_url": &resource.initiator_url,
        "source": &resource.source,
        "local_path": local_path,
        "bytes": bytes,
        "skipped": skipped_reason.is_some(),
        "reason": skipped_reason,
    })
}

fn decode_data_url(value: &Option<String>) -> Result<Option<Vec<u8>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some((meta, payload)) = value.split_once(',') else {
        return Ok(None);
    };
    if !meta.starts_with("data:image/png;base64") {
        return Ok(None);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| RkError::Source(format!("invalid screenshot data url: {error}")))?;
    Ok(Some(bytes))
}

async fn write_zip_archive(source_dir: &Path, archive_path: &Path) -> Result<()> {
    let source_dir = source_dir.to_path_buf();
    let archive_path = archive_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let file = std::fs::File::create(&archive_path)?;
        let mut writer = std::io::BufWriter::new(file);
        let files = collect_zip_files(&source_dir, &source_dir)?;
        let mut entries = Vec::with_capacity(files.len());
        for (name, path) in files {
            let bytes = std::fs::read(path)?;
            let offset = writer.stream_position()?;
            write_zip_local_file(&mut writer, &name, &bytes)?;
            entries.push(ZipEntryRecord {
                name,
                crc32: crc32(&bytes),
                size: bytes.len() as u32,
                offset: offset as u32,
            });
        }
        let central_offset = writer.stream_position()?;
        for entry in &entries {
            write_zip_central_directory(&mut writer, entry)?;
        }
        let central_size = writer.stream_position()? - central_offset;
        write_zip_end(
            &mut writer,
            entries.len() as u16,
            central_size as u32,
            central_offset as u32,
        )?;
        writer.flush()?;
        Ok(())
    })
    .await
    .map_err(|error| RkError::Source(format!("zip task failed: {error}")))??;
    Ok(())
}

fn collect_zip_files(root: &Path, dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    collect_zip_files_inner(root, dir, &mut out)?;
    out.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(out)
}

fn collect_zip_files_inner(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    let mut entries = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_zip_files_inner(root, &path, out)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| RkError::Source(format!("zip path error: {error}")))?;
        let name = relative.to_string_lossy().replace('\\', "/");
        out.push((name, path));
    }
    Ok(())
}

struct ZipEntryRecord {
    name: String,
    crc32: u32,
    size: u32,
    offset: u32,
}

fn write_zip_local_file<W: Write>(writer: &mut W, name: &str, bytes: &[u8]) -> Result<()> {
    let name_bytes = name.as_bytes();
    let crc = crc32(bytes);
    writer.write_all(&0x0403_4b50u32.to_le_bytes())?;
    writer.write_all(&20u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&crc.to_le_bytes())?;
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&(name_bytes.len() as u16).to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(name_bytes)?;
    writer.write_all(bytes)?;
    Ok(())
}

fn write_zip_central_directory<W: Write>(writer: &mut W, entry: &ZipEntryRecord) -> Result<()> {
    let name_bytes = entry.name.as_bytes();
    writer.write_all(&0x0201_4b50u32.to_le_bytes())?;
    writer.write_all(&20u16.to_le_bytes())?;
    writer.write_all(&20u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&entry.crc32.to_le_bytes())?;
    writer.write_all(&entry.size.to_le_bytes())?;
    writer.write_all(&entry.size.to_le_bytes())?;
    writer.write_all(&(name_bytes.len() as u16).to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u32.to_le_bytes())?;
    writer.write_all(&entry.offset.to_le_bytes())?;
    writer.write_all(name_bytes)?;
    Ok(())
}

fn write_zip_end<W: Write>(
    writer: &mut W,
    entry_count: u16,
    central_size: u32,
    central_offset: u32,
) -> Result<()> {
    writer.write_all(&0x0605_4b50u32.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    writer.write_all(&entry_count.to_le_bytes())?;
    writer.write_all(&entry_count.to_le_bytes())?;
    writer.write_all(&central_size.to_le_bytes())?;
    writer.write_all(&central_offset.to_le_bytes())?;
    writer.write_all(&0u16.to_le_bytes())?;
    Ok(())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn job_scoped_profile_id(job_id: Uuid, requester_key_id: Option<Uuid>) -> String {
    let job = job_id.simple().to_string();
    let actor = requester_key_id
        .map(|id| id.simple().to_string()[..12].to_string())
        .unwrap_or_else(|| "admin".to_string());
    format!("job_{job}_{actor}")
}

fn is_yt_dlp_inline_manifest_candidate(candidate: &MediaCandidate) -> bool {
    candidate.extractor == "yt_dlp"
        && candidate.kind == CandidateKind::Manifest
        && candidate.url.starts_with("data:application/x-mpegurl")
}

fn is_dash_manifest_candidate(candidate: &MediaCandidate) -> bool {
    let url = candidate.url.to_ascii_lowercase();
    let content_type = candidate
        .content_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    url.contains(".mpd") || content_type.contains("dash+xml")
}

fn safe_candidate_download_headers(candidate: &MediaCandidate) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let Some(values) = candidate
        .metadata_json
        .get("download_headers")
        .and_then(|value| value.as_object())
    else {
        return headers;
    };

    for (name, value) in values {
        let lowered = name.to_ascii_lowercase();
        if !matches!(
            lowered.as_str(),
            "user-agent" | "accept" | "accept-language" | "referer" | "origin" | "range"
        ) {
            continue;
        }
        let Some(value) = value.as_str() else {
            continue;
        };
        let Ok(header_name) = HeaderName::from_bytes(lowered.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = HeaderValue::from_str(value) else {
            continue;
        };
        headers.insert(header_name, header_value);
    }
    headers
}

fn add_yt_dlp_header_args(command: &mut Command, headers: &HeaderMap) {
    for (name, value) in headers {
        if name.as_str().eq_ignore_ascii_case("cookie") {
            continue;
        }
        if let Ok(value) = value.to_str() {
            command.arg("--add-header");
            command.arg(format!("{}: {}", name.as_str(), value));
        }
    }
}

async fn write_temp_cookies_file(
    temp_dir: &Path,
    job_id: Uuid,
    cookies: &[BrowserCookie],
) -> Result<Option<PathBuf>> {
    if cookies.is_empty() {
        return Ok(None);
    }
    tokio::fs::create_dir_all(temp_dir).await?;
    let path = temp_dir.join(format!("yt-dlp-{job_id}.cookies.txt"));
    tokio::fs::write(&path, netscape_cookie_file(cookies)).await?;
    Ok(Some(path))
}

fn netscape_cookie_file(cookies: &[BrowserCookie]) -> String {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for cookie in cookies {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            cookie.domain,
            if cookie.domain.starts_with('.') {
                "TRUE"
            } else {
                "FALSE"
            },
            cookie.path,
            if cookie.secure { "TRUE" } else { "FALSE" },
            cookie_expires(cookie.expires),
            sanitize_cookie_field(&cookie.name),
            sanitize_cookie_field(&cookie.value),
        ));
    }
    out
}

fn cookie_expires(value: f64) -> i64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.floor() as i64
    }
}

fn sanitize_cookie_field(value: &str) -> String {
    value.replace(['\t', '\r', '\n'], "")
}

fn find_delegated_download(temp_dir: &Path, candidate_id: Uuid) -> Result<PathBuf> {
    let prefix = format!("yt-dlp-{candidate_id}-download.");
    let mut matches = std::fs::read_dir(temp_dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && !name.ends_with(".part"))
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches.into_iter().next().ok_or_else(|| {
        RkError::Source("yt-dlp delegated download did not create a media file".to_string())
    })
}

fn limited_process_stderr(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim();
    if text.is_empty() {
        "no stderr".to_string()
    } else if text.len() > 700 {
        format!("{}...", &text[..700])
    } else {
        text.to_string()
    }
}

fn yt_dlp_max_filesize(max_bytes: u64) -> String {
    let mib = (max_bytes / 1024 / 1024).max(1);
    format!("{mib}M")
}

#[derive(Debug, Clone)]
struct EffectiveRuntimeSettings {
    public_base_url: String,
    max_download_bytes: u64,
    download_timeout: Duration,
    yt_dlp_timeout: Duration,
    yt_dlp_max_json_bytes: usize,
    job_ttl_hours: u64,
    page_archive_max_resources: usize,
    page_archive_max_resource_bytes: u64,
    page_archive_max_total_bytes: u64,
}

impl EffectiveRuntimeSettings {
    fn from_config(config: &AppConfig, values: &std::collections::HashMap<String, String>) -> Self {
        let max_download_mb = setting_u64(
            values,
            "max_download_mb",
            config.max_download_bytes / 1024 / 1024,
        );
        let download_timeout_seconds = setting_u64(
            values,
            "download_timeout_seconds",
            config.download_timeout.as_secs(),
        );
        let yt_dlp_timeout_seconds = setting_u64(
            values,
            "yt_dlp_timeout_seconds",
            config.yt_dlp_timeout.as_secs(),
        );
        let yt_dlp_max_json_mb = setting_usize(
            values,
            "yt_dlp_max_json_mb",
            config.yt_dlp_max_json_bytes / 1024 / 1024,
        );
        let page_archive_max_resource_mb = setting_u64(values, "page_archive_max_resource_mb", 16);
        let page_archive_max_total_mb = setting_u64(values, "page_archive_max_total_mb", 200);
        Self {
            public_base_url: values
                .get("public_base_url")
                .cloned()
                .unwrap_or_else(|| config.public_base_url.clone()),
            max_download_bytes: max_download_mb.saturating_mul(1024).saturating_mul(1024),
            download_timeout: Duration::from_secs(download_timeout_seconds),
            yt_dlp_timeout: Duration::from_secs(yt_dlp_timeout_seconds),
            yt_dlp_max_json_bytes: yt_dlp_max_json_mb.saturating_mul(1024).saturating_mul(1024),
            job_ttl_hours: setting_u64(values, "job_ttl_hours", config.job_ttl_hours),
            page_archive_max_resources: setting_usize(values, "page_archive_max_resources", 200),
            page_archive_max_resource_bytes: page_archive_max_resource_mb
                .saturating_mul(1024)
                .saturating_mul(1024),
            page_archive_max_total_bytes: page_archive_max_total_mb
                .saturating_mul(1024)
                .saturating_mul(1024),
        }
    }

    fn to_view(&self, config: &AppConfig) -> RuntimeSettingsView {
        RuntimeSettingsView {
            public_base_url: self.public_base_url.clone(),
            max_download_bytes: self.max_download_bytes,
            max_concurrent_jobs: config.max_concurrent_jobs,
            download_timeout_seconds: self.download_timeout.as_secs(),
            browser_probe_timeout_seconds: config.browser_probe_timeout.as_secs(),
            yt_dlp_timeout_seconds: self.yt_dlp_timeout.as_secs(),
            yt_dlp_max_json_bytes: self.yt_dlp_max_json_bytes,
            job_ttl_hours: self.job_ttl_hours,
            page_archive_max_resources: self.page_archive_max_resources,
            page_archive_max_resource_bytes: self.page_archive_max_resource_bytes,
            page_archive_max_total_bytes: self.page_archive_max_total_bytes,
            ffmpeg_path: config.ffmpeg_path.display().to_string(),
            browser_probe_url: config.browser_probe_url.clone(),
            yt_dlp_path: config
                .yt_dlp_path
                .as_ref()
                .map(|path| path.display().to_string()),
            you_get_path: config
                .you_get_path
                .as_ref()
                .map(|path| path.display().to_string()),
            lux_path: config
                .lux_path
                .as_ref()
                .map(|path| path.display().to_string()),
            streamlink_path: config
                .streamlink_path
                .as_ref()
                .map(|path| path.display().to_string()),
            external_probe_timeout_seconds: config.external_probe_timeout.as_secs(),
        }
    }
}

fn setting_u64(
    values: &std::collections::HashMap<String, String>,
    key: &str,
    fallback: u64,
) -> u64 {
    values
        .get(key)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(fallback)
}

fn setting_usize(
    values: &std::collections::HashMap<String, String>,
    key: &str,
    fallback: usize,
) -> usize {
    values
        .get(key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(fallback)
}

fn candidate_attempt_order<'a>(
    job: &JobRecord,
    selected: &'a [MediaCandidate],
    all: &'a [MediaCandidate],
) -> Vec<&'a MediaCandidate> {
    let mut out = selected.iter().collect::<Vec<_>>();
    if selected.len() == 1 {
        let selected_id = selected[0].id;
        let selected_kind = selected[0].kind;
        out.extend(
            all.iter()
                .filter(|candidate| candidate.id != selected_id)
                .filter(|candidate| candidate_not_selectable_reason(candidate).is_none())
                .filter(|candidate| {
                    candidate.kind == selected_kind || is_compatible_fallback(job, candidate)
                }),
        );
    }
    out.sort_by_key(|candidate| -candidate_attempt_rank(job, candidate));
    out
}

fn is_compatible_fallback(job: &JobRecord, candidate: &MediaCandidate) -> bool {
    if job.outputs.contains(&OutputKind::Audio) && !job.outputs.contains(&OutputKind::Video) {
        return matches!(
            candidate.kind,
            CandidateKind::Audio | CandidateKind::Video | CandidateKind::Manifest
        );
    }
    if job.outputs.contains(&OutputKind::Video) {
        return matches!(
            candidate.kind,
            CandidateKind::Video | CandidateKind::Manifest
        );
    }
    job.outputs.contains(&OutputKind::Image) && candidate.kind == CandidateKind::Image
}

fn candidate_attempt_rank(job: &JobRecord, candidate: &MediaCandidate) -> i64 {
    let mut rank = candidate.score;
    if job.selected_candidate_ids.contains(&candidate.id) {
        rank += 10_000;
    }

    if job.outputs.contains(&OutputKind::Audio) && !job.outputs.contains(&OutputKind::Video) {
        rank += match candidate.kind {
            CandidateKind::Audio => 2_000,
            CandidateKind::Manifest => 1_000,
            CandidateKind::Video => 700,
            _ => -3_000,
        };
    }

    if job.outputs.contains(&OutputKind::Video) {
        rank += match candidate.kind {
            CandidateKind::Video => 2_000,
            CandidateKind::Manifest => 1_600,
            CandidateKind::Audio => -1_500,
            CandidateKind::Image => -2_000,
            _ => -3_000,
        };
    }

    rank += quality_preference_rank(&job.bitrate, candidate);
    if job.outputs.contains(&OutputKind::Video) {
        if candidate_needs_audio_companion(candidate) {
            rank += 120;
        }
        rank += mp4_compatibility_rank(candidate);
    }
    rank += audio_rank(candidate) / 1000;

    if candidate.requires_authorization {
        rank -= 100;
    }
    if let Some(reason) = candidate_not_selectable_reason(candidate) {
        rank -= match reason {
            "suspect_ad" => 5_000,
            "requires_drm" | "region_blocked" => 20_000,
            _ => 15_000,
        };
    }
    rank
}

fn candidate_not_selectable_reason(candidate: &MediaCandidate) -> Option<&'static str> {
    if candidate.failure_reason.is_some() {
        return Some("failed_validation");
    }
    match candidate.validation_state {
        Some(CandidateValidationState::Failed) => Some("failed_validation"),
        Some(CandidateValidationState::RegionBlocked) => Some("region_blocked"),
        Some(CandidateValidationState::Drm) => Some("requires_drm"),
        Some(CandidateValidationState::Expired) => Some("signed_url_expired"),
        Some(CandidateValidationState::SuspectAd) => Some("suspect_ad"),
        _ => match candidate.protection {
            Some(CandidateProtection::RegionBlocked) => Some("region_blocked"),
            Some(CandidateProtection::Drm) => Some("requires_drm"),
            _ if candidate.ad_risk || is_likely_ad_or_tracking_candidate(candidate) => {
                Some("suspect_ad")
            }
            _ => None,
        },
    }
}

fn quality_preference_rank(preference: &str, candidate: &MediaCandidate) -> i64 {
    let Some(height) = candidate_quality_height(candidate) else {
        return 0;
    };
    if preference == "auto" || !preference.ends_with('p') {
        return height;
    }
    let target = preference
        .trim_end_matches('p')
        .parse::<i64>()
        .unwrap_or_default();
    if target <= 0 {
        return height;
    }
    if height <= target {
        5_000 + height
    } else {
        2_000 - (height - target)
    }
}

fn candidate_quality_height(candidate: &MediaCandidate) -> Option<i64> {
    candidate
        .quality_label
        .as_deref()
        .and_then(|label| label.trim_end_matches('p').parse::<i64>().ok())
        .or_else(|| candidate_metadata_number(candidate, "height"))
}

fn mp4_compatibility_rank(candidate: &MediaCandidate) -> i64 {
    let mut rank = 0;
    let ext = candidate_metadata_text(candidate, "ext");
    let vcodec = candidate_metadata_text(candidate, "vcodec");
    let acodec = candidate_metadata_text(candidate, "acodec");
    let value = format!(
        "{} {} {} {} {}",
        candidate.content_type.as_deref().unwrap_or_default(),
        candidate.url,
        ext.as_deref().unwrap_or_default(),
        vcodec.as_deref().unwrap_or_default(),
        acodec.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();

    if value.contains("video/mp4") || value.contains(".mp4") || ext.as_deref() == Some("mp4") {
        rank += 300;
    }
    if value.contains("avc1") || value.contains("h264") {
        rank += 300;
    }
    if value.contains("mp4a") || value.contains("aac") {
        rank += 120;
    }
    if value.contains("video/webm")
        || value.contains(".webm")
        || value.contains("vp9")
        || value.contains("vp09")
        || value.contains("av01")
    {
        rank -= 700;
    }
    if value.contains("opus") || value.contains("vorbis") {
        rank -= 180;
    }
    rank
}

fn is_likely_ad_or_tracking_candidate(candidate: &MediaCandidate) -> bool {
    let value = format!(
        "{} {}",
        candidate.url.to_ascii_lowercase(),
        candidate
            .resource_type
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase()
    );
    [
        "trafficjunky",
        "doubleclick",
        "googlesyndication",
        "adservice",
        "pre-roll",
        "preroll",
        "vast",
        "vpaid",
        "tracking",
        "tracker",
        "pixel",
    ]
    .iter()
    .any(|needle| value.contains(needle))
}

fn friendly_candidate_failure(error: &RkError) -> String {
    let message = error.to_string();
    if message.contains("has no audio stream") {
        "没有音频流".to_string()
    } else if message.contains("has no video stream") {
        "没有视频流".to_string()
    } else if message.contains("matches no streams") {
        "没有匹配的媒体流".to_string()
    } else if message.len() > 180 {
        format!("{}...", &message[..180])
    } else {
        message
    }
}

fn should_build_image_slideshow(job: &JobRecord, selected: &[MediaCandidate]) -> bool {
    job.outputs.contains(&OutputKind::Video)
        && selected
            .iter()
            .any(|candidate| candidate.kind == CandidateKind::Image)
        && selected
            .iter()
            .all(|candidate| candidate.kind == CandidateKind::Image)
}

fn image_candidate_order(candidate: &MediaCandidate) -> i64 {
    candidate_metadata_number(candidate, "index")
        .or_else(|| {
            candidate
                .quality_label
                .as_deref()
                .and_then(|label| label.rsplit('-').next())
                .and_then(|value| value.parse::<i64>().ok())
        })
        .unwrap_or(candidate.score)
}

fn image_extension(content_type: Option<&str>, url: &str) -> &'static str {
    let content_type = content_type.unwrap_or_default().to_ascii_lowercase();
    if content_type.contains("png") || has_url_extension(url, ".png") {
        "png"
    } else if content_type.contains("webp") || has_url_extension(url, ".webp") {
        "webp"
    } else {
        "jpg"
    }
}

fn has_url_extension(url: &str, extension: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|url| {
            Path::new(url.path())
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| format!(".{}", value.to_ascii_lowercase()))
        })
        .as_deref()
        == Some(extension)
}

fn best_companion_audio<'a>(
    video_candidate: &MediaCandidate,
    candidates: &'a [MediaCandidate],
) -> Option<&'a MediaCandidate> {
    if !candidate_needs_audio_companion(video_candidate) {
        return None;
    }

    candidates
        .iter()
        .filter(|candidate| candidate.kind == CandidateKind::Audio)
        .filter(|candidate| same_candidate_family(video_candidate, candidate))
        .max_by_key(|candidate| audio_rank(candidate))
}

fn candidate_needs_audio_companion(candidate: &MediaCandidate) -> bool {
    if candidate.kind != CandidateKind::Video {
        return false;
    }
    let vcodec = candidate_metadata_text(candidate, "vcodec");
    let acodec = candidate_metadata_text(candidate, "acodec");
    if codec_present(vcodec.as_deref()) && !codec_present(acodec.as_deref()) {
        return true;
    }

    let value = format!(
        "{} {} {}",
        candidate.url,
        candidate.resource_type.as_deref().unwrap_or_default(),
        candidate.quality_label.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    value.contains("bilibili")
        || value.contains(".m4s")
        || value.contains("dash")
        || value.contains("video-only")
}

fn codec_present(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim).filter(|value| !value.is_empty()),
        Some(value) if value != "none" && value != "null" && value != "unknown"
    )
}

fn same_candidate_family(left: &MediaCandidate, right: &MediaCandidate) -> bool {
    if left.extractor != right.extractor {
        return false;
    }

    if left.resource_type.as_deref() == Some("bilibili_playinfo")
        || left.resource_type.as_deref() == Some("bilibili_api")
    {
        return matches!(
            right.resource_type.as_deref(),
            Some("bilibili_playinfo") | Some("bilibili_api")
        );
    }

    left.initiator_url == right.initiator_url
}

fn audio_rank(candidate: &MediaCandidate) -> i64 {
    candidate_metadata_number(candidate, "bandwidth")
        .unwrap_or_else(|| audio_quality_id(candidate).unwrap_or(candidate.score))
}

fn audio_quality_id(candidate: &MediaCandidate) -> Option<i64> {
    candidate
        .quality_label
        .as_deref()
        .and_then(|label| label.rsplit('-').next())
        .and_then(|value| value.parse::<i64>().ok())
}

fn candidate_metadata_number(candidate: &MediaCandidate, key: &str) -> Option<i64> {
    candidate
        .metadata_json
        .get(key)
        .or_else(|| {
            candidate
                .metadata_json
                .get("candidate")
                .and_then(|metadata| metadata.get(key))
        })
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|number| number as i64))
        })
}

fn candidate_metadata_text(candidate: &MediaCandidate, key: &str) -> Option<String> {
    candidate
        .metadata_json
        .get(key)
        .or_else(|| {
            candidate
                .metadata_json
                .get("candidate")
                .and_then(|metadata| metadata.get(key))
        })
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reflection_core::models::{
        CandidateProtection, CandidateValidationState, JobCreateOptions,
    };
    use serde_json::json;

    fn candidate(
        job_id: Uuid,
        kind: CandidateKind,
        url: &str,
        quality: &str,
        metadata_json: serde_json::Value,
    ) -> MediaCandidate {
        MediaCandidate {
            id: Uuid::new_v4(),
            job_id,
            url: url.to_string(),
            kind,
            extractor: "yt_dlp".to_string(),
            method: "dump_single_json".to_string(),
            status: None,
            content_type: Some(
                match kind {
                    CandidateKind::Audio => "audio/mp4",
                    _ => "video/mp4",
                }
                .to_string(),
            ),
            content_length: None,
            resource_type: Some("https".to_string()),
            initiator_url: Some("https://example.com/watch".to_string()),
            quality_label: Some(quality.to_string()),
            score: 100,
            requires_authorization: false,
            platform: None,
            route: Some("external:yt_dlp".to_string()),
            extractor_confidence: Some(80),
            protection: Some(CandidateProtection::None),
            requires_profile: false,
            ttl_hint_seconds: None,
            ad_risk: false,
            evidence_count: 1,
            paired_candidate_ids: Vec::new(),
            failure_reason: None,
            validation_state: Some(CandidateValidationState::Untested),
            metadata_json,
            created_at: OffsetDateTime::now_utc(),
            score_breakdown_json: json!({}),
            selected: false,
            selection_reason: None,
            validation_status: None,
            resolved_ip: None,
            final_url_after_redirects: None,
            expires_at: None,
            discovered_by_event_id: None,
        }
    }

    #[test]
    fn video_only_metadata_requires_companion_audio() {
        let job_id = Uuid::new_v4();
        let video = candidate(
            job_id,
            CandidateKind::Video,
            "https://cdn.example.com/v1080.mp4",
            "1080p",
            json!({ "vcodec": "avc1.640028", "acodec": "none", "height": 1080 }),
        );
        let audio = candidate(
            job_id,
            CandidateKind::Audio,
            "https://cdn.example.com/a.m4a",
            "audio-140",
            json!({ "vcodec": "none", "acodec": "mp4a.40.2", "abr": 128 }),
        );

        assert!(candidate_needs_audio_companion(&video));
        assert_eq!(
            best_companion_audio(&video, std::slice::from_ref(&audio)).map(|c| c.id),
            Some(audio.id)
        );
    }

    #[test]
    fn selected_video_only_candidate_keeps_companion_family_available() {
        let mut job = JobRecord::new_with_options(
            "https://example.com/watch".to_string(),
            "auto".to_string(),
            "http://127.0.0.1:8787",
            JobCreateOptions {
                discovery: DiscoveryMode::External,
                outputs: vec![OutputKind::Video, OutputKind::Audio],
                ..JobCreateOptions::default()
            },
        );
        let video = candidate(
            job.id,
            CandidateKind::Video,
            "https://cdn.example.com/v1080.mp4",
            "1080p",
            json!({ "vcodec": "avc1.640028", "acodec": "none", "height": 1080 }),
        );
        let audio = candidate(
            job.id,
            CandidateKind::Audio,
            "https://cdn.example.com/a.m4a",
            "audio-140",
            json!({ "vcodec": "none", "acodec": "mp4a.40.2", "abr": 128 }),
        );
        job.selected_candidate_ids = vec![video.id];

        let all = vec![video.clone(), audio.clone()];
        let selected = vec![video.clone()];
        let attempts = candidate_attempt_order(&job, &selected, &all);

        assert_eq!(
            attempts.first().map(|candidate| candidate.id),
            Some(video.id)
        );
        assert_eq!(
            best_companion_audio(attempts[0], &all).map(|c| c.id),
            Some(audio.id)
        );
    }

    #[test]
    fn failed_or_blocked_candidates_are_not_selectable() {
        let job_id = Uuid::new_v4();
        let mut failed = candidate(
            job_id,
            CandidateKind::Manifest,
            "https://cdn.example.com/bad.m3u8",
            "720p",
            json!({}),
        );
        failed.failure_reason = Some("segment returned 403".to_string());
        assert_eq!(
            candidate_not_selectable_reason(&failed),
            Some("failed_validation")
        );

        let mut blocked = candidate(
            job_id,
            CandidateKind::Manifest,
            "https://cdn.example.com/blocked.m3u8",
            "720p",
            json!({}),
        );
        blocked.protection = Some(CandidateProtection::RegionBlocked);
        blocked.validation_state = Some(CandidateValidationState::RegionBlocked);
        assert_eq!(
            candidate_not_selectable_reason(&blocked),
            Some("region_blocked")
        );

        let mut ad = candidate(
            job_id,
            CandidateKind::Video,
            "https://cdn.example.com/vast/preroll.mp4",
            "720p",
            json!({}),
        );
        ad.ad_risk = true;
        assert_eq!(candidate_not_selectable_reason(&ad), Some("suspect_ad"));
    }

    #[test]
    fn fallback_attempts_skip_failed_candidates() {
        let mut job = JobRecord::new_with_options(
            "https://example.com/watch".to_string(),
            "auto".to_string(),
            "http://127.0.0.1:8787",
            JobCreateOptions {
                discovery: DiscoveryMode::External,
                outputs: vec![OutputKind::Video],
                ..JobCreateOptions::default()
            },
        );
        let selected = candidate(
            job.id,
            CandidateKind::Manifest,
            "https://cdn.example.com/selected.m3u8",
            "720p",
            json!({ "height": 720 }),
        );
        let mut failed_fallback = candidate(
            job.id,
            CandidateKind::Manifest,
            "https://cdn.example.com/blocked.m3u8",
            "1080p",
            json!({ "height": 1080 }),
        );
        failed_fallback.failure_reason = Some("segment returned 403".to_string());
        let good_fallback = candidate(
            job.id,
            CandidateKind::Manifest,
            "https://cdn.example.com/good.m3u8",
            "480p",
            json!({ "height": 480 }),
        );
        job.selected_candidate_ids = vec![selected.id];

        let all = vec![
            selected.clone(),
            failed_fallback.clone(),
            good_fallback.clone(),
        ];
        let attempts = candidate_attempt_order(&job, std::slice::from_ref(&selected), &all);
        let attempt_ids = attempts
            .iter()
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>();

        assert!(attempt_ids.contains(&selected.id));
        assert!(attempt_ids.contains(&good_fallback.id));
        assert!(!attempt_ids.contains(&failed_fallback.id));
    }

    #[test]
    fn safe_candidate_download_headers_drops_sensitive_values() {
        let candidate = candidate(
            Uuid::new_v4(),
            CandidateKind::Video,
            "https://cdn.example.com/video.mp4",
            "720p",
            json!({
                "download_headers": {
                    "User-Agent": "Mozilla/5.0",
                    "Referer": "https://example.com/watch",
                    "Cookie": "secret=redacted",
                    "Authorization": "Bearer redacted",
                    "X-Test": "nope"
                }
            }),
        );

        let headers = safe_candidate_download_headers(&candidate);

        assert_eq!(
            headers
                .get(reqwest::header::USER_AGENT)
                .and_then(|value| value.to_str().ok()),
            Some("Mozilla/5.0")
        );
        assert_eq!(
            headers
                .get(reqwest::header::REFERER)
                .and_then(|value| value.to_str().ok()),
            Some("https://example.com/watch")
        );
        assert!(!headers.contains_key(reqwest::header::COOKIE));
        assert!(!headers.contains_key(reqwest::header::AUTHORIZATION));
        assert!(!headers.contains_key("x-test"));
    }

    #[test]
    fn yt_dlp_max_filesize_uses_cli_size_suffix() {
        assert_eq!(yt_dlp_max_filesize(314_572_800), "300M");
        assert_eq!(yt_dlp_max_filesize(1), "1M");
    }
}
