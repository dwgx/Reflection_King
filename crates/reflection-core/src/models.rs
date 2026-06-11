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
    pub resolved_extractor: Option<String>,
    pub error_class: ErrorClass,
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
    ) -> Self {
        self.requester_ip = ip;
        self.requester_user_agent = user_agent;
        self.requester_label = label;
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
            resolved_extractor: value.resolved_extractor,
            error_class: value.error_class,
            attempt_count: value.attempt_count,
            started_at: value.started_at,
            completed_at: value.completed_at,
        }
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
}

impl PlatformHint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Bilibili => "bilibili",
            Self::Youtube => "youtube",
            Self::Soundcloud => "soundcloud",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "bilibili" => Some(Self::Bilibili),
            "youtube" => Some(Self::Youtube),
            "soundcloud" => Some(Self::Soundcloud),
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
