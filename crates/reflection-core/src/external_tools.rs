use std::{path::PathBuf, time::Duration};

use serde_json::Value;
use time::OffsetDateTime;
use tokio::{process::Command, time as tokio_time};
use uuid::Uuid;

use crate::{
    models::{
        CandidateKind, CandidateProtection, CandidateValidationState, MediaCandidate, OutputKind,
    },
    url_policy::parse_and_validate_url,
    Result, RkError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalToolKind {
    YouGet,
    Lux,
    Streamlink,
}

impl ExternalToolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::YouGet => "you_get",
            Self::Lux => "lux",
            Self::Streamlink => "streamlink",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExternalToolProbe {
    kind: ExternalToolKind,
    path: PathBuf,
    timeout: Duration,
    max_json_bytes: usize,
}

impl ExternalToolProbe {
    pub fn new(
        kind: ExternalToolKind,
        path: PathBuf,
        timeout: Duration,
        max_json_bytes: usize,
    ) -> Self {
        Self {
            kind,
            path,
            timeout,
            max_json_bytes,
        }
    }

    pub fn kind(&self) -> ExternalToolKind {
        self.kind
    }

    pub async fn probe(
        &self,
        job_id: Uuid,
        source_url: &str,
        outputs: &[OutputKind],
    ) -> Result<Vec<MediaCandidate>> {
        parse_and_validate_url(source_url)?;
        let mut command = Command::new(&self.path);
        match self.kind {
            ExternalToolKind::YouGet => {
                command.arg("--json").arg(source_url);
            }
            ExternalToolKind::Lux => {
                command.arg("-j").arg(source_url);
            }
            ExternalToolKind::Streamlink => {
                command.arg("--json").arg(source_url);
            }
        }

        let output = tokio_time::timeout(self.timeout, command.output())
            .await
            .map_err(|_| RkError::Source(format!("{} probe timed out", self.kind.as_str())))??;
        if !output.status.success() {
            return Err(RkError::Source(format!(
                "{} probe exited with {}: {}",
                self.kind.as_str(),
                output
                    .status
                    .code()
                    .map_or_else(|| "signal".to_string(), |code| code.to_string()),
                limited_stderr(&output.stderr)
            )));
        }
        if output.stdout.len() > self.max_json_bytes {
            return Err(RkError::Source(format!(
                "{} JSON exceeded {} bytes",
                self.kind.as_str(),
                self.max_json_bytes
            )));
        }
        parse_external_json(job_id, self.kind, &output.stdout, outputs)
    }
}

pub fn parse_external_json(
    job_id: Uuid,
    kind: ExternalToolKind,
    bytes: &[u8],
    outputs: &[OutputKind],
) -> Result<Vec<MediaCandidate>> {
    let value: Value = serde_json::from_slice(bytes)?;
    let mut found = Vec::new();
    scan_json(&value, "$", &mut found);

    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in found {
        if !seen.insert(raw.url.clone()) {
            continue;
        }
        if parse_and_validate_url(&raw.url).is_err() {
            continue;
        }
        let kind_value = classify_url(&raw.url, raw.kind_hint.as_deref(), raw.ext.as_deref());
        if !kind_matches_outputs(kind_value, outputs) {
            continue;
        }
        let ad_risk = is_likely_ad_or_tracking_url(&raw.url);
        let signed = is_signed_url(&raw.url);
        let score = score_candidate(kind_value, raw.height, raw.bitrate, ad_risk, outputs);
        candidates.push(MediaCandidate {
            id: Uuid::new_v4(),
            job_id,
            url: raw.url,
            kind: kind_value,
            extractor: kind.as_str().to_string(),
            method: "json_probe".to_string(),
            status: None,
            content_type: content_type_hint(kind_value, raw.ext.as_deref()),
            content_length: raw.size,
            resource_type: raw.stream_id.clone(),
            initiator_url: None,
            quality_label: raw
                .height
                .map(|height| format!("{height}p"))
                .or(raw.quality)
                .or(raw.stream_id),
            score,
            requires_authorization: false,
            platform: None,
            route: Some(format!("external:{}", kind.as_str())),
            extractor_confidence: Some(match kind {
                ExternalToolKind::YouGet => 68,
                ExternalToolKind::Lux => 65,
                ExternalToolKind::Streamlink => 72,
            }),
            protection: Some(if signed {
                CandidateProtection::SignedUrl
            } else {
                CandidateProtection::None
            }),
            requires_profile: false,
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
                "source": kind.as_str(),
                "path": raw.path,
                "ext": raw.ext,
                "height": raw.height,
                "bitrate": raw.bitrate,
            }),
            created_at: OffsetDateTime::now_utc(),
            score_breakdown_json: serde_json::json!({
                "engine": kind.as_str(),
                "height": raw.height,
                "bitrate": raw.bitrate,
                "ad_risk": ad_risk,
                "total": score,
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
    candidates.sort_by_key(|candidate| -candidate.score);
    candidates.truncate(50);
    Ok(candidates)
}

#[derive(Debug, Clone)]
struct RawUrl {
    url: String,
    path: String,
    kind_hint: Option<String>,
    ext: Option<String>,
    quality: Option<String>,
    stream_id: Option<String>,
    height: Option<i64>,
    bitrate: Option<i64>,
    size: Option<i64>,
}

fn scan_json(value: &Value, path: &str, out: &mut Vec<RawUrl>) {
    match value {
        Value::String(text) => {
            if is_http_url(text) {
                out.push(RawUrl {
                    url: text.clone(),
                    path: path.to_string(),
                    kind_hint: None,
                    ext: extension_from_url(text),
                    quality: quality_from_text(text),
                    stream_id: None,
                    height: quality_height_from_text(text),
                    bitrate: None,
                    size: None,
                });
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                scan_json(item, &format!("{path}[{index}]"), out);
            }
        }
        Value::Object(map) => {
            let url = first_string(map, &["url", "src", "download_url", "stream_url"]);
            if let Some(url) = url.filter(|url| is_http_url(url)) {
                out.push(RawUrl {
                    url,
                    path: path.to_string(),
                    kind_hint: first_string(map, &["type", "kind", "container", "protocol"]),
                    ext: first_string(map, &["ext", "extension", "container"])
                        .or_else(|| first_string(map, &["format"])),
                    quality: first_string(map, &["quality", "format", "format_note", "name"]),
                    stream_id: first_string(map, &["id", "itag", "format_id", "stream_id"]),
                    height: first_i64(map, &["height", "video_height"]),
                    bitrate: first_i64(map, &["bitrate", "tbr", "abr", "vbr"]),
                    size: first_i64(map, &["size", "filesize", "content_length"]),
                });
            }
            for (key, item) in map {
                scan_json(item, &format!("{path}.{key}"), out);
            }
        }
        _ => {}
    }
}

fn first_string(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        map.get(*key).and_then(|value| match value {
            Value::String(text) if !text.is_empty() => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
    })
}

fn first_i64(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        map.get(*key).and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|number| number.round() as i64))
                .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
        })
    })
}

fn classify_url(url: &str, kind_hint: Option<&str>, ext: Option<&str>) -> CandidateKind {
    let hint = kind_hint.unwrap_or_default().to_ascii_lowercase();
    if hint.contains("audio") {
        return CandidateKind::Audio;
    }
    if hint.contains("video") {
        return CandidateKind::Video;
    }
    let ext = ext
        .map(|value| value.trim_start_matches('.').to_ascii_lowercase())
        .or_else(|| extension_from_url(url))
        .unwrap_or_default();
    match ext.as_str() {
        "m3u8" | "mpd" => CandidateKind::Manifest,
        "mp3" | "m4a" | "aac" | "opus" | "ogg" | "wav" | "flac" => CandidateKind::Audio,
        "mp4" | "m4v" | "webm" | "mov" | "mkv" | "flv" | "ts" | "m4s" => CandidateKind::Video,
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" => CandidateKind::Image,
        _ if url.to_ascii_lowercase().contains(".m3u8") => CandidateKind::Manifest,
        _ => CandidateKind::Unknown,
    }
}

fn kind_matches_outputs(kind: CandidateKind, outputs: &[OutputKind]) -> bool {
    match kind {
        CandidateKind::Audio => outputs.contains(&OutputKind::Audio),
        CandidateKind::Video | CandidateKind::Manifest => {
            outputs.contains(&OutputKind::Video) || outputs.contains(&OutputKind::Audio)
        }
        CandidateKind::Image => outputs.contains(&OutputKind::Image),
        CandidateKind::Html => outputs.contains(&OutputKind::PageHtml),
        CandidateKind::Unknown => false,
    }
}

fn score_candidate(
    kind: CandidateKind,
    height: Option<i64>,
    bitrate: Option<i64>,
    ad_risk: bool,
    outputs: &[OutputKind],
) -> i64 {
    let mut score = match kind {
        CandidateKind::Video => 78,
        CandidateKind::Manifest => 76,
        CandidateKind::Audio => 70,
        CandidateKind::Image => 35,
        CandidateKind::Html | CandidateKind::Unknown => 5,
    };
    if let Some(height) = height {
        score += (height / 36).clamp(0, 45);
    }
    if let Some(bitrate) = bitrate {
        score += (bitrate / 250).clamp(0, 20);
    }
    if outputs.contains(&OutputKind::Audio) && !outputs.contains(&OutputKind::Video) {
        match kind {
            CandidateKind::Audio => score += 45,
            CandidateKind::Manifest => score += 10,
            CandidateKind::Video => score -= 30,
            _ => {}
        }
    }
    if ad_risk {
        score -= 80;
    }
    score
}

fn content_type_hint(kind: CandidateKind, ext: Option<&str>) -> Option<String> {
    let ext = ext?.trim_start_matches('.').to_ascii_lowercase();
    let value = match ext.as_str() {
        "m3u8" => "application/vnd.apple.mpegurl",
        "mpd" => "application/dash+xml",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "opus" => "audio/opus",
        "ogg" => "audio/ogg",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => match kind {
            CandidateKind::Audio => "audio/*",
            CandidateKind::Video => "video/*",
            CandidateKind::Image => "image/*",
            _ => return None,
        },
    };
    Some(value.to_string())
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn extension_from_url(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    parsed
        .path()
        .rsplit('/')
        .next()
        .and_then(|segment| segment.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
}

fn quality_from_text(value: &str) -> Option<String> {
    quality_height_from_text(value).map(|height| format!("{height}p"))
}

fn quality_height_from_text(value: &str) -> Option<i64> {
    for token in value.split(|ch: char| !ch.is_ascii_alphanumeric()) {
        let Some(raw) = token.strip_suffix('p').or_else(|| token.strip_suffix('P')) else {
            continue;
        };
        if let Ok(height) = raw.parse::<i64>() {
            if (144..=4320).contains(&height) {
                return Some(height);
            }
        }
    }
    None
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

fn limited_stderr(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim();
    if text.len() > 500 {
        format!("{}...", &text[..500])
    } else if text.is_empty() {
        "no stderr".to_string()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_generic_external_json_candidates() {
        let json = br#"
        {
          "streams": {
            "1080p": {
              "url": "https://github.com/media/video_1080p.mp4?sign=abc",
              "height": 1080,
              "ext": "mp4",
              "size": 2048
            },
            "ad": {
              "url": "https://github.com/ads/preroll.mp4",
              "height": 360,
              "ext": "mp4"
            }
          }
        }
        "#;

        let candidates = parse_external_json(
            Uuid::new_v4(),
            ExternalToolKind::YouGet,
            json,
            &[OutputKind::Video],
        )
        .unwrap();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].kind, CandidateKind::Video);
        assert_eq!(
            candidates[0].protection,
            Some(CandidateProtection::SignedUrl)
        );
        assert!(candidates.iter().any(|candidate| candidate.ad_risk));
    }

    #[test]
    fn filters_candidates_by_requested_output() {
        let json = br#"
        {
          "audio": { "url": "https://github.com/media/audio.m4a", "ext": "m4a" },
          "video": { "url": "https://github.com/media/video.mp4", "ext": "mp4" }
        }
        "#;

        let candidates = parse_external_json(
            Uuid::new_v4(),
            ExternalToolKind::Lux,
            json,
            &[OutputKind::Audio],
        )
        .unwrap();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].kind, CandidateKind::Audio);
    }
}
