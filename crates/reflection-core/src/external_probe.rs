use std::{cmp::Reverse, collections::HashSet, path::PathBuf, time::Duration};

use reqwest::header::HeaderMap;
use serde::Deserialize;
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

#[derive(Debug, Clone)]
pub struct YtDlpProbe {
    path: PathBuf,
    timeout: Duration,
    max_json_bytes: usize,
}

impl YtDlpProbe {
    pub fn new(path: PathBuf, timeout: Duration, max_json_bytes: usize) -> Self {
        Self {
            path,
            timeout,
            max_json_bytes,
        }
    }

    pub async fn probe(
        &self,
        job_id: Uuid,
        source_url: &str,
        outputs: &[OutputKind],
    ) -> Result<Vec<MediaCandidate>> {
        self.probe_with_headers(job_id, source_url, outputs, &HeaderMap::new())
            .await
    }

    pub async fn probe_with_headers(
        &self,
        job_id: Uuid,
        source_url: &str,
        outputs: &[OutputKind],
        headers: &HeaderMap,
    ) -> Result<Vec<MediaCandidate>> {
        parse_and_validate_url(source_url)?;

        let mut command = Command::new(&self.path);
        command
            .arg("--dump-single-json")
            .arg("--no-playlist")
            .arg("--skip-download")
            .arg("--no-warnings")
            .arg("--no-cache-dir")
            .args(yt_dlp_header_args(headers))
            .arg(source_url);

        let output = tokio_time::timeout(self.timeout, command.output())
            .await
            .map_err(|_| RkError::Source("yt-dlp probe timed out".to_string()))??;

        if !output.status.success() {
            return Err(RkError::Source(format!(
                "yt-dlp probe exited with {}: {}",
                output
                    .status
                    .code()
                    .map_or_else(|| "signal".to_string(), |code| code.to_string()),
                limited_stderr(&output.stderr)
            )));
        }

        if output.stdout.len() > self.max_json_bytes {
            return Err(RkError::Source(format!(
                "yt-dlp JSON exceeded {} bytes",
                self.max_json_bytes
            )));
        }

        parse_yt_dlp_json(job_id, &output.stdout, outputs)
    }
}

fn yt_dlp_header_args(headers: &HeaderMap) -> Vec<String> {
    let mut args = Vec::new();
    for (name, value) in headers {
        if let Ok(value) = value.to_str() {
            args.push("--add-header".to_string());
            args.push(format!("{}: {}", name.as_str(), value));
        }
    }
    args
}

#[derive(Debug, Deserialize)]
struct YtDlpInfo {
    id: Option<String>,
    title: Option<String>,
    extractor: Option<String>,
    webpage_url: Option<String>,
    duration: Option<f64>,
    formats: Option<Vec<YtDlpFormat>>,
    url: Option<String>,
    ext: Option<String>,
    protocol: Option<String>,
    format_id: Option<String>,
    format_note: Option<String>,
    format: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    tbr: Option<f64>,
    abr: Option<f64>,
    vbr: Option<f64>,
    filesize: Option<i64>,
    filesize_approx: Option<i64>,
    acodec: Option<String>,
    vcodec: Option<String>,
    http_headers: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct YtDlpFormat {
    url: Option<String>,
    ext: Option<String>,
    protocol: Option<String>,
    format_id: Option<String>,
    format_note: Option<String>,
    format: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    tbr: Option<f64>,
    abr: Option<f64>,
    vbr: Option<f64>,
    filesize: Option<i64>,
    filesize_approx: Option<i64>,
    acodec: Option<String>,
    vcodec: Option<String>,
    http_headers: Option<serde_json::Value>,
}

pub fn parse_yt_dlp_json(
    job_id: Uuid,
    bytes: &[u8],
    outputs: &[OutputKind],
) -> Result<Vec<MediaCandidate>> {
    let info: YtDlpInfo = serde_json::from_slice(bytes)?;
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    if let Some(formats) = &info.formats {
        for format in formats {
            if let Some(candidate) = format_to_candidate(job_id, &info, format, outputs)? {
                if seen.insert(candidate.url.clone()) {
                    candidates.push(candidate);
                }
            }
        }
    }

    if candidates.is_empty() {
        let format = YtDlpFormat {
            url: info.url.clone(),
            ext: info.ext.clone(),
            protocol: info.protocol.clone(),
            format_id: info.format_id.clone(),
            format_note: info.format_note.clone(),
            format: info.format.clone(),
            width: info.width,
            height: info.height,
            tbr: info.tbr,
            abr: info.abr,
            vbr: info.vbr,
            filesize: info.filesize,
            filesize_approx: info.filesize_approx,
            acodec: info.acodec.clone(),
            vcodec: info.vcodec.clone(),
            http_headers: info.http_headers.clone(),
        };
        if let Some(candidate) = format_to_candidate(job_id, &info, &format, outputs)? {
            candidates.push(candidate);
        }
    }

    candidates.sort_by_key(|candidate| Reverse(candidate.score));
    candidates.truncate(50);
    Ok(candidates)
}

fn format_to_candidate(
    job_id: Uuid,
    info: &YtDlpInfo,
    format: &YtDlpFormat,
    outputs: &[OutputKind],
) -> Result<Option<MediaCandidate>> {
    let Some(url) = format.url.as_deref() else {
        return Ok(None);
    };
    if parse_and_validate_url(url).is_err() {
        return Ok(None);
    }

    let kind = classify_format(format);
    if !kind_matches_outputs(kind, outputs) {
        return Ok(None);
    }

    let score = score_format(kind, format, outputs);
    Ok(Some(MediaCandidate {
        id: Uuid::new_v4(),
        job_id,
        url: url.to_string(),
        kind,
        extractor: "yt_dlp".to_string(),
        method: "dump_single_json".to_string(),
        status: None,
        content_type: content_type_hint(format),
        content_length: format.filesize.or(format.filesize_approx),
        resource_type: format.protocol.clone(),
        initiator_url: info.webpage_url.clone(),
        quality_label: quality_label(format),
        score,
        requires_authorization: has_sensitive_headers(format.http_headers.as_ref()),
        platform: None,
        route: Some("external:yt_dlp".to_string()),
        extractor_confidence: Some(80),
        protection: Some(candidate_protection(format)),
        requires_profile: has_sensitive_headers(format.http_headers.as_ref()),
        ttl_hint_seconds: None,
        ad_risk: is_likely_ad_or_tracking_url(url),
        evidence_count: 1,
        paired_candidate_ids: Vec::new(),
        failure_reason: None,
        validation_state: Some(if is_likely_ad_or_tracking_url(url) {
            CandidateValidationState::SuspectAd
        } else {
            CandidateValidationState::Untested
        }),
        metadata_json: serde_json::json!({
            "source": "yt_dlp",
            "extractor": info.extractor,
            "source_id": info.id,
            "title": info.title,
            "duration": info.duration,
            "format_id": format.format_id,
            "format": format.format,
            "format_note": format.format_note,
            "ext": format.ext,
            "protocol": format.protocol,
            "width": format.width,
            "height": format.height,
            "tbr": format.tbr,
            "abr": format.abr,
            "vbr": format.vbr,
            "acodec": format.acodec,
            "vcodec": format.vcodec,
            "has_http_headers": format.http_headers.is_some(),
            "download_headers": safe_download_headers(format.http_headers.as_ref()),
        }),
        created_at: OffsetDateTime::now_utc(),
        score_breakdown_json: score_breakdown(kind, format, outputs),
        selected: false,
        selection_reason: None,
        validation_status: None,
        resolved_ip: None,
        final_url_after_redirects: None,
        expires_at: None,
        discovered_by_event_id: None,
    }))
}

fn classify_format(format: &YtDlpFormat) -> CandidateKind {
    if protocol_is_manifest(format.protocol.as_deref())
        || extension_is_manifest(format.ext.as_deref())
    {
        return CandidateKind::Manifest;
    }

    let acodec = format.acodec.as_deref().unwrap_or_default();
    let vcodec = format.vcodec.as_deref().unwrap_or_default();
    let has_audio = !acodec.is_empty() && acodec != "none";
    let has_video = !vcodec.is_empty() && vcodec != "none";

    if has_video {
        CandidateKind::Video
    } else if has_audio {
        CandidateKind::Audio
    } else {
        match format.ext.as_deref().unwrap_or_default() {
            "mp3" | "m4a" | "aac" | "opus" | "ogg" | "wav" | "flac" => CandidateKind::Audio,
            "mp4" | "mkv" | "mov" | "m4v" | "webm" => CandidateKind::Video,
            "jpg" | "jpeg" | "png" | "webp" => CandidateKind::Image,
            _ => CandidateKind::Unknown,
        }
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

fn score_format(kind: CandidateKind, format: &YtDlpFormat, outputs: &[OutputKind]) -> i64 {
    let mut score = match kind {
        CandidateKind::Audio => 70,
        CandidateKind::Video => 80,
        CandidateKind::Manifest => 75,
        CandidateKind::Image => 40,
        CandidateKind::Html | CandidateKind::Unknown => 10,
    };

    if let Some(height) = format.height {
        score += (height / 36).clamp(0, 30);
    }
    if let Some(abr) = format.abr {
        score += (abr / 16.0).round() as i64;
    }
    if let Some(tbr) = format.tbr {
        score += (tbr / 250.0).round() as i64;
    }
    if format.filesize.or(format.filesize_approx).is_some() {
        score += 4;
    }
    if protocol_is_manifest(format.protocol.as_deref()) {
        score += 8;
    }
    if outputs.contains(&OutputKind::Video) {
        score += mp4_compatibility_score(format);
    }
    if outputs.contains(&OutputKind::Audio) && !outputs.contains(&OutputKind::Video) {
        match kind {
            CandidateKind::Audio => score += 40,
            CandidateKind::Video => score -= 30,
            CandidateKind::Manifest => score += 10,
            CandidateKind::Image | CandidateKind::Html | CandidateKind::Unknown => {}
        }
    }

    score
}

/// Auditable breakdown of how `score_format` arrived at the candidate score
/// (stored in `score_breakdown_json`: "how we calculated").
fn score_breakdown(
    kind: CandidateKind,
    format: &YtDlpFormat,
    outputs: &[OutputKind],
) -> serde_json::Value {
    let base = match kind {
        CandidateKind::Audio => 70,
        CandidateKind::Video => 80,
        CandidateKind::Manifest => 75,
        CandidateKind::Image => 40,
        CandidateKind::Html | CandidateKind::Unknown => 10,
    };
    let height_bonus = format.height.map(|h| (h / 36).clamp(0, 30)).unwrap_or(0);
    let abr_bonus = format
        .abr
        .map(|abr| (abr / 16.0).round() as i64)
        .unwrap_or(0);
    let tbr_bonus = format
        .tbr
        .map(|tbr| (tbr / 250.0).round() as i64)
        .unwrap_or(0);
    let filesize_bonus = if format.filesize.or(format.filesize_approx).is_some() {
        4
    } else {
        0
    };
    let manifest_bonus = if protocol_is_manifest(format.protocol.as_deref()) {
        8
    } else {
        0
    };
    let mp4_compatibility = if outputs.contains(&OutputKind::Video) {
        mp4_compatibility_score(format)
    } else {
        0
    };
    let audio_only = outputs.contains(&OutputKind::Audio) && !outputs.contains(&OutputKind::Video);
    let output_preference = if audio_only {
        match kind {
            CandidateKind::Audio => 40,
            CandidateKind::Video => -30,
            CandidateKind::Manifest => 10,
            _ => 0,
        }
    } else {
        0
    };

    serde_json::json!({
        "engine": "yt_dlp",
        "base_by_kind": base,
        "height_bonus": height_bonus,
        "abr_bonus": abr_bonus,
        "tbr_bonus": tbr_bonus,
        "filesize_bonus": filesize_bonus,
        "manifest_bonus": manifest_bonus,
        "mp4_compatibility": mp4_compatibility,
        "output_preference": output_preference,
        "audio_only_job": audio_only,
        "total": score_format(kind, format, outputs),
    })
}

fn quality_label(format: &YtDlpFormat) -> Option<String> {
    if let Some(note) = &format.format_note {
        if !note.is_empty() {
            return Some(note.clone());
        }
    }
    if let Some(height) = format.height {
        return Some(format!("{height}p"));
    }
    if let Some(abr) = format.abr {
        return Some(format!("{abr:.0}k audio"));
    }
    format.format_id.clone()
}

fn content_type_hint(format: &YtDlpFormat) -> Option<String> {
    match format.ext.as_deref()? {
        "m3u8" => Some("application/vnd.apple.mpegurl".to_string()),
        "mpd" => Some("application/dash+xml".to_string()),
        "mp3" => Some("audio/mpeg".to_string()),
        "m4a" | "aac" => Some("audio/mp4".to_string()),
        "opus" => Some("audio/opus".to_string()),
        "ogg" => Some("audio/ogg".to_string()),
        "mp4" | "m4v" => Some("video/mp4".to_string()),
        "webm" => Some("video/webm".to_string()),
        "jpg" | "jpeg" => Some("image/jpeg".to_string()),
        "png" => Some("image/png".to_string()),
        "webp" => Some("image/webp".to_string()),
        _ => None,
    }
}

fn mp4_compatibility_score(format: &YtDlpFormat) -> i64 {
    let mut score = 0;
    match format.ext.as_deref() {
        Some("mp4" | "m4v" | "m4a" | "aac") => score += 45,
        Some("webm" | "mkv") => score -= 70,
        _ => {}
    }

    let vcodec = format
        .vcodec
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if vcodec.starts_with("avc1") || vcodec.starts_with("h264") {
        score += 45;
    } else if vcodec.starts_with("vp9") || vcodec.starts_with("vp09") || vcodec.starts_with("av01")
    {
        score -= 80;
    }

    let acodec = format
        .acodec
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if acodec.starts_with("mp4a") || acodec.starts_with("aac") {
        score += 20;
    } else if acodec.starts_with("opus") || acodec.starts_with("vorbis") {
        score -= 35;
    }

    score
}

fn protocol_is_manifest(protocol: Option<&str>) -> bool {
    matches!(
        protocol,
        Some("m3u8" | "m3u8_native" | "http_dash_segments" | "dash")
    )
}

fn extension_is_manifest(extension: Option<&str>) -> bool {
    matches!(extension, Some("m3u8" | "mpd"))
}

fn has_sensitive_headers(headers: Option<&serde_json::Value>) -> bool {
    let Some(headers) = headers.and_then(|value| value.as_object()) else {
        return false;
    };
    headers.keys().any(|key| {
        let key = key.to_ascii_lowercase();
        key == "cookie" || key == "authorization" || key.starts_with("x-")
    })
}

fn safe_download_headers(headers: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(headers) = headers.and_then(|value| value.as_object()) else {
        return serde_json::json!({});
    };

    let mut out = serde_json::Map::new();
    for (name, value) in headers {
        let lowered = name.to_ascii_lowercase();
        if !matches!(
            lowered.as_str(),
            "user-agent" | "accept" | "accept-language" | "referer" | "origin" | "range"
        ) {
            continue;
        }
        let Some(value) = value.as_str().filter(|value| !value.is_empty()) else {
            continue;
        };
        out.insert(name.clone(), serde_json::Value::String(value.to_string()));
    }
    serde_json::Value::Object(out)
}

fn candidate_protection(format: &YtDlpFormat) -> CandidateProtection {
    if has_sensitive_headers(format.http_headers.as_ref()) {
        return CandidateProtection::NeedsProfile;
    }
    let protocol = format
        .protocol
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let format_text = format
        .format
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if protocol.contains("drm") || format_text.contains("drm") {
        CandidateProtection::Drm
    } else if format.url.as_deref().map(is_signed_url).unwrap_or(false) {
        CandidateProtection::SignedUrl
    } else {
        CandidateProtection::None
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
    fn parses_formats_into_policy_checked_candidates() {
        let json = br#"
        {
          "id": "abc",
          "title": "sample",
          "extractor": "Generic",
          "webpage_url": "https://example.com/watch",
          "formats": [
            {
              "url": "https://github.com/media/audio.m4a",
              "ext": "m4a",
              "format_id": "140",
              "abr": 128,
              "acodec": "mp4a.40.2",
              "vcodec": "none",
              "filesize": 1234
            },
            {
              "url": "http://127.0.0.1/private.mp4",
              "ext": "mp4",
              "format_id": "bad",
              "acodec": "aac",
              "vcodec": "h264"
            }
          ]
        }
        "#;

        let candidates = parse_yt_dlp_json(Uuid::new_v4(), json, &[OutputKind::Audio]).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, CandidateKind::Audio);
        assert_eq!(candidates[0].extractor, "yt_dlp");
        assert_eq!(candidates[0].url, "https://github.com/media/audio.m4a");
    }

    #[test]
    fn keeps_manifest_for_audio_jobs() {
        let json = br#"
        {
          "formats": [
            {
              "url": "https://github.com/media/master.m3u8",
              "ext": "mp4",
              "protocol": "m3u8_native",
              "format_id": "hls",
              "acodec": "aac",
              "vcodec": "h264"
            }
          ]
        }
        "#;

        let candidates = parse_yt_dlp_json(Uuid::new_v4(), json, &[OutputKind::Audio]).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, CandidateKind::Manifest);
    }

    #[test]
    fn rejects_unknown_candidates_that_do_not_match_outputs() {
        let json = br#"
        {
          "formats": [
            {
              "url": "https://github.com/media/file.bin",
              "ext": "bin",
              "format_id": "unknown"
            }
          ]
        }
        "#;

        let candidates = parse_yt_dlp_json(Uuid::new_v4(), json, &[OutputKind::Audio]).unwrap();

        assert!(candidates.is_empty());
    }

    #[test]
    fn marks_candidates_with_sensitive_headers() {
        let json = br#"
        {
          "formats": [
            {
              "url": "https://github.com/media/video.mp4",
              "ext": "mp4",
              "format_id": "22",
              "acodec": "aac",
              "vcodec": "h264",
              "http_headers": {
                "Cookie": "session=redacted"
              }
            }
          ]
        }
        "#;

        let candidates = parse_yt_dlp_json(Uuid::new_v4(), json, &[OutputKind::Video]).unwrap();

        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].requires_authorization);
    }

    #[test]
    fn persists_only_safe_download_headers() {
        let json = br#"
        {
          "formats": [
            {
              "url": "https://github.com/media/video.mp4",
              "ext": "mp4",
              "format_id": "22",
              "acodec": "aac",
              "vcodec": "h264",
              "http_headers": {
                "User-Agent": "Mozilla/5.0",
                "Referer": "https://example.com/watch",
                "Accept-Language": "en-us,en;q=0.5",
                "Cookie": "session=redacted",
                "Authorization": "Bearer redacted",
                "X-Test": "nope"
              }
            }
          ]
        }
        "#;

        let candidates = parse_yt_dlp_json(Uuid::new_v4(), json, &[OutputKind::Video]).unwrap();
        let headers = candidates[0]
            .metadata_json
            .get("download_headers")
            .and_then(|value| value.as_object())
            .unwrap();

        assert_eq!(
            headers.get("User-Agent").and_then(|value| value.as_str()),
            Some("Mozilla/5.0")
        );
        assert_eq!(
            headers.get("Referer").and_then(|value| value.as_str()),
            Some("https://example.com/watch")
        );
        assert!(!headers.contains_key("Cookie"));
        assert!(!headers.contains_key("Authorization"));
        assert!(!headers.contains_key("X-Test"));
    }

    #[test]
    fn prefers_audio_candidates_for_audio_only_jobs() {
        let json = br#"
        {
          "formats": [
            {
              "url": "https://github.com/media/video.webm",
              "ext": "webm",
              "format_id": "244",
              "height": 480,
              "tbr": 900,
              "acodec": "none",
              "vcodec": "vp9"
            },
            {
              "url": "https://github.com/media/audio.m4a",
              "ext": "m4a",
              "format_id": "140",
              "abr": 128,
              "acodec": "mp4a.40.2",
              "vcodec": "none"
            }
          ]
        }
        "#;

        let candidates = parse_yt_dlp_json(Uuid::new_v4(), json, &[OutputKind::Audio]).unwrap();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].kind, CandidateKind::Audio);
        assert!(candidates[0].score > candidates[1].score);
    }

    #[test]
    fn prefers_mp4_compatible_video_for_video_jobs() {
        let json = br#"
        {
          "formats": [
            {
              "url": "https://github.com/media/video.webm",
              "ext": "webm",
              "format_id": "244",
              "height": 480,
              "tbr": 900,
              "acodec": "none",
              "vcodec": "vp9"
            },
            {
              "url": "https://github.com/media/video.mp4",
              "ext": "mp4",
              "format_id": "135",
              "height": 480,
              "tbr": 700,
              "acodec": "none",
              "vcodec": "avc1.4d401e"
            }
          ]
        }
        "#;

        let candidates = parse_yt_dlp_json(Uuid::new_v4(), json, &[OutputKind::Video]).unwrap();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].content_type.as_deref(), Some("video/mp4"));
        assert!(candidates[0].score > candidates[1].score);
    }
}
