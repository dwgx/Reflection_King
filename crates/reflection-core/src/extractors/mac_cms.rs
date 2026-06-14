//! MacCMS episode-page extractor.
//!
//! A large class of anime/video mirrors expose the active route through
//! `player_aaaa`. Browser clicking can easily switch to the next episode, so
//! this adapter reads the route metadata directly and validates HLS manifests
//! with GET requests before offering them as candidates.

use async_trait::async_trait;
use base64::Engine;
use reqwest::{header, Client, StatusCode, Url};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    models::{CandidateKind, CandidateProtection, CandidateValidationState, MediaCandidate},
    Result,
};

use super::{ExtractContext, ExtractResult, SourceExtractor};

const DESKTOP_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0 Safari/537.36";
const PLAYCONF_PREFIX: &str = "player_aaaa";

pub struct MacCmsEpisodeExtractor;

#[derive(Debug, Clone)]
struct PlayerConfig {
    url: String,
    url_next: Option<String>,
    from: Option<String>,
    link: Option<String>,
    link_next: Option<String>,
    id: Option<String>,
    sid: Option<i64>,
    nid: Option<i64>,
    encrypt: i64,
}

#[derive(Debug, Clone)]
struct ValidatedMedia {
    original_url: String,
    playable_url: String,
    status: u16,
    content_type: Option<String>,
    failure_reason: Option<String>,
    validation_state: CandidateValidationState,
    protection: CandidateProtection,
    metadata: serde_json::Value,
}

#[async_trait]
impl SourceExtractor for MacCmsEpisodeExtractor {
    fn name(&self) -> &'static str {
        "mac_cms"
    }

    fn matches(&self, ctx: &ExtractContext) -> bool {
        ctx.host()
            .map(|host| {
                host == "dmttang.com"
                    || host.ends_with(".dmttang.com")
                    || host == "83dm.com"
                    || host.ends_with(".83dm.com")
                    || host.contains("yinghua")
            })
            .unwrap_or(false)
    }

    async fn extract(&self, ctx: &ExtractContext) -> Result<ExtractResult> {
        let client = Client::builder()
            .user_agent(DESKTOP_UA)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;

        let html = client
            .get(ctx.url.clone())
            .header(header::REFERER, ctx.source_url.as_str())
            .header(header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.8")
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let Some(config) = extract_player_config(&html)? else {
            return Ok(ExtractResult {
                candidates: Vec::new(),
                warnings: vec!["mac_cms player_aaaa not found".to_string()],
                browser_session: None,
                page_snapshot: None,
            });
        };

        let mut warnings = Vec::new();
        let mut raw_urls = Vec::new();
        if let Some(url) = decode_player_url(&config.url, config.encrypt) {
            raw_urls.push(("current", url));
        } else {
            warnings.push("mac_cms current url could not be decoded".to_string());
        }
        if let Some(next_url) = config
            .url_next
            .as_deref()
            .and_then(|url| decode_player_url(url, config.encrypt))
        {
            raw_urls.push(("next", next_url));
        }

        let mut candidates = Vec::new();
        for (position, (slot, raw_url)) in raw_urls.into_iter().enumerate() {
            let Ok(resolved_url) = ctx.url.join(&raw_url) else {
                warnings.push(format!("mac_cms ignored invalid media url: {raw_url}"));
                continue;
            };
            let validated = validate_media_url(&client, &resolved_url, &ctx.source_url).await;
            if let Err(error) = &validated {
                warnings.push(format!("mac_cms validation failed for {slot}: {error}"));
            }
            let validated = validated.unwrap_or_else(|error| ValidatedMedia {
                original_url: resolved_url.to_string(),
                playable_url: resolved_url.to_string(),
                status: 0,
                content_type: None,
                failure_reason: Some(error),
                validation_state: CandidateValidationState::Failed,
                protection: CandidateProtection::Unknown,
                metadata: json!({}),
            });
            candidates.push(candidate_from_validated(
                ctx, &config, &validated, slot, position,
            ));
        }

        Ok(ExtractResult {
            candidates,
            warnings,
            browser_session: None,
            page_snapshot: None,
        })
    }
}

fn candidate_from_validated(
    ctx: &ExtractContext,
    config: &PlayerConfig,
    media: &ValidatedMedia,
    slot: &str,
    position: usize,
) -> MediaCandidate {
    let is_manifest = media.playable_url.to_ascii_lowercase().contains(".m3u8");
    let usable_bonus = if media.validation_state == CandidateValidationState::Usable {
        120
    } else {
        -600
    };
    MediaCandidate {
        id: Uuid::new_v4(),
        job_id: ctx.job_id,
        url: media.playable_url.clone(),
        kind: if is_manifest {
            CandidateKind::Manifest
        } else {
            CandidateKind::Video
        },
        extractor: "mac_cms".to_string(),
        method: "player_aaaa".to_string(),
        status: if media.status == 0 {
            None
        } else {
            Some(media.status)
        },
        content_type: media.content_type.clone().or_else(|| {
            if is_manifest {
                Some("application/vnd.apple.mpegurl".to_string())
            } else {
                Some("video/mp4".to_string())
            }
        }),
        content_length: None,
        resource_type: config.from.clone().or_else(|| Some("mac_cms".to_string())),
        initiator_url: Some(ctx.source_url.clone()),
        quality_label: quality_label(&media.playable_url),
        score: 210 + usable_bonus - position as i64,
        requires_authorization: false,
        platform: Some(ctx.platform_hint),
        route: config
            .from
            .as_ref()
            .map(|from| format!("mac_cms/{from}/{slot}")),
        extractor_confidence: Some(
            if media.validation_state == CandidateValidationState::Usable {
                82
            } else {
                55
            },
        ),
        protection: Some(media.protection),
        requires_profile: false,
        ttl_hint_seconds: signed_url_ttl_hint(&media.playable_url),
        ad_risk: false,
        evidence_count: if media.validation_state == CandidateValidationState::Usable {
            3
        } else {
            1
        },
        paired_candidate_ids: Vec::new(),
        failure_reason: media.failure_reason.clone(),
        validation_state: Some(media.validation_state),
        metadata_json: json!({
            "source": "mac_cms_player_aaaa",
            "player": {
                "from": config.from,
                "id": config.id,
                "sid": config.sid,
                "nid": config.nid,
                "link": config.link,
                "link_next": config.link_next,
                "slot": slot,
                "original_url": media.original_url,
            },
            "validation": media.metadata,
            "download_headers": {
                "User-Agent": DESKTOP_UA,
                "Referer": ctx.source_url,
                "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
                "Accept": "*/*"
            }
        }),
        created_at: OffsetDateTime::now_utc(),
        score_breakdown_json: json!({
            "engine": "mac_cms",
            "usable": media.validation_state == CandidateValidationState::Usable,
            "total": 210 + usable_bonus - position as i64,
        }),
        selected: false,
        selection_reason: None,
        validation_status: None,
        resolved_ip: None,
        final_url_after_redirects: None,
        expires_at: None,
        discovered_by_event_id: None,
    }
}

async fn validate_media_url(
    client: &Client,
    url: &Url,
    referer: &str,
) -> std::result::Result<ValidatedMedia, String> {
    let original_url = url.to_string();
    let lower_url = url.as_str().to_ascii_lowercase();
    let initial_range = if lower_url.contains(".m3u8") {
        None
    } else {
        Some("bytes=0-16383")
    };
    let response = get_small(client, url.clone(), referer, initial_range).await?;
    let status = response.status;
    if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
        return Ok(failed_media(
            original_url,
            url.to_string(),
            status,
            response.content_type,
            classify_http_failure(status, &response.body),
            &response.body,
        ));
    }

    if lower_url.contains(".m3u8") || looks_like_m3u8(&response.body) {
        return validate_hls(client, url, referer, response).await;
    }

    Ok(ValidatedMedia {
        original_url,
        playable_url: url.to_string(),
        status: status.as_u16(),
        content_type: response.content_type,
        failure_reason: None,
        validation_state: CandidateValidationState::Usable,
        protection: protection_for_url(url.as_str()),
        metadata: json!({
            "master_status": status.as_u16(),
            "kind": "file"
        }),
    })
}

async fn validate_hls(
    client: &Client,
    master_url: &Url,
    referer: &str,
    master: SmallResponse,
) -> std::result::Result<ValidatedMedia, String> {
    let Some(next_url) = first_hls_child(master_url, &master.body) else {
        return Ok(ValidatedMedia {
            original_url: master_url.to_string(),
            playable_url: master_url.to_string(),
            status: master.status.as_u16(),
            content_type: master.content_type,
            failure_reason: None,
            validation_state: CandidateValidationState::Usable,
            protection: protection_for_url(master_url.as_str()),
            metadata: json!({
                "master_status": master.status.as_u16(),
                "playlist_type": "media_or_single_level"
            }),
        });
    };

    let variant = get_small(client, next_url.clone(), referer, None).await?;
    if !variant.status.is_success() {
        return Ok(failed_media(
            master_url.to_string(),
            master_url.to_string(),
            variant.status,
            variant.content_type,
            classify_http_failure(variant.status, &variant.body),
            &variant.body,
        ));
    }

    let segment = first_hls_segment(&next_url, &variant.body);
    if let Some(segment_url) = segment {
        let segment_response =
            get_small(client, segment_url.clone(), referer, Some("bytes=0-1023")).await?;
        if !segment_response.status.is_success()
            && segment_response.status != StatusCode::PARTIAL_CONTENT
        {
            return Ok(failed_media(
                master_url.to_string(),
                master_url.to_string(),
                segment_response.status,
                segment_response.content_type,
                classify_http_failure(segment_response.status, &segment_response.body),
                &segment_response.body,
            ));
        }
        return Ok(ValidatedMedia {
            original_url: master_url.to_string(),
            playable_url: master_url.to_string(),
            status: master.status.as_u16(),
            content_type: master.content_type,
            failure_reason: None,
            validation_state: CandidateValidationState::Usable,
            protection: protection_for_url(master_url.as_str()),
            metadata: json!({
                "master_status": master.status.as_u16(),
                "variant_status": variant.status.as_u16(),
                "segment_status": segment_response.status.as_u16(),
                "variant_url": next_url.to_string(),
                "segment_url": segment_url.to_string(),
                "playlist_type": "master_variant"
            }),
        });
    }

    Ok(ValidatedMedia {
        original_url: master_url.to_string(),
        playable_url: master_url.to_string(),
        status: master.status.as_u16(),
        content_type: master.content_type,
        failure_reason: None,
        validation_state: CandidateValidationState::Usable,
        protection: protection_for_url(master_url.as_str()),
        metadata: json!({
            "master_status": master.status.as_u16(),
            "variant_status": variant.status.as_u16(),
            "variant_url": next_url.to_string(),
            "playlist_type": "master_variant_without_segment_sample"
        }),
    })
}

#[derive(Debug)]
struct SmallResponse {
    status: StatusCode,
    content_type: Option<String>,
    body: String,
}

async fn get_small(
    client: &Client,
    url: Url,
    referer: &str,
    range: Option<&str>,
) -> std::result::Result<SmallResponse, String> {
    let mut request = client
        .get(url)
        .header(header::USER_AGENT, DESKTOP_UA)
        .header(header::REFERER, referer)
        .header(header::ACCEPT, "*/*")
        .header(header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.8");
    if let Some(range) = range {
        request = request.header(header::RANGE, range);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_string());
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("read failed: {error}"))?;
    let body = String::from_utf8_lossy(&bytes[..bytes.len().min(16_384)]).to_string();
    Ok(SmallResponse {
        status,
        content_type,
        body,
    })
}

fn failed_media(
    original_url: String,
    playable_url: String,
    status: StatusCode,
    content_type: Option<String>,
    reason: String,
    body: &str,
) -> ValidatedMedia {
    let state = if reason.contains("region")
        || body.contains("国内网络")
        || body.contains("当前区域禁止访问")
        || body.to_ascii_lowercase().contains("the region")
    {
        CandidateValidationState::RegionBlocked
    } else {
        CandidateValidationState::Failed
    };
    let protection = if state == CandidateValidationState::RegionBlocked {
        CandidateProtection::RegionBlocked
    } else {
        CandidateProtection::Unknown
    };
    ValidatedMedia {
        original_url,
        playable_url,
        status: status.as_u16(),
        content_type,
        failure_reason: Some(reason.clone()),
        validation_state: state,
        protection,
        metadata: json!({
            "status": status.as_u16(),
            "failure": reason,
            "body_sample": body.chars().take(240).collect::<String>(),
        }),
    }
}

fn classify_http_failure(status: StatusCode, body: &str) -> String {
    if body.contains("国内网络") {
        return format!("cdn region blocked: HTTP {status}");
    }
    if body.contains("当前区域禁止访问") || body.to_ascii_lowercase().contains("the region")
    {
        return format!("cdn region blocked: HTTP {status}");
    }
    if status == StatusCode::FORBIDDEN {
        return "cdn forbidden: HTTP 403".to_string();
    }
    if status == StatusCode::NOT_FOUND {
        return "cdn not found: HTTP 404".to_string();
    }
    format!("cdn returned HTTP {status}")
}

fn extract_player_config(html: &str) -> Result<Option<PlayerConfig>> {
    let Some(prefix_start) = html.find(PLAYCONF_PREFIX) else {
        return Ok(None);
    };
    let Some(brace_offset) = html[prefix_start..].find('{') else {
        return Ok(None);
    };
    let start = prefix_start + brace_offset;
    let Some(end) = find_matching_brace(&html[start..]) else {
        return Ok(None);
    };
    let json_text = &html[start..start + end + 1];
    let value: serde_json::Value = serde_json::from_str(json_text)?;
    Ok(Some(PlayerConfig {
        url: value
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        url_next: value
            .get("url_next")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        from: value
            .get("from")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        link: value
            .get("link")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        link_next: value
            .get("link_next")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        id: value
            .get("id")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        sid: value.get("sid").and_then(|value| value.as_i64()),
        nid: value.get("nid").and_then(|value| value.as_i64()),
        encrypt: value
            .get("encrypt")
            .and_then(|value| value.as_i64())
            .unwrap_or(0),
    }))
}

fn find_matching_brace(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn decode_player_url(value: &str, encrypt: i64) -> Option<String> {
    match encrypt {
        0 => Some(value.to_string()),
        1 => percent_decode(value),
        2 => {
            let decoded = percent_decode(value)?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(decoded.as_bytes())
                .ok()?;
            let text = String::from_utf8(bytes).ok()?;
            percent_decode(&text).or(Some(text))
        }
        _ => Some(value.to_string()),
    }
    .filter(|value| !value.trim().is_empty())
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = percent_decode_bytes(value);
    String::from_utf8(bytes).ok()
}

fn percent_decode_bytes(value: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                    continue;
                }
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

fn looks_like_m3u8(body: &str) -> bool {
    body.trim_start().starts_with("#EXTM3U")
}

fn first_hls_child(base: &Url, playlist: &str) -> Option<Url> {
    let mut saw_stream_inf = false;
    for line in playlist.lines().map(str::trim) {
        if line.starts_with("#EXT-X-STREAM-INF") {
            saw_stream_inf = true;
            continue;
        }
        if saw_stream_inf && !line.is_empty() && !line.starts_with('#') {
            return base.join(line).ok();
        }
    }
    None
}

fn first_hls_segment(base: &Url, playlist: &str) -> Option<Url> {
    for line in playlist.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.contains(".ts") || line.contains(".m4s") || line.contains(".mp4") {
            return base.join(line).ok();
        }
    }
    None
}

fn quality_label(url: &str) -> Option<String> {
    for quality in ["2160p", "1440p", "1080p", "720p", "480p", "360p", "240p"] {
        if url.to_ascii_lowercase().contains(quality) {
            return Some(quality.to_string());
        }
    }
    None
}

fn protection_for_url(url: &str) -> CandidateProtection {
    let lowered = url.to_ascii_lowercase();
    if [
        "expires=",
        "expire=",
        "signature=",
        "sign=",
        "token=",
        "auth_key=",
        "hash=",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
    {
        CandidateProtection::SignedUrl
    } else {
        CandidateProtection::None
    }
}

fn signed_url_ttl_hint(url: &str) -> Option<i64> {
    if protection_for_url(url) == CandidateProtection::SignedUrl {
        Some(14_400)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_player_aaaa_config() {
        let html = r#"
        <script>
        var player_aaaa={"flag":"play","encrypt":0,"link":"/vodplay/872-1-1.html","link_next":"/vodplay/872-14-2.html","url":"https://example.com/master.m3u8","url_next":"https://example.com/next.m3u8","from":"lzm3u8","id":"872","sid":14,"nid":1}
        </script>
        "#;
        let config = extract_player_config(html).unwrap().unwrap();
        assert_eq!(config.url, "https://example.com/master.m3u8");
        assert_eq!(config.from.as_deref(), Some("lzm3u8"));
        assert_eq!(config.sid, Some(14));
        assert_eq!(config.nid, Some(1));
    }

    #[test]
    fn decodes_legacy_encrypt_modes() {
        assert_eq!(
            decode_player_url("https%3A%2F%2Fexample.com%2Fa.m3u8", 1).as_deref(),
            Some("https://example.com/a.m3u8")
        );
        assert_eq!(
            decode_player_url("aHR0cHM6Ly9leGFtcGxlLmNvbS9iLm0zdTg=", 2).as_deref(),
            Some("https://example.com/b.m3u8")
        );
    }

    #[test]
    fn resolves_hls_variant_and_segment() {
        let base = Url::parse("https://cdn.example.com/a/master.m3u8").unwrap();
        let child = first_hls_child(
            &base,
            "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=3000\n3000k/hls/mixed.m3u8\n",
        )
        .unwrap();
        assert_eq!(
            child.as_str(),
            "https://cdn.example.com/a/3000k/hls/mixed.m3u8"
        );
        let segment = first_hls_segment(&child, "#EXTM3U\n#EXTINF:2,\n0001.ts?hash=abc\n").unwrap();
        assert_eq!(
            segment.as_str(),
            "https://cdn.example.com/a/3000k/hls/0001.ts?hash=abc"
        );
    }
}
