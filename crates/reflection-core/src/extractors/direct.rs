//! Direct file / manifest extractor.
//!
//! When the input URL itself points at a media file or a streaming manifest,
//! there is nothing to discover: the URL is the candidate. This is the safest,
//! fastest tier and runs first in the auto chain.

use async_trait::async_trait;
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    models::{CandidateKind, MediaCandidate},
    Result,
};

use super::{ExtractContext, ExtractResult, SourceExtractor};

const MANIFEST_EXTS: &[&str] = &["m3u8", "mpd"];
const AUDIO_EXTS: &[&str] = &["mp3", "m4a", "aac", "wav", "flac", "opus", "ogg"];
const VIDEO_EXTS: &[&str] = &["mp4", "m4v", "webm", "flv", "mov", "mkv", "m4s"];
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "avif"];

pub struct DirectExtractor;

impl DirectExtractor {
    fn extension(ctx: &ExtractContext) -> Option<String> {
        let path = ctx.url.path().to_ascii_lowercase();
        path.rsplit('/')
            .next()
            .and_then(|segment| segment.rsplit_once('.'))
            .map(|(_, ext)| ext.to_string())
    }
}

#[async_trait]
impl SourceExtractor for DirectExtractor {
    fn name(&self) -> &'static str {
        "direct"
    }

    fn matches(&self, ctx: &ExtractContext) -> bool {
        Self::extension(ctx)
            .map(|ext| classify_extension(&ext).is_some())
            .unwrap_or(false)
    }

    async fn extract(&self, ctx: &ExtractContext) -> Result<ExtractResult> {
        let Some(ext) = Self::extension(ctx) else {
            return Ok(ExtractResult::default());
        };
        let Some(kind) = classify_extension(&ext) else {
            return Ok(ExtractResult::default());
        };

        let candidate = MediaCandidate {
            id: Uuid::new_v4(),
            job_id: ctx.job_id,
            url: ctx.source_url.clone(),
            kind,
            extractor: "direct".to_string(),
            method: "url".to_string(),
            status: None,
            content_type: content_type_for(&ext),
            content_length: None,
            resource_type: Some("direct_url".to_string()),
            initiator_url: Some(ctx.source_url.clone()),
            quality_label: None,
            score: base_score(kind),
            requires_authorization: false,
            metadata_json: json!({ "source": "direct", "ext": ext }),
            created_at: OffsetDateTime::now_utc(),
            score_breakdown_json: json!({
                "engine": "direct",
                "base_by_kind": base_score(kind),
                "total": base_score(kind),
            }),
            selected: false,
            selection_reason: None,
            validation_status: None,
            resolved_ip: None,
            final_url_after_redirects: None,
            expires_at: None,
            discovered_by_event_id: None,
        };

        Ok(ExtractResult::candidates(vec![candidate]))
    }
}

fn classify_extension(ext: &str) -> Option<CandidateKind> {
    if MANIFEST_EXTS.contains(&ext) {
        Some(CandidateKind::Manifest)
    } else if AUDIO_EXTS.contains(&ext) {
        Some(CandidateKind::Audio)
    } else if VIDEO_EXTS.contains(&ext) {
        Some(CandidateKind::Video)
    } else if IMAGE_EXTS.contains(&ext) {
        Some(CandidateKind::Image)
    } else {
        None
    }
}

fn base_score(kind: CandidateKind) -> i64 {
    match kind {
        CandidateKind::Manifest => 90,
        CandidateKind::Video => 88,
        CandidateKind::Audio => 85,
        CandidateKind::Image => 50,
        _ => 20,
    }
}

fn content_type_for(ext: &str) -> Option<String> {
    let value = match ext {
        "m3u8" => "application/vnd.apple.mpegurl",
        "mpd" => "application/dash+xml",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "opus" => "audio/opus",
        "ogg" => "audio/ogg",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => return None,
    };
    Some(value.to_string())
}
