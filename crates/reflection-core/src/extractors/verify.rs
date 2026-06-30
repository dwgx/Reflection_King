//! Candidate verification stage.
//!
//! Routing (the extractor chain) decides *how* to fetch a source; verification
//! decides whether what we found can actually be played. This module probes the
//! highest-scoring candidates with a cheap HTTP HEAD / 2-byte Range GET and moves
//! each from `Untested` to a terminal `CandidateValidationState`.
//!
//! SCOPE: this stage only operates on candidates that an extractor actually
//! produced (`direct`/`generic`/`hanime1`/`mac_cms`/`browser`). External/yt-dlp
//! style jobs emit a final artifact without ever populating `MediaCandidate`
//! rows, so `outcome.candidates` is empty for them and this stage no-ops. Do not
//! add per-extractor-name special casing — the empty-vector path handles it.
//!
//! SSRF: the probe reuses the exact `redirect::none()` + per-hop `validate_url`
//! pattern audited in `hanime.rs`, plus `validate_response_url_and_peer` on the
//! final response. It deliberately uses `.no_proxy()` (the default in
//! `policy_client_builder`): verification measures real server->origin
//! reachability and policy, which is independent of the scraping-side proxy.

use std::time::Duration;

use reqwest::{header, Method, Url};
use time::OffsetDateTime;

use super::ResolveOutcome;
use crate::{
    models::{CandidateKind, CandidateValidationState, MediaCandidate},
    policy_http::{policy_client_builder, validate_response_url_and_peer},
    url_policy::validate_url,
    Result, RkError,
};

const VERIFY_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_REDIRECTS: usize = 5;
const BODY_HEAD_CAP: usize = 16 * 1024;
const SOFT_BLOCK_BYTES: i64 = 4096;
const DEFAULT_TOP_N: usize = 8;
const DEFAULT_TRUNCATE_MAX: usize = 80;
const DEFAULT_SKEW_SECS: i64 = 60;
const VERIFY_UA: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Runtime knobs. Read from env so the stage is a no-op until explicitly enabled.
#[derive(Debug, Clone)]
pub struct VerifyConfig {
    pub enabled: bool,
    pub top_n: usize,
    pub truncate_max: usize,
    pub expiry_skew_secs: i64,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            top_n: DEFAULT_TOP_N,
            truncate_max: DEFAULT_TRUNCATE_MAX,
            expiry_skew_secs: DEFAULT_SKEW_SECS,
        }
    }
}

impl VerifyConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("RK_VERIFY_ENABLED") {
            cfg.enabled = matches!(v.trim(), "1" | "true" | "yes" | "on");
        }
        if let Some(n) = env_usize("RK_VERIFY_TOP_N") {
            cfg.top_n = n.max(1);
        }
        if let Some(n) = env_usize("RK_VERIFY_TRUNCATE_MAX") {
            cfg.truncate_max = n.max(1);
        }
        if let Some(n) = env_i64("RK_VERIFY_SKEW_SECS") {
            cfg.expiry_skew_secs = n.max(0);
        }
        cfg
    }
}

fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.trim().parse().ok()
}

fn env_i64(key: &str) -> Option<i64> {
    std::env::var(key).ok()?.trim().parse().ok()
}

/// Result of a single probe. Pure data so `classify_probe` can be unit-tested
/// without any network.
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    pub status: u16,
    pub content_type: Option<String>,
    pub content_length: Option<i64>,
    pub accept_ranges: bool,
    pub final_url: String,
    pub resolved_ip: Option<String>,
    pub body_head: Option<String>,
    pub policy_status: String,
}

/// A candidate is "unverified" if it has no state yet or is still `Untested`.
/// `direct.rs` seeds `Some(Untested)`, so we must match both.
fn is_unverified(c: &MediaCandidate) -> bool {
    matches!(
        c.validation_state,
        None | Some(CandidateValidationState::Untested)
    )
}

fn is_media_kind(kind: CandidateKind) -> bool {
    matches!(
        kind,
        CandidateKind::Video
            | CandidateKind::Audio
            | CandidateKind::Image
            | CandidateKind::Manifest
    )
}

fn content_type_is_media(ct: &str) -> bool {
    let ct = ct.split(';').next().unwrap_or(ct).trim().to_ascii_lowercase();
    ct.starts_with("video/")
        || ct.starts_with("audio/")
        || ct.starts_with("image/")
        || ct == "application/octet-stream"
}

fn content_type_is_manifest(ct: &str) -> bool {
    let ct = ct.split(';').next().unwrap_or(ct).trim().to_ascii_lowercase();
    ct == "application/vnd.apple.mpegurl"
        || ct == "application/x-mpegurl"
        || ct == "audio/mpegurl"
        || ct == "application/dash+xml"
}

/// True when the URL *path* ends in a manifest extension, ignoring any
/// `?query`/`#fragment`. CDNs routinely serve signed manifests like
/// `master.m3u8?token=...`; a naive `str::ends_with(".m3u8")` on the raw URL
/// misses those, so the body never gets parsed and DRM detection is skipped.
fn url_path_has_manifest_ext(raw: &str) -> bool {
    let path = match Url::parse(raw) {
        Ok(u) => u.path().to_ascii_lowercase(),
        // Not absolute (shouldn't happen for candidate URLs) — fall back to the
        // substring before any query/fragment.
        Err(_) => raw
            .split(['?', '#'])
            .next()
            .unwrap_or(raw)
            .to_ascii_lowercase(),
    };
    path.ends_with(".m3u8") || path.ends_with(".mpd")
}

/// Parse signed-URL expiry from common query needles. Returns epoch seconds.
fn parse_expiry_epoch(url: &Url) -> Option<i64> {
    const EPOCH_KEYS: &[&str] = &["expires", "expire", "oe", "e"];
    const MIN_EPOCH: i64 = 1_000_000_000; // 2001-09
    const MAX_EPOCH: i64 = 100_000_000_000; // year ~5138
    for (k, v) in url.query_pairs() {
        let key = k.to_ascii_lowercase();
        if !EPOCH_KEYS.contains(&key.as_str()) {
            continue;
        }
        let raw = v.trim();
        // Decimal first: a plain epoch like 1700000000 is *also* valid hex, so
        // hex must only win for 0x-prefixed or hex-lettered values.
        if let Ok(n) = raw.parse::<i64>() {
            if (MIN_EPOCH..MAX_EPOCH).contains(&n) {
                return Some(n);
            }
        }
        let hex = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X"));
        let looks_hex = hex.is_some()
            || raw.chars().any(|c| c.is_ascii_hexdigit() && !c.is_ascii_digit());
        if looks_hex {
            if let Ok(n) = i64::from_str_radix(hex.unwrap_or(raw), 16) {
                if (MIN_EPOCH..MAX_EPOCH).contains(&n) {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn body_looks_like_html(body: &str) -> bool {
    let head = body.trim_start();
    let lower = head
        .get(..head.len().min(512))
        .unwrap_or(head)
        .to_ascii_lowercase();
    lower.starts_with("<!doctype html")
        || lower.starts_with("<html")
        || lower.contains("<head")
        || lower.contains("<body")
}

fn body_has_geo_signal(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("not available in your country")
        || lower.contains("not available in your region")
        || lower.contains("geo-restricted")
        || lower.contains("geo restricted")
}

fn body_has_login_signal(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("sign in") || lower.contains("log in") || lower.contains("login required")
}

/// Classify a manifest body (HLS/DASH) into a terminal state.
fn classify_manifest(body: &str) -> CandidateValidationState {
    use CandidateValidationState::*;
    let trimmed = body.trim_start();
    let lower = body.to_ascii_lowercase();
    if trimmed.starts_with("#EXTM3U") {
        // DRM may be advertised at the media-playlist level (#EXT-X-KEY) or, in
        // a master playlist (which we probe first), at the multivariant level
        // (#EXT-X-SESSION-KEY, RFC 8216). The substring "ext-x-key" does NOT
        // occur inside "ext-x-session-key", so both must be checked. A key tag
        // alone (e.g. METHOD=AES-128 clear-key) is not necessarily unplayable;
        // we flag DRM only when a real DRM system is named, either by vendor
        // word or by KEYFORMAT system UUID (Widevine / PlayReady).
        let has_key_tag = lower.contains("#ext-x-key") || lower.contains("#ext-x-session-key");
        let names_drm_system = lower.contains("sample-aes") // FairPlay / SAMPLE-AES(-CTR)
            || lower.contains("com.apple.streamingkeydelivery")
            || lower.contains("widevine")
            || lower.contains("playready")
            || lower.contains("edef8ba9-79d6-4ace-a3c8-27dcd51d21ed") // Widevine UUID
            || lower.contains("9a04f079-9840-4286-ab92-e65be0885f95"); // PlayReady UUID
        if has_key_tag && names_drm_system {
            return Drm;
        }
        if lower.contains("#ext-x-stream-inf")
            || lower.contains("#extinf")
            || lower.contains(".ts")
            || lower.contains(".m4s")
            || lower.contains(".m3u8")
        {
            return Usable;
        }
        // Valid header but no variants/segments: empty playlist.
        return SuspectAd;
    }
    if lower.contains("<mpd") {
        if lower.contains("<contentprotection") {
            return Drm;
        }
        if lower.contains("<representation") {
            return Usable;
        }
        return SuspectAd;
    }
    // Not a parseable manifest (HTML / empty / truncated).
    SuspectAd
}

/// Pure classifier: maps a probe outcome to a terminal state. No network.
/// `kind` is the candidate's declared kind; `expired` is the pre-probe TTL verdict.
pub fn classify_probe(
    outcome: &ProbeOutcome,
    kind: CandidateKind,
    expired: bool,
) -> CandidateValidationState {
    use CandidateValidationState::*;

    if expired {
        return Expired;
    }

    let status = outcome.status;
    let ct = outcome.content_type.clone().unwrap_or_default();
    let body = outcome.body_head.clone().unwrap_or_default();

    match status {
        410 => return Expired,
        451 => return RegionBlocked,
        401 | 403 => {
            if body_has_geo_signal(&body) {
                return RegionBlocked;
            }
            if body_has_login_signal(&body) {
                return NeedsProfile;
            }
            // 403 on a signed URL with no other signal: most often expiry.
            return NeedsProfile;
        }
        s if (400..500).contains(&s) => return Failed,
        s if (500..600).contains(&s) => return Failed,
        _ => {}
    }

    // 2xx / 3xx-resolved path.
    // Manifest needs body parse regardless of declared kind.
    if content_type_is_manifest(&ct) || url_path_has_manifest_ext(&outcome.final_url) {
        if body.is_empty() {
            return SuspectAd;
        }
        return classify_manifest(&body);
    }

    // Soft-block: 200 + HTML (or tiny body) where we expected media.
    if is_media_kind(kind) {
        let html = (!ct.is_empty() && ct.split(';').next().unwrap_or(&ct).trim().eq_ignore_ascii_case("text/html"))
            || (!body.is_empty() && body_looks_like_html(&body));
        let tiny = outcome
            .content_length
            .map(|len| len > 0 && len < SOFT_BLOCK_BYTES)
            .unwrap_or(false);
        if html || tiny {
            return SuspectAd;
        }
    }

    if (200..400).contains(&status) && !ct.is_empty() && content_type_is_media(&ct) {
        return Usable;
    }

    // 2xx but content-type missing or non-media and not obviously a soft block:
    // conservative — possibly playable but unconfirmed.
    SuspectAd
}

/// SSRF-safe probe. `want_body` selects GET+Range (manifest / HEAD fallback)
/// vs HEAD. Mirrors hanime.rs: redirect::none(), manual per-hop `validate_url`,
/// final `validate_response_url_and_peer`.
async fn probe(url: &Url, want_body: bool) -> Result<ProbeOutcome> {
    let client = policy_client_builder(VERIFY_TIMEOUT)
        .user_agent(VERIFY_UA)
        .build()?;
    let mut current = url.clone();
    let mut redirects = 0usize;
    let method = if want_body { Method::GET } else { Method::HEAD };

    let response = loop {
        validate_url(&current)?;
        let mut req = client.request(method.clone(), current.clone());
        if want_body {
            req = req.header(header::RANGE, "bytes=0-16383");
        }
        let resp = req.send().await?;
        if !resp.status().is_redirection() {
            break resp;
        }
        if redirects >= MAX_REDIRECTS {
            return Err(RkError::UrlPolicy(format!(
                "verify exceeded {MAX_REDIRECTS} redirects from {current}"
            )));
        }
        let loc = resp
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                RkError::UrlPolicy(format!("verify redirect from {current} missing Location"))
            })?;
        current = current
            .join(loc)
            .map_err(|e| RkError::UrlPolicy(format!("verify invalid Location `{loc}`: {e}")))?;
        redirects += 1;
    };

    validate_response_url_and_peer(&response)?;
    let status = response.status().as_u16();
    let h = response.headers();
    let content_type = h
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let content_length = h
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());
    let accept_ranges = h
        .get(header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("bytes"))
        .unwrap_or(false);
    let final_url = response.url().to_string();
    let resolved_ip = response.remote_addr().map(|a| a.ip().to_string());

    let body_head = if want_body {
        let bytes = response.bytes().await.unwrap_or_default();
        let cut = bytes.len().min(BODY_HEAD_CAP);
        Some(String::from_utf8_lossy(&bytes[..cut]).into_owned())
    } else {
        None
    };

    Ok(ProbeOutcome {
        status,
        content_type,
        content_length,
        accept_ranges,
        final_url,
        resolved_ip,
        body_head,
        policy_status: "passed".into(),
    })
}

/// Probe a single candidate (with HEAD->Range fallback + manifest body fetch)
/// and write the verdict + observed fields back onto it.
async fn verify_one(candidate: &mut MediaCandidate, skew_secs: i64) {
    let url = match Url::parse(&candidate.url) {
        Ok(u) => u,
        Err(_) => {
            candidate.validation_state = Some(CandidateValidationState::Failed);
            candidate.validation_status = Some("probe_error: invalid_url".into());
            return;
        }
    };

    // Pre-probe TTL check (zero network).
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if let Some(epoch) = parse_expiry_epoch(&url) {
        candidate.expires_at = OffsetDateTime::from_unix_timestamp(epoch).ok();
        if epoch < now {
            candidate.validation_state = Some(CandidateValidationState::Expired);
            candidate.validation_status = Some("expired: signed_url_ttl".into());
            return;
        }
        let _near = epoch < now + skew_secs;
    }

    let is_manifest =
        candidate.kind == CandidateKind::Manifest || url_path_has_manifest_ext(&candidate.url);

    // Manifests need a body; everything else starts with a cheap HEAD.
    let mut outcome = match probe(&url, is_manifest).await {
        Ok(o) => o,
        Err(RkError::UrlPolicy(reason)) => {
            candidate.validation_state = Some(CandidateValidationState::Failed);
            candidate.validation_status = Some(format!("blocked: {reason}"));
            return;
        }
        Err(e) => {
            candidate.validation_state = Some(CandidateValidationState::Failed);
            candidate.validation_status = Some(format!("probe_error: {e}"));
            return;
        }
    };

    // HEAD unsupported / unhelpful -> one Range GET fallback.
    let head_unhelpful = !is_manifest
        && (matches!(outcome.status, 405 | 501)
            || (outcome.content_type.is_none() && (200..300).contains(&outcome.status)));
    if head_unhelpful {
        if let Ok(o) = probe(&url, true).await {
            outcome = o;
        }
    }

    let expired = false; // TTL already handled above.
    let state = classify_probe(&outcome, candidate.kind, expired);

    candidate.validation_state = Some(state);
    candidate.validation_status = Some(outcome.policy_status.clone());
    candidate.status = Some(outcome.status);
    if outcome.content_type.is_some() {
        candidate.content_type = outcome.content_type.clone();
    }
    if outcome.content_length.is_some() {
        candidate.content_length = outcome.content_length;
    }
    candidate.final_url_after_redirects = Some(outcome.final_url.clone());
    if outcome.resolved_ip.is_some() {
        candidate.resolved_ip = outcome.resolved_ip.clone();
    }
    if matches!(
        state,
        CandidateValidationState::Failed
            | CandidateValidationState::Expired
            | CandidateValidationState::Drm
            | CandidateValidationState::RegionBlocked
    ) {
        candidate.failure_reason = Some(state.as_str().to_string());
    }
}

/// Verify the top-N highest-scoring *unverified* candidates in place.
/// Idempotent: candidates with a terminal state are skipped, so calling this
/// from multiple points still probes each candidate at most once and the total
/// probe count is bounded by `cfg.top_n`.
pub async fn verify_top_n(cands: &mut [MediaCandidate], cfg: &VerifyConfig) {
    if !cfg.enabled {
        return;
    }
    let mut idx: Vec<usize> = cands
        .iter()
        .enumerate()
        .filter(|(_, c)| is_unverified(c))
        .map(|(i, _)| i)
        .collect();
    idx.sort_by_key(|&i| -cands[i].score);
    idx.truncate(cfg.top_n);

    // Sequential await over a bounded set; keeps borrow simple and cost is
    // already capped at top_n cheap probes.
    for i in idx {
        verify_one(&mut cands[i], cfg.expiry_skew_secs).await;
    }
}

/// `true` if at least one candidate is confirmed playable. Drives the SAFE
/// short-circuit: we only skip expensive extractors once something is `Usable`,
/// never on an `Untested` candidate.
pub fn has_usable(cands: &[MediaCandidate]) -> bool {
    cands
        .iter()
        .any(|c| c.validation_state == Some(CandidateValidationState::Usable))
}

/// Sort tier for a validation state. Lower is better. `Untested`/`None` rank
/// above all confirmed-bad states so unverified-beyond-top-N candidates are not
/// buried by known-bad ones.
fn state_tier(state: Option<CandidateValidationState>) -> u8 {
    use CandidateValidationState::*;
    match state {
        Some(Usable) => 0,
        Some(NeedsProfile) => 1,
        Some(SuspectAd) => 2,
        None | Some(Untested) => 3,
        Some(RegionBlocked) => 4,
        Some(Drm) => 5,
        Some(Expired) => 6,
        Some(Failed) => 7,
    }
}

/// Final ordering: state tier ascending, then score descending.
pub fn sort_verified(cands: &mut [MediaCandidate]) {
    cands.sort_by(|a, b| {
        state_tier(a.validation_state)
            .cmp(&state_tier(b.validation_state))
            .then_with(|| b.score.cmp(&a.score))
    });
}

/// `true` if running this stage would change ordering or verdicts — used to skip
/// work when verification produced nothing (all probes failed to run).
pub fn outcome_has_candidates(outcome: &ResolveOutcome) -> bool {
    !outcome.candidates.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CandidateProtection;
    use uuid::Uuid;

    fn mk_candidate(state: Option<CandidateValidationState>, score: i64) -> MediaCandidate {
        MediaCandidate {
            id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            url: "https://cdn.example/v.mp4".to_string(),
            kind: CandidateKind::Video,
            extractor: "mock".to_string(),
            method: "test".to_string(),
            status: None,
            content_type: None,
            content_length: None,
            resource_type: None,
            initiator_url: None,
            quality_label: None,
            score,
            requires_authorization: false,
            platform: None,
            route: Some("mock".to_string()),
            extractor_confidence: Some(50),
            protection: Some(CandidateProtection::None),
            requires_profile: false,
            ttl_hint_seconds: None,
            ad_risk: false,
            evidence_count: 1,
            paired_candidate_ids: Vec::new(),
            failure_reason: None,
            validation_state: state,
            metadata_json: serde_json::Value::Null,
            created_at: OffsetDateTime::now_utc(),
            score_breakdown_json: serde_json::Value::Null,
            selected: false,
            selection_reason: None,
            validation_status: None,
            resolved_ip: None,
            final_url_after_redirects: None,
            expires_at: None,
            discovered_by_event_id: None,
        }
    }

    fn outcome(status: u16, ct: Option<&str>, len: Option<i64>, body: Option<&str>) -> ProbeOutcome {
        ProbeOutcome {
            status,
            content_type: ct.map(str::to_string),
            content_length: len,
            accept_ranges: true,
            final_url: "https://cdn.example/m.mp4".into(),
            resolved_ip: Some("203.0.113.1".into()),
            body_head: body.map(str::to_string),
            policy_status: "passed".into(),
        }
    }

    /// Like `outcome` but with an explicit `final_url` (to exercise the
    /// manifest-by-URL path, including signed/tokenized query suffixes).
    fn outcome_url(
        final_url: &str,
        status: u16,
        ct: Option<&str>,
        body: Option<&str>,
    ) -> ProbeOutcome {
        let mut o = outcome(status, ct, None, body);
        o.final_url = final_url.into();
        o
    }

    #[test]
    fn signed_manifest_url_with_query_still_parses_body_as_drm() {
        // Regression: a signed HLS URL ends with `?token=...`, not `.m3u8`, and
        // the CDN mislabels the content-type as octet-stream. The manifest must
        // still be body-parsed so DRM is detected (not waved through as Usable).
        let body = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://x\"\n#EXTINF:6,\nseg.ts\n";
        let o = outcome_url(
            "https://cdn.example/hls/master.m3u8?token=abc123&expires=later",
            200,
            Some("application/octet-stream"),
            Some(body),
        );
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::Drm
        );
    }

    #[test]
    fn signed_dash_url_with_query_parses_body() {
        let body = "<MPD><Period><Representation id=\"1\"/></Period></MPD>";
        let o = outcome_url(
            "https://cdn.example/dash/manifest.mpd?sig=deadbeef",
            200,
            Some("application/octet-stream"),
            Some(body),
        );
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::Usable
        );
    }

    #[test]
    fn url_path_manifest_ext_ignores_query_and_fragment() {
        assert!(url_path_has_manifest_ext(
            "https://cdn.example/master.m3u8?token=abc"
        ));
        assert!(url_path_has_manifest_ext(
            "https://cdn.example/dash/manifest.mpd#t=10"
        ));
        assert!(url_path_has_manifest_ext("https://cdn.example/v.M3U8"));
        assert!(!url_path_has_manifest_ext(
            "https://cdn.example/watch?file=movie.m3u8"
        ));
        assert!(!url_path_has_manifest_ext("https://cdn.example/clip.mp4"));
    }

    #[test]
    fn media_206_is_usable() {
        let o = outcome(206, Some("video/mp4"), Some(7_000_000), None);
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::Usable
        );
    }

    #[test]
    fn tiny_html_is_suspect_ad() {
        let o = outcome(200, Some("text/html"), Some(300), Some("<html><body>ad</body></html>"));
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::SuspectAd
        );
    }

    #[test]
    fn gone_is_expired() {
        let o = outcome(410, None, None, None);
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::Expired
        );
    }

    #[test]
    fn unavailable_for_legal_is_region_blocked() {
        let o = outcome(451, None, None, None);
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::RegionBlocked
        );
    }

    #[test]
    fn forbidden_with_geo_is_region_blocked() {
        let o = outcome(403, Some("text/html"), None, Some("This video is not available in your country."));
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::RegionBlocked
        );
    }

    #[test]
    fn forbidden_with_login_is_needs_profile() {
        let o = outcome(403, Some("text/html"), None, Some("Please sign in to continue"));
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::NeedsProfile
        );
    }

    #[test]
    fn pre_probe_expiry_short_circuits() {
        let o = outcome(200, Some("video/mp4"), Some(7_000_000), None);
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, true),
            CandidateValidationState::Expired
        );
    }

    #[test]
    fn hls_master_is_usable() {
        let body = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000\nlow.m3u8\n";
        let o = outcome(200, Some("application/vnd.apple.mpegurl"), None, Some(body));
        assert_eq!(
            classify_probe(&o, CandidateKind::Manifest, false),
            CandidateValidationState::Usable
        );
    }

    #[test]
    fn hls_with_sample_aes_is_drm() {
        let body = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://x\"\n#EXTINF:6,\nseg.ts\n";
        let o = outcome(200, Some("application/vnd.apple.mpegurl"), None, Some(body));
        assert_eq!(
            classify_probe(&o, CandidateKind::Manifest, false),
            CandidateValidationState::Drm
        );
    }

    #[test]
    fn dash_with_content_protection_is_drm() {
        let body = "<MPD><Period><ContentProtection schemeIdUri=\"urn:mpeg:dash:mp4protection:2011\"/></Period></MPD>";
        let o = outcome(200, Some("application/dash+xml"), None, Some(body));
        assert_eq!(
            classify_probe(&o, CandidateKind::Manifest, false),
            CandidateValidationState::Drm
        );
    }

    #[test]
    fn hls_master_session_key_fairplay_is_drm() {
        // Master playlist advertises DRM via #EXT-X-SESSION-KEY (RFC 8216), not
        // #EXT-X-KEY. "ext-x-key" is NOT a substring of "ext-x-session-key", so
        // the old check let this through as Usable (it has #EXT-X-STREAM-INF).
        let body = "#EXTM3U\n#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,KEYFORMAT=\"com.apple.streamingkeydelivery\",URI=\"skd://x\"\n#EXT-X-STREAM-INF:BANDWIDTH=800000\nlow.m3u8\n";
        let o = outcome(200, Some("application/vnd.apple.mpegurl"), None, Some(body));
        assert_eq!(
            classify_probe(&o, CandidateKind::Manifest, false),
            CandidateValidationState::Drm
        );
    }

    #[test]
    fn hls_widevine_keyformat_uuid_is_drm() {
        // Real Widevine HLS (shaka angel-one): SAMPLE-AES-CTR + KEYFORMAT system
        // UUID, no literal "widevine" word. Detected by the UUID.
        let body = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES-CTR,URI=\"data:text/plain;base64,AA==\",KEYFORMAT=\"urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed\"\n#EXTINF:4,\nseg.m4s\n";
        let o = outcome(200, Some("application/x-mpegurl"), None, Some(body));
        assert_eq!(
            classify_probe(&o, CandidateKind::Manifest, false),
            CandidateValidationState::Drm
        );
    }

    #[test]
    fn hls_master_clear_no_key_is_usable() {
        // Clean master (Apple bipbop): #EXT-X-STREAM-INF, no key tag -> Usable.
        let body = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=2177116,RESOLUTION=960x540\nv0.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=8001098,RESOLUTION=1920x1080\nv1.m3u8\n";
        let o = outcome(200, Some("application/vnd.apple.mpegurl"), None, Some(body));
        assert_eq!(
            classify_probe(&o, CandidateKind::Manifest, false),
            CandidateValidationState::Usable
        );
    }

    #[test]
    fn server_error_is_failed() {
        let o = outcome(503, None, None, None);
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::Failed
        );
    }

    #[test]
    fn parse_expiry_decimal() {
        let u = Url::parse("https://cdn.example/v.mp4?expires=2000000000&sig=abc").unwrap();
        assert_eq!(parse_expiry_epoch(&u), Some(2_000_000_000));
    }

    #[test]
    fn parse_expiry_plain_epoch_not_misread_as_hex() {
        // Regression: 1700000000 is valid hex too; decimal must win so it is not
        // read as a far-future timestamp (which would skip the Expired verdict).
        let u = Url::parse("https://cdn.example/v.mp4?expires=1700000000").unwrap();
        assert_eq!(parse_expiry_epoch(&u), Some(1_700_000_000));
    }

    #[test]
    fn parse_expiry_hex_with_letters() {
        // hex epoch 0x6553f100 = 1699997440
        let u = Url::parse("https://cdn.example/v.mp4?oe=6553F100").unwrap();
        assert_eq!(parse_expiry_epoch(&u), Some(0x6553_F100));
    }

    #[test]
    fn sort_orders_usable_above_untested_above_failed() {
        let mk = |state: Option<CandidateValidationState>, score: i64| mk_candidate(state, score);
        let mut v = vec![
            mk(Some(CandidateValidationState::Failed), 100),
            mk(Some(CandidateValidationState::Untested), 90),
            mk(Some(CandidateValidationState::Usable), 10),
        ];
        sort_verified(&mut v);
        assert_eq!(v[0].validation_state, Some(CandidateValidationState::Usable));
        assert_eq!(v[1].validation_state, Some(CandidateValidationState::Untested));
        assert_eq!(v[2].validation_state, Some(CandidateValidationState::Failed));
    }

    #[tokio::test]
    async fn disabled_config_is_noop() {
        let mut v = vec![mk_candidate(Some(CandidateValidationState::Untested), 50)];
        let cfg = VerifyConfig::default(); // enabled = false
        verify_top_n(&mut v, &cfg).await;
        assert_eq!(v[0].validation_state, Some(CandidateValidationState::Untested));
    }

    #[test]
    fn has_usable_detects_usable() {
        let c = mk_candidate(Some(CandidateValidationState::Usable), 10);
        assert!(has_usable(&[c]));
    }

    // --- Live integration tests (network). Run explicitly:
    //   cargo test -p reflection-core --release verify_live -- --ignored --nocapture
    // Probes deliberately bypass the proxy (.no_proxy()); home-cloud reaches
    // these CDNs directly. Anchors per VERIFICATION-DESIGN.md §8.2.

    fn live_candidate(url: &str, kind: CandidateKind) -> MediaCandidate {
        let mut c = mk_candidate(Some(CandidateValidationState::Untested), 100);
        c.url = url.to_string();
        c.kind = kind;
        c
    }

    fn enabled_cfg() -> VerifyConfig {
        VerifyConfig {
            enabled: true,
            top_n: 8,
            truncate_max: 80,
            expiry_skew_secs: 60,
        }
    }

    #[tokio::test]
    #[ignore]
    async fn verify_live_direct_mp4_is_usable() {
        let mut cands = vec![
            live_candidate(
                "https://download.blender.org/durian/trailer/sintel_trailer-480p.mp4",
                CandidateKind::Video,
            ),
            live_candidate(
                "https://www.w3schools.com/html/mov_bbb.mp4",
                CandidateKind::Video,
            ),
            live_candidate(
                "https://archive.org/download/BigBuckBunny_328/BigBuckBunny_512kb.mp4",
                CandidateKind::Video,
            ),
        ];
        verify_top_n(&mut cands, &enabled_cfg()).await;
        for c in &cands {
            println!(
                "DIRECT {} -> {:?} status={:?} ct={:?} final={:?}",
                c.url, c.validation_state, c.status, c.content_type, c.final_url_after_redirects
            );
            assert_eq!(
                c.validation_state,
                Some(CandidateValidationState::Usable),
                "expected Usable for {}",
                c.url
            );
        }
        assert!(has_usable(&cands));
    }

    #[tokio::test]
    #[ignore]
    async fn verify_live_manifests_are_usable() {
        let mut cands = vec![
            live_candidate(
                "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8",
                CandidateKind::Manifest,
            ),
            live_candidate(
                "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd",
                CandidateKind::Manifest,
            ),
        ];
        verify_top_n(&mut cands, &enabled_cfg()).await;
        for c in &cands {
            println!(
                "MANIFEST {} -> {:?} status={:?} ct={:?}",
                c.url, c.validation_state, c.status, c.content_type
            );
            assert_eq!(
                c.validation_state,
                Some(CandidateValidationState::Usable),
                "expected Usable for {}",
                c.url
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn verify_live_expired_signed_url_zero_network() {
        // expires far in the past -> Expired before any network call.
        let mut cands = vec![live_candidate(
            "https://cdn.example/secret.mp4?expires=1000000001&signature=deadbeef",
            CandidateKind::Video,
        )];
        verify_top_n(&mut cands, &enabled_cfg()).await;
        println!(
            "EXPIRED {} -> {:?} status={:?}",
            cands[0].url, cands[0].validation_state, cands[0].status
        );
        assert_eq!(
            cands[0].validation_state,
            Some(CandidateValidationState::Expired)
        );
        // never probed -> no HTTP status recorded.
        assert!(cands[0].status.is_none());
    }

    #[tokio::test]
    #[ignore]
    async fn verify_live_sort_promotes_usable_over_untested() {
        // High-score reachable mp4 + a high-score bogus host. After verify, the
        // Usable one must sort first even though both started Untested.
        let mut cands = vec![
            live_candidate(
                "https://nonexistent-host-rk-verify-test.invalid/x.mp4",
                CandidateKind::Video,
            ),
            live_candidate(
                "https://download.blender.org/durian/trailer/sintel_trailer-480p.mp4",
                CandidateKind::Video,
            ),
        ];
        cands[0].score = 999; // bogus ranks highest by raw score
        cands[1].score = 1;
        verify_top_n(&mut cands, &enabled_cfg()).await;
        sort_verified(&mut cands);
        println!(
            "SORT[0] {} -> {:?} score={}",
            cands[0].url, cands[0].validation_state, cands[0].score
        );
        // The reachable mp4 (low raw score) must now lead.
        assert_eq!(
            cands[0].validation_state,
            Some(CandidateValidationState::Usable)
        );
        assert!(cands[0].url.contains("blender.org"));
    }
}
