use std::{collections::HashMap, time::Duration};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    models::{CandidateKind, MediaCandidate, PlatformHint},
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
}

#[derive(Debug, Clone, Deserialize)]
struct HeadersForUrlResponse {
    headers: HashMap<String, String>,
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
        Ok(response
            .candidates
            .into_iter()
            .filter(|candidate| parse_and_validate_url(&candidate.url).is_ok())
            .map(|candidate| candidate.into_media_candidate(job_id, &metadata))
            .collect())
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
}

impl BrowserCandidate {
    fn into_media_candidate(self, job_id: Uuid, metadata: &ProbeMetadata) -> MediaCandidate {
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
            metadata_json: serde_json::json!({
                "final_url": metadata.final_url,
                "title": metadata.title,
                "warnings": metadata.warnings,
                "event_count": metadata.event_count,
                "timed_out": metadata.timed_out,
            }),
            created_at: OffsetDateTime::now_utc(),
        }
    }
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
