use std::{collections::HashMap, time::Duration};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    models::{
        CandidateKind, CandidateProtection, CandidateValidationState, MediaCandidate, PlatformHint,
    },
    url_policy::parse_and_validate_url,
    Result, RkError,
};

#[derive(Debug, Clone)]
pub struct BrowserProbeClient {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeRequest {
    pub url: String,
    #[serde(rename = "profileId")]
    pub profile_id: String,
    #[serde(rename = "platformHint")]
    pub platform_hint: String,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProbeResponse {
    #[serde(rename = "finalUrl")]
    pub final_url: String,
    pub title: Option<String>,
    pub candidates: Vec<BrowserCandidate>,
    pub warnings: Vec<String>,
    #[serde(rename = "eventCount")]
    pub event_count: usize,
    #[serde(rename = "timedOut")]
    pub timed_out: bool,
    // Fields emitted by the enhanced sidecar (Phase 3). Optional so older
    // sidecars still deserialize cleanly.
    #[serde(rename = "userAgent", default)]
    pub user_agent: Option<String>,
    #[serde(rename = "playbackTriggered", default)]
    pub playback_triggered: Option<bool>,
    #[serde(rename = "consoleErrors", default)]
    pub console_errors: Option<Vec<String>>,
}

/// Result of a browser probe that also carries session metadata for the trace.
#[derive(Debug, Clone)]
pub struct BrowserProbeOutcome {
    pub candidates: Vec<MediaCandidate>,
    pub final_url: String,
    pub title: Option<String>,
    pub warnings: Vec<String>,
    pub event_count: usize,
    pub timed_out: bool,
    pub user_agent: Option<String>,
    pub playback_triggered: bool,
    pub console_errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrowserCandidate {
    pub url: String,
    pub kind: String,
    pub method: String,
    pub status: Option<u16>,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
    #[serde(rename = "contentLength")]
    pub content_length: Option<i64>,
    #[serde(rename = "resourceType")]
    pub resource_type: Option<String>,
    #[serde(rename = "initiatorUrl")]
    pub initiator_url: Option<String>,
    #[serde(rename = "qualityLabel")]
    pub quality_label: Option<String>,
    pub score: i64,
    #[serde(rename = "requiresAuthorization")]
    pub requires_authorization: bool,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct HeadersForUrlResponse {
    headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrowserCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: f64,
    #[serde(rename = "httpOnly")]
    pub http_only: bool,
    pub secure: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct CookiesForUrlResponse {
    cookies: Vec<BrowserCookie>,
}

#[derive(Debug, Clone, Serialize)]
struct ImportCookiesRequest {
    cookies: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
struct LoginSessionStartRequest<'a> {
    url: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct LoginClickRequest {
    x: f64,
    y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    button: Option<String>,
    #[serde(rename = "clickCount", skip_serializing_if = "Option::is_none")]
    click_count: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
struct LoginTypeRequest<'a> {
    text: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct LoginPressRequest<'a> {
    key: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct LoginNavigateRequest<'a> {
    url: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct LoginWheelRequest {
    #[serde(rename = "deltaX")]
    delta_x: f64,
    #[serde(rename = "deltaY")]
    delta_y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    y: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct LoginResizeRequest {
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoginSessionView {
    pub id: String,
    #[serde(rename = "profileId")]
    pub profile_id: String,
    pub url: String,
    pub title: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "lastActiveAt")]
    pub last_active_at: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoginSessionSnapshot {
    pub session: LoginSessionView,
    pub image: String,
    pub url: String,
    pub title: Option<String>,
    pub width: u32,
    pub height: u32,
}

impl BrowserProbeClient {
    pub fn new(base_url: impl Into<String>, timeout: Duration) -> Result<Self> {
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    pub async fn probe(
        &self,
        job_id: Uuid,
        url: &str,
        profile_id: &str,
        platform_hint: PlatformHint,
        outputs: &[String],
    ) -> Result<Vec<MediaCandidate>> {
        Ok(self
            .probe_session(job_id, url, profile_id, platform_hint, outputs)
            .await?
            .candidates)
    }

    /// Probe a page and return both the policy-checked candidates and the
    /// session metadata (final URL, title, warnings, counts) so callers can
    /// persist a `browser_sessions` record for the trace.
    pub async fn probe_session(
        &self,
        job_id: Uuid,
        url: &str,
        profile_id: &str,
        platform_hint: PlatformHint,
        outputs: &[String],
    ) -> Result<BrowserProbeOutcome> {
        let request = ProbeRequest {
            url: url.to_string(),
            profile_id: profile_id.to_string(),
            platform_hint: platform_hint.as_str().to_string(),
            outputs: outputs.to_vec(),
        };
        let response = self
            .client
            .post(format!("{}/probe", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(RkError::Browser(format!(
                "browser probe returned HTTP {}",
                response.status()
            )));
        }

        let response: ProbeResponse = response.json().await?;
        let metadata = ProbeMetadata::from(&response);
        let candidates = response
            .candidates
            .iter()
            .filter(|candidate| parse_and_validate_url(&candidate.url).is_ok())
            .cloned()
            .map(|candidate| candidate.into_media_candidate(job_id, platform_hint, &metadata))
            .collect::<Vec<_>>();

        Ok(BrowserProbeOutcome {
            candidates,
            final_url: response.final_url,
            title: response.title,
            warnings: response.warnings,
            event_count: response.event_count,
            timed_out: response.timed_out,
            user_agent: response.user_agent,
            playback_triggered: response.playback_triggered.unwrap_or(false),
            console_errors: response.console_errors.unwrap_or_default(),
        })
    }

    pub async fn headers_for_url(
        &self,
        profile_id: &str,
        url: &str,
        referer: Option<&str>,
    ) -> Result<HeaderMap> {
        let response = self
            .client
            .post(format!(
                "{}/profiles/{}/headers-for-url",
                self.base_url, profile_id
            ))
            .json(&serde_json::json!({
                "url": url,
                "referer": referer,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(RkError::Browser(format!(
                "headers-for-url returned HTTP {}",
                response.status()
            )));
        }

        let response: HeadersForUrlResponse = response.json().await?;
        let mut headers = HeaderMap::new();
        for (key, value) in response.headers {
            let Ok(name) = HeaderName::from_bytes(key.as_bytes()) else {
                continue;
            };
            let Ok(value) = HeaderValue::from_str(&value) else {
                continue;
            };
            headers.insert(name, value);
        }
        Ok(headers)
    }

    pub async fn cookies_for_url(&self, profile_id: &str, url: &str) -> Result<Vec<BrowserCookie>> {
        let response = self
            .client
            .post(format!(
                "{}/profiles/{}/cookies-for-url",
                self.base_url, profile_id
            ))
            .json(&serde_json::json!({ "url": url }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(RkError::Browser(format!(
                "cookies-for-url returned HTTP {}",
                response.status()
            )));
        }

        let response: CookiesForUrlResponse = response.json().await?;
        Ok(response.cookies)
    }

    pub async fn import_cookies(
        &self,
        profile_id: &str,
        cookies: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let response = self
            .client
            .post(format!(
                "{}/profiles/{}/cookies/import",
                self.base_url, profile_id
            ))
            .json(&ImportCookiesRequest { cookies })
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(RkError::Browser(format!(
                "cookies/import returned HTTP {}",
                response.status()
            )));
        }

        Ok(response.json().await?)
    }

    pub async fn start_login_session(
        &self,
        profile_id: &str,
        url: &str,
    ) -> Result<LoginSessionSnapshot> {
        let response = self
            .client
            .post(format!(
                "{}/profiles/{}/login-sessions",
                self.base_url, profile_id
            ))
            .json(&LoginSessionStartRequest { url })
            .send()
            .await?;
        self.login_response(response, "login-sessions").await
    }

    pub async fn login_session_snapshot(&self, session_id: &str) -> Result<LoginSessionSnapshot> {
        let response = self
            .client
            .get(format!(
                "{}/login-sessions/{}/snapshot",
                self.base_url, session_id
            ))
            .send()
            .await?;
        self.login_response(response, "login-session snapshot")
            .await
    }

    pub async fn login_session_click(
        &self,
        session_id: &str,
        x: f64,
        y: f64,
        button: Option<&str>,
        click_count: Option<u8>,
    ) -> Result<LoginSessionSnapshot> {
        let response = self
            .client
            .post(format!(
                "{}/login-sessions/{}/click",
                self.base_url, session_id
            ))
            .json(&LoginClickRequest {
                x,
                y,
                button: button.map(ToString::to_string),
                click_count,
            })
            .send()
            .await?;
        self.login_response(response, "login-session click").await
    }

    pub async fn login_session_type(
        &self,
        session_id: &str,
        text: &str,
    ) -> Result<LoginSessionSnapshot> {
        let response = self
            .client
            .post(format!(
                "{}/login-sessions/{}/type",
                self.base_url, session_id
            ))
            .json(&LoginTypeRequest { text })
            .send()
            .await?;
        self.login_response(response, "login-session type").await
    }

    pub async fn login_session_press(
        &self,
        session_id: &str,
        key: &str,
    ) -> Result<LoginSessionSnapshot> {
        let response = self
            .client
            .post(format!(
                "{}/login-sessions/{}/press",
                self.base_url, session_id
            ))
            .json(&LoginPressRequest { key })
            .send()
            .await?;
        self.login_response(response, "login-session press").await
    }

    pub async fn login_session_navigate(
        &self,
        session_id: &str,
        url: &str,
    ) -> Result<LoginSessionSnapshot> {
        let response = self
            .client
            .post(format!(
                "{}/login-sessions/{}/navigate",
                self.base_url, session_id
            ))
            .json(&LoginNavigateRequest { url })
            .send()
            .await?;
        self.login_response(response, "login-session navigate")
            .await
    }

    pub async fn login_session_wheel(
        &self,
        session_id: &str,
        delta_x: f64,
        delta_y: f64,
        x: Option<f64>,
        y: Option<f64>,
    ) -> Result<LoginSessionSnapshot> {
        let response = self
            .client
            .post(format!(
                "{}/login-sessions/{}/wheel",
                self.base_url, session_id
            ))
            .json(&LoginWheelRequest {
                delta_x,
                delta_y,
                x,
                y,
            })
            .send()
            .await?;
        self.login_response(response, "login-session wheel").await
    }

    pub async fn login_session_resize(
        &self,
        session_id: &str,
        width: u32,
        height: u32,
    ) -> Result<LoginSessionSnapshot> {
        let response = self
            .client
            .post(format!(
                "{}/login-sessions/{}/resize",
                self.base_url, session_id
            ))
            .json(&LoginResizeRequest { width, height })
            .send()
            .await?;
        self.login_response(response, "login-session resize").await
    }

    pub async fn close_login_session(&self, session_id: &str) -> Result<serde_json::Value> {
        let response = self
            .client
            .post(format!(
                "{}/login-sessions/{}/close",
                self.base_url, session_id
            ))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(RkError::Browser(format!(
                "login-session close returned HTTP {}",
                response.status()
            )));
        }
        Ok(response.json().await?)
    }

    async fn login_response(
        &self,
        response: reqwest::Response,
        label: &str,
    ) -> Result<LoginSessionSnapshot> {
        if !response.status().is_success() {
            return Err(RkError::Browser(format!(
                "{label} returned HTTP {}",
                response.status()
            )));
        }
        Ok(response.json().await?)
    }
}

impl BrowserCandidate {
    fn into_media_candidate(
        self,
        job_id: Uuid,
        platform_hint: PlatformHint,
        metadata: &ProbeMetadata,
    ) -> MediaCandidate {
        let ad_risk = is_likely_ad_or_tracking_url(&self.url);
        let signed = is_signed_url(&self.url);
        let protection = if self.requires_authorization {
            CandidateProtection::NeedsProfile
        } else if signed {
            CandidateProtection::SignedUrl
        } else {
            CandidateProtection::None
        };
        MediaCandidate {
            id: Uuid::new_v4(),
            job_id,
            url: self.url,
            kind: CandidateKind::parse(&self.kind).unwrap_or(CandidateKind::Unknown),
            extractor: "browser_probe".to_string(),
            method: self.method,
            status: self.status,
            content_type: self.content_type,
            content_length: self.content_length,
            resource_type: self.resource_type,
            initiator_url: self.initiator_url,
            quality_label: self.quality_label,
            score: self.score,
            requires_authorization: self.requires_authorization,
            platform: if platform_hint == PlatformHint::Auto {
                None
            } else {
                Some(platform_hint)
            },
            route: Some("browser_probe".to_string()),
            extractor_confidence: Some(70),
            protection: Some(protection),
            requires_profile: self.requires_authorization,
            ttl_hint_seconds: None,
            ad_risk,
            evidence_count: 1,
            paired_candidate_ids: Vec::new(),
            failure_reason: None,
            validation_state: Some(if ad_risk {
                CandidateValidationState::SuspectAd
            } else {
                CandidateValidationState::Untested
            }),
            metadata_json: serde_json::json!({
                "final_url": metadata.final_url,
                "title": metadata.title,
                "warnings": metadata.warnings,
                "event_count": metadata.event_count,
                "timed_out": metadata.timed_out,
                "candidate": self.metadata,
            }),
            created_at: OffsetDateTime::now_utc(),
            score_breakdown_json: serde_json::Value::Object(Default::default()),
            selected: false,
            selection_reason: None,
            validation_status: None,
            resolved_ip: None,
            final_url_after_redirects: None,
            expires_at: None,
            discovered_by_event_id: None,
        }
    }
}

fn is_signed_url(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    [
        "expires=",
        "expire=",
        "deadline=",
        "x-expires=",
        "x-amz-expires=",
        "signature=",
        "sign=",
        "token=",
        "auth_key=",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn is_likely_ad_or_tracking_url(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
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
        "/ads/",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

struct ProbeMetadata {
    final_url: String,
    title: Option<String>,
    warnings: Vec<String>,
    event_count: usize,
    timed_out: bool,
}

impl From<&ProbeResponse> for ProbeMetadata {
    fn from(value: &ProbeResponse) -> Self {
        Self {
            final_url: value.final_url.clone(),
            title: value.title.clone(),
            warnings: value.warnings.clone(),
            event_count: value.event_count,
            timed_out: value.timed_out,
        }
    }
}
