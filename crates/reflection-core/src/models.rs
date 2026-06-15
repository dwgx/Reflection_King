use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::observability::ErrorClass;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Resolving,
    CandidatesReady,
    NeedsProfile,
    CandidateSelected,
    Downloading,
    Capturing,
    Probing,
    Transcoding,
    Remuxing,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobRequest {
    pub url: String,
    pub bitrate: Option<String>,
    pub discovery: Option<DiscoveryMode>,
    pub platform_hint: Option<PlatformHint>,
    pub outputs: Option<Vec<OutputKind>>,
    pub profile_id: Option<String>,
    pub auth_mode: Option<AuthMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyRole {
    User,
    Admin,
}

impl ApiKeyRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Admin => "admin",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApiKeyRecord {
    pub id: Uuid,
    pub label: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub role: ApiKeyRole,
    pub max_download_bytes: Option<u64>,
    pub allow_browser_probe: bool,
    pub allow_ytdlp: bool,
    pub allow_external_adapters: bool,
    pub allow_login_profile: bool,
    pub created_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyView {
    pub id: Uuid,
    pub label: String,
    pub key_prefix: String,
    pub role: ApiKeyRole,
    pub max_download_bytes: Option<u64>,
    pub allow_browser_probe: bool,
    pub allow_ytdlp: bool,
    pub allow_external_adapters: bool,
    pub allow_login_profile: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserKeyRequest {
    pub label: Option<String>,
    pub key: Option<String>,
    pub max_download_mb: Option<u64>,
    pub allow_browser_probe: bool,
    pub allow_ytdlp: bool,
    #[serde(default)]
    pub allow_external_adapters: bool,
    #[serde(default)]
    pub allow_login_profile: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedUserKeyResponse {
    pub key: String,
    pub record: ApiKeyView,
}

#[derive(Debug, Clone, Serialize)]
pub struct RotatedAdminKeyResponse {
    pub key: String,
    pub record: ApiKeyView,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSettingsView {
    pub public_base_url: String,
    pub max_download_bytes: u64,
    pub max_concurrent_jobs: usize,
    pub download_timeout_seconds: u64,
    pub browser_probe_timeout_seconds: u64,
    pub yt_dlp_timeout_seconds: u64,
    pub yt_dlp_max_json_bytes: usize,
    pub job_ttl_hours: u64,
    pub page_archive_max_resources: usize,
    pub page_archive_max_resource_bytes: u64,
    pub page_archive_max_total_bytes: u64,
    pub ffmpeg_path: String,
    pub browser_probe_url: Option<String>,
    pub yt_dlp_path: Option<String>,
    pub you_get_path: Option<String>,
    pub lux_path: Option<String>,
    pub streamlink_path: Option<String>,
    pub external_probe_timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRuntimeSettingsRequest {
    pub public_base_url: Option<String>,
    pub max_download_mb: Option<u64>,
    pub download_timeout_seconds: Option<u64>,
    pub yt_dlp_timeout_seconds: Option<u64>,
    pub yt_dlp_max_json_mb: Option<usize>,
    pub job_ttl_hours: Option<u64>,
    pub page_archive_max_resources: Option<usize>,
    pub page_archive_max_resource_mb: Option<u64>,
    pub page_archive_max_total_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HiddenJobBatchView {
    pub id: Uuid,
    pub actor_key_id: Option<Uuid>,
    pub actor_label: Option<String>,
    pub hidden_count: i64,
    pub restored_count: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub restored_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClearJobsResponse {
    pub batch_id: Option<Uuid>,
    pub hidden: u64,
    pub history_deleted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreJobsResponse {
    pub batch_id: Option<Uuid>,
    pub restored: u64,
    pub history_deleted: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobIssueKind {
    None,
    Failed,
    NeedsProfile,
    Unsupported,
    TooLarge,
    Timeout,
    PolicyBlocked,
}

impl JobIssueKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Failed => "failed",
            Self::NeedsProfile => "needs_profile",
            Self::Unsupported => "unsupported",
            Self::TooLarge => "too_large",
            Self::Timeout => "timeout",
            Self::PolicyBlocked => "policy_blocked",
        }
    }
}

impl From<ApiKeyRecord> for ApiKeyView {
    fn from(value: ApiKeyRecord) -> Self {
        Self {
            id: value.id,
            label: value.label,
            key_prefix: value.key_prefix,
            role: value.role,
            max_download_bytes: value.max_download_bytes,
            allow_browser_probe: value.allow_browser_probe,
            allow_ytdlp: value.allow_ytdlp,
            allow_external_adapters: value.allow_external_adapters,
            allow_login_profile: value.allow_login_profile,
            created_at: value.created_at,
            revoked_at: value.revoked_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobView {
    pub id: Uuid,
    pub status: JobStatus,
    pub source_url: String,
    pub bitrate: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    pub status_url: String,
    pub media_url: Option<String>,
    pub artifacts_url: String,
    pub candidates_url: String,
    pub error: Option<String>,
    pub discovery: DiscoveryMode,
    pub platform_hint: PlatformHint,
    pub outputs: Vec<OutputKind>,
    pub profile_id: String,
    pub auth_mode: AuthMode,
    pub trace_url: String,
    pub requester_ip: Option<String>,
    pub requester_user_agent: Option<String>,
    pub requester_label: Option<String>,
    pub requester_key_id: Option<Uuid>,
    pub resolved_extractor: Option<String>,
    pub error_class: ErrorClass,
    pub issue_kind: JobIssueKind,
    pub issue_label: String,
    pub issue_detail: Option<String>,
    pub profile_action_url: Option<String>,
    pub attempt_count: i64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub started_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub completed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct JobRecord {
    pub id: Uuid,
    pub status: JobStatus,
    pub source_url: String,
    pub bitrate: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub status_url: String,
    pub media_url: Option<String>,
    pub error: Option<String>,
    pub discovery: DiscoveryMode,
    pub platform_hint: PlatformHint,
    pub outputs: Vec<OutputKind>,
    pub profile_id: String,
    pub auth_mode: AuthMode,
    pub selected_candidate_ids: Vec<Uuid>,
    /// Who created the job (observability: "who / what IP / what browser").
    pub requester_ip: Option<String>,
    pub requester_user_agent: Option<String>,
    pub requester_label: Option<String>,
    pub requester_key_id: Option<Uuid>,
    /// The extractor chain that actually produced the result, e.g.
    /// "street_voice>browser_probe".
    pub resolved_extractor: Option<String>,
    pub error_class: ErrorClass,
    pub attempt_count: i64,
    pub started_at: Option<OffsetDateTime>,
    pub completed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct JobCreateOptions {
    pub discovery: DiscoveryMode,
    pub platform_hint: PlatformHint,
    pub outputs: Vec<OutputKind>,
    pub profile_id: String,
    pub auth_mode: AuthMode,
}

impl Default for JobCreateOptions {
    fn default() -> Self {
        Self {
            discovery: DiscoveryMode::Direct,
            platform_hint: PlatformHint::Auto,
            outputs: vec![OutputKind::Video, OutputKind::Audio],
            profile_id: "admin_default".to_string(),
            auth_mode: AuthMode::None,
        }
    }
}

impl JobRecord {
    pub fn new(source_url: String, bitrate: String, public_base_url: &str) -> Self {
        Self::new_with_options(
            source_url,
            bitrate,
            public_base_url,
            JobCreateOptions::default(),
        )
    }

    pub fn new_with_options(
        source_url: String,
        bitrate: String,
        public_base_url: &str,
        options: JobCreateOptions,
    ) -> Self {
        let id = Uuid::new_v4();
        let now = OffsetDateTime::now_utc();
        Self {
            id,
            status: JobStatus::Queued,
            source_url,
            bitrate,
            created_at: now,
            updated_at: now,
            status_url: format!("{public_base_url}/api/jobs/{id}"),
            media_url: None,
            error: None,
            discovery: options.discovery,
            platform_hint: options.platform_hint,
            outputs: options.outputs,
            profile_id: options.profile_id,
            auth_mode: options.auth_mode,
            selected_candidate_ids: Vec::new(),
            requester_ip: None,
            requester_user_agent: None,
            requester_label: None,
            requester_key_id: None,
            resolved_extractor: None,
            error_class: ErrorClass::None,
            attempt_count: 0,
            started_at: None,
            completed_at: None,
        }
    }

    /// Attach requester provenance captured from the inbound HTTP request.
    pub fn with_requester(
        mut self,
        ip: Option<String>,
        user_agent: Option<String>,
        label: Option<String>,
        key_id: Option<Uuid>,
    ) -> Self {
        self.requester_ip = ip;
        self.requester_user_agent = user_agent;
        self.requester_label = label;
        self.requester_key_id = key_id;
        self
    }

    pub fn update_status(&mut self, status: JobStatus) {
        self.status = status;
        self.updated_at = OffsetDateTime::now_utc();
    }

    pub fn mark_ready(&mut self, media_url: String) {
        self.status = JobStatus::Ready;
        self.media_url = Some(media_url);
        self.error = None;
        self.updated_at = OffsetDateTime::now_utc();
    }

    pub fn mark_error(&mut self, error: String) {
        self.status = JobStatus::Error;
        self.error = Some(error);
        self.updated_at = OffsetDateTime::now_utc();
    }
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Resolving => "resolving",
            Self::CandidatesReady => "candidates_ready",
            Self::NeedsProfile => "needs_profile",
            Self::CandidateSelected => "candidate_selected",
            Self::Downloading => "downloading",
            Self::Capturing => "capturing",
            Self::Probing => "probing",
            Self::Transcoding => "transcoding",
            Self::Remuxing => "remuxing",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "resolving" => Some(Self::Resolving),
            "candidates_ready" => Some(Self::CandidatesReady),
            "needs_profile" => Some(Self::NeedsProfile),
            "candidate_selected" => Some(Self::CandidateSelected),
            "downloading" => Some(Self::Downloading),
            "capturing" => Some(Self::Capturing),
            "probing" => Some(Self::Probing),
            "transcoding" => Some(Self::Transcoding),
            "remuxing" => Some(Self::Remuxing),
            "ready" => Some(Self::Ready),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl From<JobRecord> for JobView {
    fn from(value: JobRecord) -> Self {
        let issue_kind = job_issue_kind(&value);
        let issue_detail = value.error.clone();
        Self {
            id: value.id,
            status: value.status,
            source_url: value.source_url,
            bitrate: value.bitrate,
            created_at: value.created_at,
            updated_at: value.updated_at,
            status_url: value.status_url,
            media_url: value.media_url,
            error: value.error,
            artifacts_url: format!("/api/jobs/{}/artifacts", value.id),
            candidates_url: format!("/api/jobs/{}/candidates", value.id),
            discovery: value.discovery,
            platform_hint: value.platform_hint,
            outputs: value.outputs,
            profile_id: value.profile_id,
            auth_mode: value.auth_mode,
            trace_url: format!("/api/jobs/{}/trace", value.id),
            requester_ip: value.requester_ip,
            requester_user_agent: value.requester_user_agent,
            requester_label: value.requester_label,
            requester_key_id: value.requester_key_id,
            resolved_extractor: value.resolved_extractor,
            error_class: value.error_class,
            issue_kind,
            issue_label: issue_kind.label().to_string(),
            issue_detail,
            profile_action_url: if issue_kind == JobIssueKind::NeedsProfile {
                Some(format!("/api/jobs/{}/browser-login-session", value.id))
            } else {
                None
            },
            attempt_count: value.attempt_count,
            started_at: value.started_at,
            completed_at: value.completed_at,
        }
    }
}

fn job_issue_kind(job: &JobRecord) -> JobIssueKind {
    if job.status == JobStatus::NeedsProfile {
        return JobIssueKind::NeedsProfile;
    }
    if job.status != JobStatus::Error {
        return JobIssueKind::None;
    }
    match job.error_class {
        ErrorClass::TooLarge => return JobIssueKind::TooLarge,
        ErrorClass::Timeout => return JobIssueKind::Timeout,
        ErrorClass::DrmBlocked => return JobIssueKind::Unsupported,
        _ => {}
    }
    let lowered = job
        .error
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if lowered.contains("fresh cookies")
        || lowered.contains("sign in")
        || lowered.contains("login required")
        || lowered.contains("requires authorization")
        || lowered.contains("requires headers")
        || lowered.contains("human verification")
        || lowered.contains("human browser interaction")
        || lowered.contains("security verification")
        || lowered.contains("security challenge")
        || lowered.contains("cloudflare")
        || lowered.contains("turnstile")
        || lowered.contains("captcha")
        || lowered.contains("profile")
    {
        JobIssueKind::NeedsProfile
    } else if lowered.contains("unsupported")
        || lowered.contains("not supported")
        || lowered.contains("no media candidates")
        || lowered.contains("did not find media candidates")
        || lowered.contains("no extractor matched")
        || lowered.contains("dash")
        || lowered.contains("mpd")
        || lowered.contains("blob")
    {
        JobIssueKind::Unsupported
    } else if matches!(job.error_class, ErrorClass::Blocked) {
        JobIssueKind::PolicyBlocked
    } else {
        JobIssueKind::Failed
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMode {
    Direct,
    External,
    Browser,
    Auto,
}

impl DiscoveryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::External => "external",
            Self::Browser => "browser",
            Self::Auto => "auto",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "direct" => Some(Self::Direct),
            "external" => Some(Self::External),
            "browser" => Some(Self::Browser),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlatformHint {
    Auto,
    Bilibili,
    Youtube,
    Soundcloud,
    Douyin,
    Kuaishou,
    Pornhub,
    Acfun,
    Iqiyi,
    Youku,
    Tiktok,
    Vimeo,
    Live,
    Generic,
}

impl PlatformHint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Bilibili => "bilibili",
            Self::Youtube => "youtube",
            Self::Soundcloud => "soundcloud",
            Self::Douyin => "douyin",
            Self::Kuaishou => "kuaishou",
            Self::Pornhub => "pornhub",
            Self::Acfun => "acfun",
            Self::Iqiyi => "iqiyi",
            Self::Youku => "youku",
            Self::Tiktok => "tiktok",
            Self::Vimeo => "vimeo",
            Self::Live => "live",
            Self::Generic => "generic",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "bilibili" => Some(Self::Bilibili),
            "youtube" => Some(Self::Youtube),
            "soundcloud" => Some(Self::Soundcloud),
            "douyin" => Some(Self::Douyin),
            "kuaishou" => Some(Self::Kuaishou),
            "pornhub" => Some(Self::Pornhub),
            "acfun" => Some(Self::Acfun),
            "iqiyi" => Some(Self::Iqiyi),
            "youku" => Some(Self::Youku),
            "tiktok" => Some(Self::Tiktok),
            "vimeo" => Some(Self::Vimeo),
            "live" => Some(Self::Live),
            "generic" => Some(Self::Generic),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    Audio,
    Video,
    Image,
    Markdown,
    PageHtml,
}

impl OutputKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Image => "image",
            Self::Markdown => "markdown",
            Self::PageHtml => "page_html",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    Auto,
    None,
    Profile,
    Cookies,
}

impl AuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::Profile => "profile",
            Self::Cookies => "cookies",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "none" => Some(Self::None),
            "profile" => Some(Self::Profile),
            "cookies" => Some(Self::Cookies),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateProtection {
    None,
    NeedsProfile,
    SignedUrl,
    Drm,
    RegionBlocked,
    Unknown,
}

impl CandidateProtection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NeedsProfile => "needs_profile",
            Self::SignedUrl => "signed_url",
            Self::Drm => "drm",
            Self::RegionBlocked => "region_blocked",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "needs_profile" => Some(Self::NeedsProfile),
            "signed_url" => Some(Self::SignedUrl),
            "drm" => Some(Self::Drm),
            "region_blocked" => Some(Self::RegionBlocked),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateValidationState {
    Untested,
    Usable,
    NeedsProfile,
    SuspectAd,
    Expired,
    Drm,
    RegionBlocked,
    Failed,
}

impl CandidateValidationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Untested => "untested",
            Self::Usable => "usable",
            Self::NeedsProfile => "needs_profile",
            Self::SuspectAd => "suspect_ad",
            Self::Expired => "expired",
            Self::Drm => "drm",
            Self::RegionBlocked => "region_blocked",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "untested" => Some(Self::Untested),
            "usable" => Some(Self::Usable),
            "needs_profile" => Some(Self::NeedsProfile),
            "suspect_ad" => Some(Self::SuspectAd),
            "expired" => Some(Self::Expired),
            "drm" => Some(Self::Drm),
            "region_blocked" => Some(Self::RegionBlocked),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Audio,
    Video,
    Image,
    Manifest,
    Html,
    Unknown,
}

impl CandidateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Image => "image",
            Self::Manifest => "manifest",
            Self::Html => "html",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "audio" => Some(Self::Audio),
            "video" => Some(Self::Video),
            "image" => Some(Self::Image),
            "manifest" => Some(Self::Manifest),
            "html" => Some(Self::Html),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCandidate {
    pub id: Uuid,
    pub job_id: Uuid,
    pub url: String,
    pub kind: CandidateKind,
    pub extractor: String,
    pub method: String,
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub content_length: Option<i64>,
    pub resource_type: Option<String>,
    pub initiator_url: Option<String>,
    pub quality_label: Option<String>,
    pub score: i64,
    pub requires_authorization: bool,
    #[serde(default)]
    pub platform: Option<PlatformHint>,
    #[serde(default)]
    pub route: Option<String>,
    #[serde(default)]
    pub extractor_confidence: Option<i64>,
    #[serde(default)]
    pub protection: Option<CandidateProtection>,
    #[serde(default)]
    pub requires_profile: bool,
    #[serde(default)]
    pub ttl_hint_seconds: Option<i64>,
    #[serde(default)]
    pub ad_risk: bool,
    #[serde(default)]
    pub evidence_count: i64,
    #[serde(default)]
    pub paired_candidate_ids: Vec<Uuid>,
    pub failure_reason: Option<String>,
    #[serde(default)]
    pub validation_state: Option<CandidateValidationState>,
    pub metadata_json: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Per-component breakdown of how `score` was computed (auditability:
    /// "how we calculated"). Empty object when not supplied by the extractor.
    #[serde(default)]
    pub score_breakdown_json: serde_json::Value,
    /// Whether this candidate was selected for capture.
    #[serde(default)]
    pub selected: bool,
    pub selection_reason: Option<String>,
    /// Result of the pre-capture URL policy re-check ("passed" / "blocked: ...").
    pub validation_status: Option<String>,
    pub resolved_ip: Option<String>,
    pub final_url_after_redirects: Option<String>,
    /// Expiry parsed from a signed media URL, if any.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    /// The pipeline event that first surfaced this candidate.
    pub discovered_by_event_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactView {
    pub id: Uuid,
    pub job_id: Uuid,
    pub kind: OutputKind,
    pub path: String,
    pub media_url: String,
    pub content_type: String,
    pub bytes: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectCandidatesRequest {
    pub candidate_ids: Vec<Uuid>,
}

pub fn normalize_outputs(value: Option<Vec<OutputKind>>) -> Vec<OutputKind> {
    let mut outputs = value.unwrap_or_else(|| vec![OutputKind::Video, OutputKind::Audio]);
    outputs.sort_by_key(|output| output.as_str());
    outputs.dedup();
    if outputs.is_empty() {
        vec![OutputKind::Video, OutputKind::Audio]
    } else {
        outputs
    }
}

pub fn normalize_profile_id(value: Option<String>) -> String {
    let filtered: String = value
        .unwrap_or_else(|| "admin_default".to_string())
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .take(64)
        .collect();
    if filtered.is_empty() {
        "admin_default".to_string()
    } else {
        filtered
    }
}

pub fn normalize_bitrate(value: Option<&str>) -> String {
    match value {
        Some(
            value @ ("auto" | "2160p" | "1440p" | "1080p" | "720p" | "480p" | "360p" | "96k"
            | "128k" | "160k" | "192k" | "256k" | "320k"),
        ) => value.to_string(),
        _ => "192k".to_string(),
    }
}
