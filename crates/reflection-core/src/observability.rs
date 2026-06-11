//! Full-chain observability records.
//!
//! Every job carries an append-only trail so that any artifact can answer:
//! who (which extractor/profile) did what, against which URL, with which
//! headers, how a score was computed, what went wrong, and exactly when.
//!
//! Secrets never land in these records: header maps are redacted before they
//! are persisted (see [`redact_headers`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// Header names whose values must never be stored in clear text.
const SENSITIVE_HEADERS: &[&str] = &[
    "cookie",
    "set-cookie",
    "authorization",
    "proxy-authorization",
    "x-csrf-token",
    "x-xsrf-token",
    "x-auth-token",
    "x-api-key",
    "x-bili-ticket",
    "www-authenticate",
];

/// Replace the values of sensitive headers with a length-only placeholder while
/// keeping non-sensitive headers intact, so the trail stays useful for
/// debugging without ever leaking credentials.
pub fn redact_header_map(headers: &BTreeMap<String, String>) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for (key, value) in headers {
        out.insert(key.clone(), redact_one(key, value));
    }
    serde_json::Value::Object(out)
}

/// Redact a `serde_json::Value` that is expected to be a flat header object.
/// Non-object values are returned unchanged.
pub fn redact_headers(value: &serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(map) = value else {
        return value.clone();
    };
    let mut out = serde_json::Map::new();
    for (key, raw) in map {
        let rendered = raw
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| raw.to_string());
        out.insert(key.clone(), redact_one(key, &rendered));
    }
    serde_json::Value::Object(out)
}

fn redact_one(key: &str, value: &str) -> serde_json::Value {
    if SENSITIVE_HEADERS.contains(&key.to_ascii_lowercase().as_str()) {
        serde_json::Value::String(format!("[redacted len={}]", value.len()))
    } else {
        serde_json::Value::String(value.to_string())
    }
}

/// Classification of why a fetch attempt failed. Drives retry decisions and the
/// `error_class` columns on jobs and request logs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    None,
    Dns,
    Tls,
    Http4xx,
    Http5xx,
    Timeout,
    TooLarge,
    Blocked,
    Parse,
    DrmBlocked,
    Internal,
}

impl ErrorClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Dns => "dns",
            Self::Tls => "tls",
            Self::Http4xx => "http_4xx",
            Self::Http5xx => "http_5xx",
            Self::Timeout => "timeout",
            Self::TooLarge => "too_large",
            Self::Blocked => "blocked",
            Self::Parse => "parse",
            Self::DrmBlocked => "drm_blocked",
            Self::Internal => "internal",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "dns" => Self::Dns,
            "tls" => Self::Tls,
            "http_4xx" => Self::Http4xx,
            "http_5xx" => Self::Http5xx,
            "timeout" => Self::Timeout,
            "too_large" => Self::TooLarge,
            "blocked" => Self::Blocked,
            "parse" => Self::Parse,
            "drm_blocked" => Self::DrmBlocked,
            "internal" => Self::Internal,
            _ => Self::None,
        }
    }

    /// Whether a fresh attempt has a reasonable chance of succeeding.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Timeout | Self::Http5xx | Self::Dns | Self::Tls)
    }
}

/// Stage of the pipeline a request belongs to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestPhase {
    Resolve,
    Probe,
    Download,
    Capture,
    Headers,
}

impl RequestPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resolve => "resolve",
            Self::Probe => "probe",
            Self::Download => "download",
            Self::Capture => "capture",
            Self::Headers => "headers",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "probe" => Self::Probe,
            "download" => Self::Download,
            "capture" => Self::Capture,
            "headers" => Self::Headers,
            _ => Self::Resolve,
        }
    }
}

/// One outbound HTTP request made by the backend or the browser sidecar.
/// Answers "what IP / what browser / what we gave it / what came back / when".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLog {
    pub id: Uuid,
    pub job_id: Uuid,
    pub candidate_id: Option<Uuid>,
    pub phase: RequestPhase,
    pub method: String,
    pub url: String,
    pub host: Option<String>,
    pub resolved_ip: Option<String>,
    pub egress_ip: Option<String>,
    pub request_headers_json: serde_json::Value,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub profile_id: Option<String>,
    pub response_status: Option<u16>,
    pub response_headers_json: Option<serde_json::Value>,
    pub content_type: Option<String>,
    pub content_length: Option<i64>,
    pub bytes_read: Option<i64>,
    pub redirect_chain_json: Option<serde_json::Value>,
    pub http_version: Option<String>,
    pub tls_version: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub ended_at: Option<OffsetDateTime>,
    pub duration_ms: Option<i64>,
    pub error_class: ErrorClass,
    pub error_message: Option<String>,
}

impl RequestLog {
    /// Begin a request record at the current time with empty response fields.
    pub fn begin(job_id: Uuid, phase: RequestPhase, method: &str, url: &str) -> Self {
        let host = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string));
        Self {
            id: Uuid::new_v4(),
            job_id,
            candidate_id: None,
            phase,
            method: method.to_string(),
            url: url.to_string(),
            host,
            resolved_ip: None,
            egress_ip: None,
            request_headers_json: serde_json::Value::Object(Default::default()),
            user_agent: None,
            referer: None,
            profile_id: None,
            response_status: None,
            response_headers_json: None,
            content_type: None,
            content_length: None,
            bytes_read: None,
            redirect_chain_json: None,
            http_version: None,
            tls_version: None,
            started_at: OffsetDateTime::now_utc(),
            ended_at: None,
            duration_ms: None,
            error_class: ErrorClass::None,
            error_message: None,
        }
    }

    pub fn with_candidate(mut self, candidate_id: Uuid) -> Self {
        self.candidate_id = Some(candidate_id);
        self
    }

    /// Stamp `ended_at`/`duration_ms` from `started_at` to now.
    pub fn complete(&mut self) {
        let ended = OffsetDateTime::now_utc();
        self.duration_ms = Some(((ended - self.started_at).whole_milliseconds()) as i64);
        self.ended_at = Some(ended);
    }

    pub fn fail(&mut self, class: ErrorClass, message: impl Into<String>) {
        self.error_class = class;
        self.error_message = Some(message.into());
        self.complete();
    }
}

/// Type of an entry in the append-only pipeline event log.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineEventType {
    StatusChange,
    ExtractorAttempt,
    CandidateFound,
    CandidateSelected,
    DownloadStart,
    Probe,
    Transcode,
    Warning,
    Error,
    Note,
}

impl PipelineEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StatusChange => "status_change",
            Self::ExtractorAttempt => "extractor_attempt",
            Self::CandidateFound => "candidate_found",
            Self::CandidateSelected => "candidate_selected",
            Self::DownloadStart => "download_start",
            Self::Probe => "probe",
            Self::Transcode => "transcode",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Note => "note",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "status_change" => Self::StatusChange,
            "extractor_attempt" => Self::ExtractorAttempt,
            "candidate_found" => Self::CandidateFound,
            "candidate_selected" => Self::CandidateSelected,
            "download_start" => Self::DownloadStart,
            "probe" => Self::Probe,
            "transcode" => Self::Transcode,
            "warning" => Self::Warning,
            "error" => Self::Error,
            _ => Self::Note,
        }
    }
}

/// One step in the lifecycle of a job. Answers "our steps / who / when".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineEvent {
    pub id: Uuid,
    pub job_id: Uuid,
    pub seq: i64,
    pub stage: String,
    pub actor: String,
    pub event_type: PipelineEventType,
    pub detail_json: serde_json::Value,
    pub candidate_id: Option<Uuid>,
    pub request_log_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub duration_ms: Option<i64>,
}

impl PipelineEvent {
    pub fn new(
        job_id: Uuid,
        stage: impl Into<String>,
        actor: impl Into<String>,
        event_type: PipelineEventType,
        detail_json: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            job_id,
            seq: 0,
            stage: stage.into(),
            actor: actor.into(),
            event_type,
            detail_json,
            candidate_id: None,
            request_log_id: None,
            created_at: OffsetDateTime::now_utc(),
            duration_ms: None,
        }
    }
}

/// One browser probe session. Answers "what browser".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSession {
    pub id: Uuid,
    pub job_id: Uuid,
    pub profile_id: String,
    pub user_agent: Option<String>,
    pub viewport: Option<String>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub headed: bool,
    pub final_url: Option<String>,
    pub page_title: Option<String>,
    pub event_count: i64,
    pub candidate_count: i64,
    pub playback_triggered: bool,
    pub timed_out: bool,
    pub warnings_json: serde_json::Value,
    pub console_errors_json: serde_json::Value,
    pub screenshot_path: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub ended_at: Option<OffsetDateTime>,
}

/// ffprobe result captured before a transcode/remux decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProbe {
    pub id: Uuid,
    pub job_id: Uuid,
    pub candidate_id: Option<Uuid>,
    pub container: Option<String>,
    pub duration_s: Option<f64>,
    pub overall_bitrate: Option<i64>,
    pub streams_json: serde_json::Value,
    pub raw_json: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// One ffmpeg invocation. Answers "how we processed it".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodeRun {
    pub id: Uuid,
    pub job_id: Uuid,
    pub candidate_id: Option<Uuid>,
    pub tool: String,
    pub command_redacted: String,
    pub input_bytes: Option<i64>,
    pub output_bytes: Option<i64>,
    pub output_path: Option<String>,
    pub output_kind: Option<String>,
    pub profile: Option<String>,
    pub exit_code: Option<i64>,
    pub stderr_tail: Option<String>,
    pub duration_ms: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Per-host crawl policy and observed behavior, including a learned player API
/// pattern that can be reused on later jobs for the same host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPolicy {
    pub host: String,
    pub allow_mode: String,
    pub max_concurrency: i64,
    pub crawl_delay_ms: i64,
    pub requires_user_auth: bool,
    pub last_status: Option<i64>,
    pub blocked_count: i64,
    pub learned_api_pattern: Option<String>,
    pub notes: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl DomainPolicy {
    pub fn default_for(host: &str) -> Self {
        Self {
            host: host.to_string(),
            allow_mode: "allow".to_string(),
            max_concurrency: 2,
            crawl_delay_ms: 0,
            requires_user_auth: false,
            last_status: None,
            blocked_count: 0,
            learned_api_pattern: None,
            notes: None,
            updated_at: OffsetDateTime::now_utc(),
        }
    }
}

/// Aggregate timeline for a single job, returned by `GET /api/jobs/{id}/trace`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobTrace {
    pub job_id: Uuid,
    pub events: Vec<PipelineEvent>,
    pub requests: Vec<RequestLog>,
    pub browser_sessions: Vec<BrowserSession>,
    pub probes: Vec<MediaProbe>,
    pub transcodes: Vec<TranscodeRun>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_only_sensitive_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("User-Agent".to_string(), "Mozilla/5.0".to_string());
        headers.insert(
            "Referer".to_string(),
            "https://example.com/page".to_string(),
        );
        headers.insert("Cookie".to_string(), "session=supersecretvalue".to_string());
        headers.insert("Authorization".to_string(), "Bearer abc.def".to_string());

        let redacted = redact_header_map(&headers);

        assert_eq!(redacted["User-Agent"], "Mozilla/5.0");
        assert_eq!(redacted["Referer"], "https://example.com/page");
        // Sensitive values are replaced with a length-only placeholder, and the
        // original secret never appears.
        assert!(redacted["Cookie"]
            .as_str()
            .unwrap()
            .starts_with("[redacted len="));
        assert!(redacted["Authorization"]
            .as_str()
            .unwrap()
            .starts_with("[redacted len="));
        let serialized = redacted.to_string();
        assert!(!serialized.contains("supersecretvalue"));
        assert!(!serialized.contains("abc.def"));
    }

    #[test]
    fn error_class_retry_policy() {
        assert!(ErrorClass::Timeout.is_retryable());
        assert!(ErrorClass::Http5xx.is_retryable());
        assert!(!ErrorClass::Http4xx.is_retryable());
        assert!(!ErrorClass::Blocked.is_retryable());
        assert!(!ErrorClass::DrmBlocked.is_retryable());
        assert_eq!(ErrorClass::parse("drm_blocked"), ErrorClass::DrmBlocked);
        assert_eq!(ErrorClass::parse("nonsense"), ErrorClass::None);
    }
}
