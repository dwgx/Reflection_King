use std::sync::Arc;

use reflection_core::{
    browser_probe::BrowserProbeClient,
    download::Downloader,
    external_probe::YtDlpProbe,
    job_store::JobStore,
    models::{
        ArtifactView, CandidateKind, DiscoveryMode, JobRecord, JobStatus, JobView, MediaCandidate,
        OutputKind,
    },
    paths::StoragePaths,
    transcode::Transcoder,
    AppConfig, Result, RkError,
};
use time::OffsetDateTime;
use tokio::sync::{mpsc, Mutex, Semaphore};
use tracing::{debug, error, info};
use uuid::Uuid;

pub struct AppState {
    pub config: AppConfig,
    pub paths: StoragePaths,
    job_store: JobStore,
    browser_probe: Option<BrowserProbeClient>,
    yt_dlp_probe: Option<YtDlpProbe>,
    queue_tx: mpsc::Sender<Uuid>,
    queue_rx: Mutex<mpsc::Receiver<Uuid>>,
    worker_slots: Arc<Semaphore>,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self> {
        let paths = StoragePaths::new(config.storage_dir.clone());
        paths.ensure().await?;
        let job_store = JobStore::connect(&paths.database_path()).await?;
        let browser_probe = config
            .browser_probe_url
            .as_ref()
            .map(|url| BrowserProbeClient::new(url, config.browser_probe_timeout))
            .transpose()?;
        let yt_dlp_probe = config
            .yt_dlp_path
            .clone()
            .map(|path| YtDlpProbe::new(path, config.yt_dlp_timeout, config.yt_dlp_max_json_bytes));

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

    pub async fn list_candidates(&self, id: Uuid) -> Result<Vec<MediaCandidate>> {
        self.job_store.list_candidates(id).await
    }

    pub fn browser_probe_configured(&self) -> bool {
        self.browser_probe.is_some()
    }

    pub fn yt_dlp_configured(&self) -> bool {
        self.yt_dlp_probe.is_some()
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
                    state.mark_error(job_id, error.to_string()).await;
                }
            });
        }
    }

    async fn process_job(&self, job_id: Uuid) -> Result<()> {
        debug!(%job_id, "starting job");

        let job = self
            .get_job(job_id)
            .await
            .and_then(|job| job.ok_or_else(|| RkError::NotFound(format!("job {job_id}"))))?;

        if job.status == JobStatus::CandidateSelected {
            return self.process_selected_candidates(job).await;
        }

        match job.discovery {
            DiscoveryMode::External => return self.resolve_external_candidates(job).await,
            DiscoveryMode::Browser => return self.resolve_browser_candidates(job).await,
            DiscoveryMode::Auto => {
                if self.yt_dlp_probe.is_some() {
                    match self.resolve_external_candidates(job.clone()).await {
                        Ok(()) => return Ok(()),
                        Err(error) => {
                            info!(%job_id, "external discovery failed, falling back to browser: {error}");
                        }
                    }
                }
                return self.resolve_browser_candidates(job).await;
            }
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

        tokio::fs::remove_dir_all(&job_paths.temp_dir).await.ok();
        self.mark_ready(
            job_id,
            format!("{}/media/{job_id}/audio.mp3", self.config.public_base_url),
        )
        .await?;

        Ok(())
    }

    async fn resolve_external_candidates(&self, job: JobRecord) -> Result<()> {
        let Some(yt_dlp_probe) = &self.yt_dlp_probe else {
            return Err(RkError::Source(
                "RK_YTDLP_PATH is required for external discovery".to_string(),
            ));
        };

        self.update_status(job.id, JobStatus::Resolving).await?;
        let candidates = yt_dlp_probe
            .probe(job.id, &job.source_url, &job.outputs)
            .await?;

        if candidates.is_empty() {
            return Err(RkError::Source(
                "yt-dlp did not find usable media candidates".to_string(),
            ));
        }

        self.job_store
            .replace_candidates(job.id, &candidates)
            .await?;
        self.update_status(job.id, JobStatus::CandidatesReady)
            .await?;
        Ok(())
    }

    async fn resolve_browser_candidates(&self, job: JobRecord) -> Result<()> {
        let Some(browser_probe) = &self.browser_probe else {
            return Err(RkError::Browser(
                "RK_BROWSER_PROBE_URL is required for browser discovery".to_string(),
            ));
        };
        self.update_status(job.id, JobStatus::Resolving).await?;
        let output_names = job
            .outputs
            .iter()
            .map(|output| output.as_str().to_string())
            .collect::<Vec<_>>();
        let candidates = browser_probe
            .probe(
                job.id,
                &job.source_url,
                &job.profile_id,
                job.platform_hint,
                &output_names,
            )
            .await?;

        if candidates.is_empty() {
            return Err(RkError::Source(
                "browser probe did not find media candidates".to_string(),
            ));
        }

        self.job_store
            .replace_candidates(job.id, &candidates)
            .await?;
        self.update_status(job.id, JobStatus::CandidatesReady)
            .await?;
        Ok(())
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

        for candidate in &selected {
            self.process_candidate(&job, candidate, &candidates).await?;
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
                    self.update_status(job.id, JobStatus::Transcoding).await?;
                    let output_path = job_dir.join(format!("audio-{}.mp3", candidate.id));
                    Transcoder::new(self.config.ffmpeg_path.clone())
                        .media_url_to_mp3_with_headers(
                            &candidate.url,
                            &output_path,
                            &job.bitrate,
                            &headers,
                        )
                        .await?;
                    self.insert_artifact(job.id, OutputKind::Audio, output_path, "audio/mpeg")
                        .await?;
                } else {
                    self.update_status(job.id, JobStatus::Remuxing).await?;
                    let output_path = job_dir.join(format!("video-{}.mp4", candidate.id));
                    if let Some(audio_candidate) =
                        best_companion_audio(candidate, available_candidates)
                    {
                        let audio_headers = self.download_headers(job, audio_candidate).await?;
                        Transcoder::new(self.config.ffmpeg_path.clone())
                            .media_urls_to_mp4_with_headers(
                                &candidate.url,
                                &headers,
                                &audio_candidate.url,
                                &audio_headers,
                                &output_path,
                            )
                            .await?;
                    } else {
                        Transcoder::new(self.config.ffmpeg_path.clone())
                            .media_url_to_mp4_with_headers(&candidate.url, &output_path, &headers)
                            .await?;
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
    ) -> Result<reqwest::header::HeaderMap> {
        if !candidate.requires_authorization && candidate.extractor != "browser_probe" {
            return Ok(reqwest::header::HeaderMap::new());
        }
        let Some(browser_probe) = &self.browser_probe else {
            return Ok(reqwest::header::HeaderMap::new());
        };
        browser_probe
            .headers_for_url(
                &job.profile_id,
                &candidate.url,
                candidate.initiator_url.as_deref(),
            )
            .await
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
        self.job_store.update_status(job_id, status).await
    }

    async fn mark_ready(&self, job_id: Uuid, media_url: String) -> Result<()> {
        self.job_store.mark_ready(job_id, &media_url).await
    }

    async fn mark_error(&self, job_id: Uuid, error: String) {
        if let Err(store_error) = self.job_store.mark_error(job_id, &error).await {
            error!(%job_id, "failed to persist job error: {store_error}");
        }
    }
}

fn validate_candidate_url(url: &str) -> Result<()> {
    reflection_core::url_policy::parse_and_validate_url(url).map(|_| ())
}

fn best_companion_audio<'a>(
    video_candidate: &MediaCandidate,
    candidates: &'a [MediaCandidate],
) -> Option<&'a MediaCandidate> {
    if video_candidate.kind != CandidateKind::Video {
        return None;
    }

    candidates
        .iter()
        .filter(|candidate| candidate.kind == CandidateKind::Audio)
        .filter(|candidate| same_candidate_family(video_candidate, candidate))
        .max_by_key(|candidate| audio_rank(candidate))
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
        .get("candidate")
        .and_then(|metadata| metadata.get(key))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|number| number as i64))
        })
}
