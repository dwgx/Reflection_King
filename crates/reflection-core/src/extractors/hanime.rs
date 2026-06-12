//! Hanime1 dedicated extractor.
//!
//! Playwright often triggers Cloudflare on Hanime1, while a mobile browser
//! request returns the normal watch page. The page embeds complete MP4 URLs in
//! inline JavaScript, so this extractor keeps that path ahead of the browser
//! fallback and preserves the headers required for replay.

use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    models::{CandidateKind, CandidateProtection, CandidateValidationState, MediaCandidate},
    Result,
};

use super::{ExtractContext, ExtractResult, SourceExtractor};

const HANIME_MOBILE_UA: &str = "Mozilla/5.0 (Linux; Android 8.0; Pixel 2 Build/OPD3.170816.012) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.88 Mobile Safari/537.36 Edg/87.0.664.66";

pub struct HanimeExtractor;

#[async_trait]
impl SourceExtractor for HanimeExtractor {
    fn name(&self) -> &'static str {
        "hanime1"
    }

    fn matches(&self, ctx: &ExtractContext) -> bool {
        ctx.host()
            .map(|host| host == "hanime1.me" || host.ends_with(".hanime1.me"))
            .unwrap_or(false)
    }

    async fn extract(&self, ctx: &ExtractContext) -> Result<ExtractResult> {
        let client = reqwest::Client::builder()
            .user_agent(HANIME_MOBILE_UA)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;
        let response = client
            .get(ctx.url.clone())
            .header(reqwest::header::REFERER, "https://hanime1.me")
            .header(reqwest::header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.8")
            .send()
            .await?;
        let status = response.status();
        let html = response.text().await?;
        if status == StatusCode::FORBIDDEN && looks_like_cloudflare(&html) {
            return Ok(ExtractResult {
                candidates: Vec::new(),
                warnings: vec!["hanime1 cloudflare_blocked: HTTP 403".to_string()],
                browser_session: None,
            });
        }
        if !status.is_success() {
            return Ok(ExtractResult {
                candidates: Vec::new(),
                warnings: vec![format!("hanime1 returned HTTP {status}")],
                browser_session: None,
            });
        }

        if looks_like_cloudflare(&html) {
            return Ok(ExtractResult {
                candidates: Vec::new(),
                warnings: vec!["hanime1 page returned Cloudflare challenge".to_string()],
                browser_session: None,
            });
        }

        let urls = extract_media_urls(&html);
        let mut candidates = Vec::new();
        for (index, url) in urls.into_iter().enumerate() {
            let quality = quality_label(&url);
            candidates.push(MediaCandidate {
                id: Uuid::new_v4(),
                job_id: ctx.job_id,
                url: url.clone(),
                kind: if url.to_ascii_lowercase().contains(".m3u8") {
                    CandidateKind::Manifest
                } else {
                    CandidateKind::Video
                },
                extractor: "hanime1".to_string(),
                method: "mobile_html".to_string(),
                status: None,
                content_type: if url.to_ascii_lowercase().contains(".m3u8") {
                    Some("application/vnd.apple.mpegurl".to_string())
                } else {
                    Some("video/mp4".to_string())
                },
                content_length: None,
                resource_type: Some("hanime1_inline_media".to_string()),
                initiator_url: Some(ctx.source_url.clone()),
                quality_label: quality.clone(),
                score: 230 + quality_score(quality.as_deref()) - (index as i64),
                requires_authorization: false,
                platform: Some(ctx.platform_hint),
                route: Some("hanime1/mobile_html".to_string()),
                extractor_confidence: Some(78),
                protection: Some(CandidateProtection::SignedUrl),
                requires_profile: false,
                ttl_hint_seconds: Some(14_400),
                ad_risk: false,
                evidence_count: 1,
                paired_candidate_ids: Vec::new(),
                failure_reason: None,
                validation_state: Some(CandidateValidationState::Untested),
                metadata_json: json!({
                    "source": "hanime1_mobile_html",
                    "download_headers": {
                        "User-Agent": HANIME_MOBILE_UA,
                        "Referer": "https://hanime1.me",
                        "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8"
                    }
                }),
                created_at: OffsetDateTime::now_utc(),
                score_breakdown_json: json!({
                    "engine": "hanime1",
                    "quality": quality_score(quality.as_deref()),
                    "total": 230 + quality_score(quality.as_deref()) - (index as i64),
                }),
                selected: false,
                selection_reason: None,
                validation_status: None,
                resolved_ip: None,
                final_url_after_redirects: None,
                expires_at: None,
                discovered_by_event_id: None,
            });
        }

        Ok(ExtractResult::candidates(candidates))
    }
}

fn looks_like_cloudflare(html: &str) -> bool {
    html.contains("Attention Required") || html.contains("cf-browser-verification")
}

fn extract_media_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let normalized = html.replace("\\/", "/").replace("&amp;", "&");
    let bytes = normalized.as_bytes();
    let mut index = 0usize;
    while let Some(offset) = normalized[index..].find("https://") {
        let start = index + offset;
        let mut end = start;
        while end < bytes.len() {
            let byte = bytes[end];
            if matches!(
                byte,
                b'"' | b'\'' | b'<' | b'>' | b'\\' | b' ' | b'\n' | b'\r' | b'\t'
            ) {
                break;
            }
            end += 1;
        }
        let raw = normalized[start..end].to_string();
        if is_hanime_media_url(&raw) && !urls.contains(&raw) {
            urls.push(raw);
        }
        index = end.saturating_add(1);
    }
    urls.sort_by_key(|url| -quality_score(quality_label(url).as_deref()));
    urls
}

fn is_hanime_media_url(url: &str) -> bool {
    let lowered = url.to_ascii_lowercase();
    (lowered.contains("hembed.com") || lowered.contains("saawsedge.com"))
        && (lowered.contains(".mp4") || lowered.contains(".m3u8"))
}

fn quality_label(url: &str) -> Option<String> {
    for quality in ["2160p", "1440p", "1080p", "720p", "480p", "360p", "240p"] {
        if url.to_ascii_lowercase().contains(quality) {
            return Some(quality.to_string());
        }
    }
    None
}

fn quality_score(quality: Option<&str>) -> i64 {
    match quality {
        Some("2160p") => 2160,
        Some("1440p") => 1440,
        Some("1080p") => 1080,
        Some("720p") => 720,
        Some("480p") => 480,
        Some("360p") => 360,
        Some("240p") => 240,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_ranks_hanime_mp4_urls() {
        let html = r#"
          <script>
          var a = "https:\/\/vdownload.hembed.com\/406643-720p.mp4?secure=a";
          var b = "https:\/\/vdownload.hembed.com\/406643-1080p.mp4?secure=b";
          </script>
        "#;
        let urls = extract_media_urls(html);
        assert_eq!(
            urls[0],
            "https://vdownload.hembed.com/406643-1080p.mp4?secure=b"
        );
        assert_eq!(
            urls[1],
            "https://vdownload.hembed.com/406643-720p.mp4?secure=a"
        );
    }

    #[test]
    fn detects_cloudflare_challenge() {
        assert!(looks_like_cloudflare(
            "<title>Attention Required! | Cloudflare</title>"
        ));
    }
}
