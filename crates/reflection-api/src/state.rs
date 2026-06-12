use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use reflection_core::{
    browser_probe::{BrowserProbeClient, LoginSessionSnapshot},
    download::Downloader,
    external_probe::YtDlpProbe,
    external_tools::{ExternalToolKind, ExternalToolProbe},
    extractors::{ExtractContext, SourceResolver},
    job_store::JobStore,
    models::{
        ApiKeyRecord, ApiKeyView, ArtifactView, CandidateKind, ClearJobsResponse,
        CreateUserKeyRequest, CreatedUserKeyResponse, DiscoveryMode, HiddenJobBatchView, JobRecord,
        JobStatus, JobView, MediaCandidate, OutputKind, RestoreJobsResponse,
        RotatedAdminKeyResponse,
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
            .map(|url| BrowserProbeClient::new(url, config.browser_probe_timeout))
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

    pub async fn rotate_admin_key(&self) -> Result<RotatedAdminKeyResponse> {
        self.job_store.rotate_admin_key().await
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
            self.job_store
                .get_candidate(job_id, *candidate_id)
                .await?
                .ok_or_else(|| RkError::NotFound(format!("candidate {candidate_id}")))?;
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
                    state.mark_error(job_id, error.to_string(), class).await;
                }
            });
        }
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

        match job.discovery {
            // External/Browser/Auto all run through the resolver chain; the
            // chain composition is chosen from the discovery mode.
            DiscoveryMode::External | DiscoveryMode::Browser | DiscoveryMode::Auto => {
                return self.resolve_candidates(job).await
            }
            // Direct keeps the fast immediate download+transcode path below.
            DiscoveryMode::Direct => {}
        }

        let job_paths = self.paths.prepare_job(job_id).await?;

        self.update_status(job_id, JobStatus::Downloading).await?;
        let downloader =
            Downloader::new(self.config.download_timeout, self.config.max_download_bytes)?;
        downloader
            .download_to_file(&job.source_url, &job_paths.input_path)
            .await?;

        self.update_status(job_id, JobStatus::Transcoding).await?;
        let transcoder = Transcoder::new(self.config.ffmpeg_path.clone());
        transcoder
            .audio_to_mp3(&job_paths.input_path, &job_paths.output_path, &job.bitrate)
            .await?;
        self.insert_artifact(
            job_id,
            OutputKind::Audio,
            job_paths.output_path,
            "audio/mpeg",
        )
        .await?;

        tokio::fs::remove_dir_all(&job_paths.temp_dir).await.ok();
        self.mark_ready(
            job_id,
            format!("{}/media/{job_id}/audio.mp3", self.config.public_base_url),
        )
        .await?;

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
            yt_dlp: self.yt_dlp_probe.clone(),
            external_tools: self.external_tool_probes.clone(),
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

        if outcome.candidates.is_empty() {
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
        self.record_candidate_summary(job.id, &winner, &outcome.candidates)
            .await;
        self.job_store
            .replace_candidates(job.id, &outcome.candidates)
            .await?;
        self.update_status(job.id, JobStatus::CandidatesReady)
            .await?;
        Ok(())
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

        if should_build_image_slideshow(&job, &selected) {
            self.process_image_slideshow(&job, &selected).await?;
        } else {
            let attempts = candidate_attempt_order(&job, &selected, &candidates);
            let mut failures = Vec::new();
            let mut success_count = 0usize;

            for candidate in attempts {
                match self.process_candidate(&job, candidate, &candidates).await {
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
                format!(
                    "{}/api/jobs/{}/artifacts",
                    self.config.public_base_url, job.id
                )
            });
        self.mark_ready(job.id, media_url).await?;
        Ok(())
    }

    async fn process_image_slideshow(
        &self,
        job: &JobRecord,
        candidates: &[MediaCandidate],
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
        let downloader =
            Downloader::new(self.config.download_timeout, self.config.max_download_bytes)?;
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
        Transcoder::new(self.config.ffmpeg_path.clone())
            .images_to_mp4(&list_path, &output_path, 1920, 1080)
            .await?;
        self.insert_artifact(job.id, OutputKind::Video, output_path, "video/mp4")
            .await?;

        tokio::fs::remove_dir_all(&temp_dir).await.ok();
        Ok(())
    }

    async fn process_candidate(
        &self,
        job: &JobRecord,
        candidate: &MediaCandidate,
        available_candidates: &[MediaCandidate],
    ) -> Result<()> {
        validate_candidate_url(&candidate.url)?;
        if candidate.extractor == "yt_dlp" && candidate.requires_authorization {
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
                    Downloader::new(self.config.download_timeout, self.config.max_download_bytes)?;
                downloader
                    .download_to_file_with_headers(&candidate.url, &input_path, headers)
                    .await?;

                self.update_status(job.id, JobStatus::Transcoding).await?;
                let output_path = job_dir.join(format!("audio-{}.mp3", candidate.id));
                Transcoder::new(self.config.ffmpeg_path.clone())
                    .audio_to_mp3(&input_path, &output_path, &job.bitrate)
                    .await?;
                self.insert_artifact(job.id, OutputKind::Audio, output_path, "audio/mpeg")
                    .await?;
            }
            CandidateKind::Video | CandidateKind::Manifest => {
                let headers = self.download_headers(job, candidate).await?;
                if job.outputs.contains(&OutputKind::Audio)
                    && !job.outputs.contains(&OutputKind::Video)
                {
                    let output_path = job_dir.join(format!("audio-{}.mp3", candidate.id));
                    let raw_result = async {
                        let transcoder = Transcoder::new(self.config.ffmpeg_path.clone());
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
                    self.insert_artifact(job.id, OutputKind::Audio, output_path, "audio/mpeg")
                        .await?;
                } else {
                    let output_path = job_dir.join(format!("video-{}.mp4", candidate.id));
                    let raw_result = async {
                        let transcoder = Transcoder::new(self.config.ffmpeg_path.clone());
                        let stream_info = transcoder
                            .probe_url_with_headers(&candidate.url, &headers)
                            .await?;
                        if !stream_info.has_video {
                            return Err(RkError::Source(
                                "candidate has no video stream".to_string(),
                            ));
                        }
                        self.update_status(job.id, JobStatus::Remuxing).await?;
                        let audio_candidate = best_companion_audio(candidate, available_candidates);
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
                            let audio_headers = self.download_headers(job, audio_candidate).await?;
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
                    self.insert_artifact(job.id, OutputKind::Video, output_path, "video/mp4")
                        .await?;
                }
            }
            CandidateKind::Image => {
                self.update_status(job.id, JobStatus::Downloading).await?;
                let output_path = job_dir.join(format!("image-{}.bin", candidate.id));
                let headers = self.download_headers(job, candidate).await?;
                Downloader::new(self.config.download_timeout, self.config.max_download_bytes)?
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
                )
                .await?;
            }
            CandidateKind::Html | CandidateKind::Unknown => {}
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
        let profile_headers = browser_probe
            .headers_for_url(
                &job.profile_id,
                &candidate.url,
                candidate.initiator_url.as_deref(),
            )
            .await?;
        headers.extend(profile_headers);
        Ok(headers)
    }

    fn should_try_yt_dlp_delegated_download(&self, candidate: &MediaCandidate) -> bool {
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
    ) -> Result<()> {
        let source_url = candidate
            .initiator_url
            .as_deref()
            .unwrap_or(job.source_url.as_str());
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
            .arg(format!("{}B", self.config.max_download_bytes))
            .arg("-o")
            .arg(&output_template);
        if let Some(format_id) = candidate_metadata_text(candidate, "format_id") {
            command.arg("-f").arg(format_id);
        }
        command.arg(source_url);

        let output = tokio_time::timeout(self.config.download_timeout, command.output())
            .await
            .map_err(|_| RkError::Source("yt-dlp delegated download timed out".to_string()))??;
        if !output.status.success() {
            return Err(RkError::Source(format!(
                "yt-dlp delegated download exited with {}: {}",
                output
                    .status
                    .code()
                    .map_or_else(|| "signal".to_string(), |code| code.to_string()),
                limited_process_stderr(&output.stderr)
            )));
        }

        let downloaded = find_delegated_download(temp_dir, candidate.id)?;
        let bytes = tokio::fs::metadata(&downloaded).await?.len();
        if bytes > self.config.max_download_bytes {
            return Err(RkError::DownloadTooLarge {
                max_bytes: self.config.max_download_bytes,
            });
        }

        match output_kind {
            OutputKind::Audio => {
                self.update_status(job.id, JobStatus::Transcoding).await?;
                Transcoder::new(self.config.ffmpeg_path.clone())
                    .audio_to_mp3(&downloaded, output_path, &job.bitrate)
                    .await
            }
            OutputKind::Video => {
                self.update_status(job.id, JobStatus::Remuxing).await?;
                Transcoder::new(self.config.ffmpeg_path.clone())
                    .media_to_mp4(&downloaded, output_path)
                    .await
            }
            _ => Err(RkError::Source(
                "yt-dlp delegated download only supports audio or video outputs".to_string(),
            )),
        }
    }

    async fn insert_artifact(
        &self,
        job_id: Uuid,
        kind: OutputKind,
        path: std::path::PathBuf,
        content_type: &str,
    ) -> Result<()> {
        let bytes = tokio::fs::metadata(&path).await?.len() as i64;
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| RkError::Source("invalid artifact filename".to_string()))?;
        let artifact = ArtifactView {
            id: Uuid::new_v4(),
            job_id,
            kind,
            path: path.display().to_string(),
            media_url: format!("{}/media/{job_id}/{filename}", self.config.public_base_url),
            content_type: content_type.to_string(),
            bytes,
            created_at: OffsetDateTime::now_utc(),
        };
        self.job_store.insert_artifact(&artifact).await
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

fn validate_candidate_url(url: &str) -> Result<()> {
    reflection_core::url_policy::parse_and_validate_url(url).map(|_| ())
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
    if is_likely_ad_or_tracking_candidate(candidate) {
        rank -= 5_000;
    }
    rank
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
}
