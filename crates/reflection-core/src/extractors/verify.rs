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
/// Max extra GET attempts after an initial transient (5xx/429) probe. archive's
/// sharded storage can fail over across several nodes (observed 500,500,206), so
/// one retry is not always enough; capped so a down origin still terminates.
const TRANSIENT_RETRIES: usize = 2;
/// Per-attempt cool-off for rate-limit statuses (429/503) when the origin sends
/// no `Retry-After`. Seconds-scale because throttle windows are seconds, not
/// milliseconds (observed: upload.wikimedia.org clears a 429 in ~2-3s); the
/// short 300ms linear backoff used for flaky 5xx nodes never clears it.
const RATE_LIMIT_BACKOFF_MS: u64 = 2500;
/// Upper bound on an honored `Retry-After`, so a hostile/confused origin asking
/// us to wait minutes can't stall a verify pass; past this we fall through to a
/// non-terminal classification instead.
const RETRY_AFTER_CAP_SECS: u64 = 8;
const BODY_HEAD_CAP: usize = 16 * 1024;
/// Manifests (HLS/DASH) must be scanned in full for DRM markers (`#EXT-X-KEY`,
/// `#EXT-X-SESSION-KEY`, DASH `<ContentProtection>`), which can sit past the
/// 16 KiB media-probe cap — a real multivariant master or multi-DRM MPD exceeds
/// it (e.g. axprod Manifest_1080p.mpd is 18.5 KiB). Truncating there risks
/// waving a protected stream through as Usable, so give manifest bodies a much
/// larger (still bounded) budget.
const MANIFEST_BODY_CAP: usize = 512 * 1024;
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
    /// `Retry-After` (delta-seconds form) when the origin sent one, e.g. on 429
    /// / 503. None when absent or in HTTP-date form (rare for rate limits).
    pub retry_after_secs: Option<u64>,
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
        // `binary/octet-stream` is a legacy-but-live alias of application/octet-stream.
        // PeerTube and its object-storage backends (observed:
        // makertube01.fsn1.your-objectstorage.com, tuba.hyperreal.top) serve real
        // fragmented MP4s (ftyp/iso5 magic, 200/206) with this exact content-type;
        // without it those genuine playable files fell through to SuspectAd across
        // many federated instances.
        || ct == "binary/octet-stream"
        // Ogg container (Theora video / Vorbis|Opus audio) is commonly served as
        // application/ogg, not video/* — e.g. archive.org's .ogv/.ogg derivatives
        // (VoyagetothePlanetofPrehistoricWomen.ogg -> 200 application/ogg, OggS
        // magic). Without this an OggS-magic media file fell through to SuspectAd.
        // x-ogg is the legacy alias some older servers still emit.
        || ct == "application/ogg"
        || ct == "application/x-ogg"
}

fn content_type_is_manifest(ct: &str) -> bool {
    let ct = ct.split(';').next().unwrap_or(ct).trim().to_ascii_lowercase();
    ct == "application/vnd.apple.mpegurl"
        || ct == "application/x-mpegurl"
        || ct == "audio/mpegurl"
        || ct == "application/dash+xml"
}

/// Transient HTTP statuses that warrant a bounded retry before a terminal
/// verdict: 429 (rate-limited) and the retryable 5xx (500/502/503/504). Sharded
/// CDN / storage backends intermittently emit these for a request that succeeds
/// on a subsequent attempt (see TRANSIENT_RETRIES).
fn is_transient_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

/// In an HLS master (multivariant) playlist, return the first variant playlist
/// URL (the non-comment line following an `#EXT-X-STREAM-INF`), resolved
/// against `base`. DRM in HLS-Widevine/-PlayReady is frequently declared only
/// in the *media* playlist via `#EXT-X-KEY`, not in the master we probe first;
/// descending one level lets us catch that. Returns None when the body is not
/// a master playlist (no `#EXT-X-STREAM-INF`) or no variant URI follows.
fn first_hls_variant_url(base: &Url, body: &str) -> Option<Url> {
    let mut lines = body.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim_start().to_ascii_uppercase().starts_with("#EXT-X-STREAM-INF") {
            // The URI is the next non-blank, non-comment line.
            for next in lines.by_ref() {
                let t = next.trim();
                if t.is_empty() || t.starts_with('#') {
                    continue;
                }
                return base.join(t).ok();
            }
        }
    }
    None
}

/// Given a probed outcome, return the first variant URL to descend into iff the
/// body is an HLS *master* playlist (has `#EXT-X-STREAM-INF`) — i.e. worth a
/// one-level DRM re-check. None for media playlists, DASH, or non-manifests.
fn hls_master_variant_to_probe(outcome: &ProbeOutcome) -> Option<Url> {
    let body = outcome.body_head.as_deref()?;
    if !body.trim_start().starts_with("#EXTM3U") {
        return None;
    }
    let base = Url::parse(&outcome.final_url).ok()?;
    first_hls_variant_url(&base, body)
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

/// A hard access denial from object storage / a CDN ACL (S3 `AccessDenied`,
/// GCS `<Error>` bodies, generic "access denied"/"forbidden" XML). Distinct
/// from a login wall (a profile fixes that) or a geo block: these URLs are not
/// retrievable as-is — typically a private object or one needing a signature
/// the page itself generates server-side. Observed on TED `py.tedcdn.com`
/// fallback MP4s, which 403 with an S3 `<Error><Code>AccessDenied</Code>` body
/// regardless of Referer/Origin. `Failed` is more honest than `NeedsProfile`.
fn body_has_access_denied_signal(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("<code>accessdenied</code>")
        || lower.contains("<code>signaturedoesnotmatch</code>")
        || lower.contains("<code>invalidaccesskeyid</code>")
        || lower.contains("accessdenied")
}

/// A soft/transient denial from an anti-bot or rate-limit gate that returns 401
/// or 403 instead of 429. The resource exists and is publicly reachable (no
/// account fixes it) — it is throttling this caller right now. Observed on
/// Odysee `player.odycdn.com/api/v3/streams/free/...` (public, no-login content)
/// which 401s with "this content cannot be accessed at the moment" and flips to
/// 429 "Try again later" under a Referer. Classifying these NeedsProfile is
/// wrong (there is no profile to supply); SuspectAd (recoverable, retry later)
/// matches how the sibling 429 path is already treated.
fn body_has_transient_throttle_signal(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("cannot be accessed at the moment")
        || lower.contains("try again later")
        || lower.contains("too many requests")
        || lower.contains("rate limit")
        || lower.contains("temporarily unavailable")
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
            // Object-storage / CDN ACL denial (S3 AccessDenied, signature
            // mismatch): not retrievable as-is and no profile fixes it. Checked
            // before the login signal since some error pages carry both words.
            if body_has_access_denied_signal(&body) {
                return Failed;
            }
            // Anti-bot / rate-limit gate that answers 401/403 instead of 429.
            // The resource is public (no profile fixes it); it is throttling us
            // now. Treat like a persistent 429 -> SuspectAd, not NeedsProfile.
            if body_has_transient_throttle_signal(&body) {
                return SuspectAd;
            }
            if body_has_login_signal(&body) {
                return NeedsProfile;
            }
            // 403 with no other signal: most often a signed-URL/auth gate a
            // profile may satisfy.
            return NeedsProfile;
        }
        // A 429 that survived the rate-limit retries above is a persistent
        // throttle, not a dead resource (Too Many Requests = "exists, come back
        // later"). Marking it terminal Failed mis-buries a recoverable stream
        // (observed: upload.wikimedia.org transcoded variants under burst);
        // SuspectAd keeps it as an unconfirmed-but-not-dead candidate.
        429 => return SuspectAd,
        s if (400..500).contains(&s) => return Failed,
        s if (500..600).contains(&s) => return Failed,
        _ => {}
    }
    // 2xx / 3xx-resolved path.
    // Manifest needs body parse regardless of declared content-type. Gate on the
    // candidate *kind* too, not just CT / path extension: a Manifest-kind
    // candidate can carry the `.m3u8` in its query string (mac_cms / hanime match
    // `url.contains(".m3u8")`, so the marker may sit after `?`) or be served as
    // application/octet-stream (observed: shaka DASH assets). Either way
    // url_path_has_manifest_ext (path-only) and content_type_is_manifest both miss
    // it, so without the kind check it would skip classify_manifest and fall
    // through to the media arm as Usable — the DRM parse never runs (same bypass
    // family as the signed-manifest and 16 KiB-cap fixes). The body is present
    // because verify_one fetches it whenever the candidate kind is Manifest.
    if kind == CandidateKind::Manifest
        || content_type_is_manifest(&ct)
        || url_path_has_manifest_ext(&outcome.final_url)
    {
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
        // Kind/content-type mismatch guard: an `image/*` response only confirms an
        // Image candidate. A Video/Audio candidate that resolves to an image is a
        // poster/thumbnail that leaked past extraction (e.g. a JSON-LD VideoObject
        // whose extensionless `contentUrl` actually points at the still frame) —
        // waving it through as Usable would defeat verify's whole purpose. Demote
        // to SuspectAd: unconfirmed, not a confirmed playable stream. octet-stream
        // and the audio/video/ogg families stay Usable for any media kind.
        let ct_base = ct.split(';').next().unwrap_or(&ct).trim();
        if ct_base.starts_with("image/") && !matches!(kind, CandidateKind::Image) {
            return SuspectAd;
        }
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
    probe_capped(url, want_body, BODY_HEAD_CAP).await
}

/// Like `probe`, but with an explicit body-capture cap. Manifests pass
/// MANIFEST_BODY_CAP so a DRM marker past 16 KiB is not truncated away; media
/// and error-body probes use the smaller default.
async fn probe_capped(url: &Url, want_body: bool, body_cap: usize) -> Result<ProbeOutcome> {
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
            // Range upper bound follows the cap (inclusive byte index). Origins
            // that ignore Range and stream the whole body are bounded by the
            // capped chunk-read below (not by buffering the full body first).
            req = req.header(header::RANGE, format!("bytes=0-{}", body_cap - 1));
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
    let retry_after_secs = h
        .get(header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok());

    let body_head = if want_body {
        // Stream chunks and stop once we have `body_cap` bytes, rather than
        // `response.bytes().await` which buffers the WHOLE body first and only
        // then truncates. The Range header above is best-effort: some origins
        // ignore it and reply 200 with the full file (observed live:
        // oplay.radiomercure.fr HLS -fragmented.mp4 -> 200, Range ignored,
        // tens-of-MB streamed). With the old buffer-then-cut, those large bodies
        // ran past VERIFY_TIMEOUT and surfaced as a no-status Err -> false Failed
        // on a genuinely-playable file. Capping the read bounds both time and
        // memory regardless of whether the server honors Range.
        let mut collected: Vec<u8> = Vec::new();
        let mut resp = response;
        while collected.len() < body_cap {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    let remaining = body_cap - collected.len();
                    let take = chunk.len().min(remaining);
                    collected.extend_from_slice(&chunk[..take]);
                    if take < chunk.len() {
                        break; // hit the cap mid-chunk
                    }
                }
                Ok(None) => break, // body fully read under the cap
                Err(_) => break,   // partial body is still classifiable
            }
        }
        Some(String::from_utf8_lossy(&collected).into_owned())
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
        retry_after_secs,
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

    // Manifests need a body (with the larger manifest cap so a late DRM marker
    // isn't truncated); everything else starts with a cheap HEAD.
    let initial_cap = if is_manifest { MANIFEST_BODY_CAP } else { BODY_HEAD_CAP };
    let mut outcome = match probe_capped(&url, is_manifest, initial_cap).await {
        Ok(o) => o,
        Err(RkError::UrlPolicy(reason)) => {
            candidate.validation_state = Some(CandidateValidationState::Failed);
            candidate.validation_status = Some(format!("blocked: {reason}"));
            return;
        }
        Err(e) => {
            // A non-policy error on the initial probe. For a media candidate that
            // was a HEAD: many CDNs/object stores simply don't implement HEAD and
            // hang until the client timeout (observed live: PeerTube
            // /download/.../*-fragmented.mp4 on p2b.drjpdns.com, dalek.zone,
            // oplay.radiomercure.fr -> HEAD stalls, 0 bytes; the same URL serves a
            // 200 on GET). Without a GET fallback the timeout surfaced as a
            // no-status probe_error -> false Failed on a genuinely-playable file.
            // The capped chunk-read keeps the GET cheap even when the origin
            // ignores Range and streams the whole file.
            if !is_manifest {
                match probe(&url, true).await {
                    Ok(o) => o,
                    Err(_) => {
                        candidate.validation_state = Some(CandidateValidationState::Failed);
                        candidate.validation_status = Some(format!("probe_error: {e}"));
                        return;
                    }
                }
            } else {
                candidate.validation_state = Some(CandidateValidationState::Failed);
                candidate.validation_status = Some(format!("probe_error: {e}"));
                return;
            }
        }
    };

    // HEAD unsupported / unhelpful -> one Range GET fallback. A 401/403 from a
    // HEAD carries no body, so the classifier can't tell an S3 AccessDenied
    // (Failed) from a geo block (RegionBlocked), a login wall (NeedsProfile), or
    // a bare signed-URL gate. Fetch the error body once so those branches have
    // the signal they need (observed: TED py.tedcdn.com fallback MP4s 403 with
    // an `<Error><Code>AccessDenied</Code>` body).
    let head_unhelpful = !is_manifest
        && (matches!(outcome.status, 405 | 501 | 401 | 403)
            || (outcome.content_type.is_none() && (200..300).contains(&outcome.status)));
    if head_unhelpful && outcome.body_head.is_none() {
        if let Ok(o) = probe(&url, true).await {
            outcome = o;
        }
    }

    // Bounded retries on a transient status before we commit a terminal Failed.
    // CDNs / sharded storage (e.g. archive.org's per-item dnNNNN nodes that a
    // download/ URL 302-redirects to) intermittently 5xx or 429 a request that
    // succeeds moments later; without a retry a genuinely-Usable stream gets
    // permanently marked Failed (observed: ElephantsDream ed_1024.mp4 -> 500
    // then 200; charlie_chaplin_film_fest -> 500, 500, then 206 — a single retry
    // is not enough when archive fails over across multiple nodes). Retry with a
    // body GET so a recovered node also yields ct/len. Short linear backoff;
    // capped at TRANSIENT_RETRIES so a truly-down origin still terminates fast.
    let mut attempts = 0;
    while is_transient_status(outcome.status) && attempts < TRANSIENT_RETRIES {
        attempts += 1;
        // 429 (and 503) are rate-limit / cool-off statuses: a 300ms nudge does
        // not clear the window (observed: upload.wikimedia.org throttles after
        // ~2 rapid probes, sends no Retry-After, and recovers in ~2-3s). Honor
        // Retry-After when present, else back off seconds-scale for a throttle
        // vs the short linear backoff that suffices for a flaky 5xx node.
        let delay_ms = if matches!(outcome.status, 429 | 503) {
            outcome
                .retry_after_secs
                .map(|s| s.min(RETRY_AFTER_CAP_SECS) * 1000)
                .unwrap_or(RATE_LIMIT_BACKOFF_MS * attempts as u64)
        } else {
            300 * attempts as u64
        };
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        match probe_capped(&url, true, initial_cap).await {
            Ok(o) => outcome = o,
            Err(_) => break,
        }
    }

    let expired = false; // TTL already handled above.
    let state = classify_probe(&outcome, candidate.kind, expired);

    // HLS master-playlist DRM descent: a master can classify Usable while its
    // variant playlists carry the #EXT-X-KEY (real Widevine/PlayReady HLS, e.g.
    // shaka angel-one). When we got a Usable HLS master, fetch the first
    // variant once and re-check for DRM so a protected stream isn't waved
    // through. One level only (no segment fetch); failures leave state as-is.
    let state = if state == CandidateValidationState::Usable {
        if let Some(variant) = hls_master_variant_to_probe(&outcome) {
            match probe_capped(&variant, true, MANIFEST_BODY_CAP).await {
                Ok(vo) => {
                    let vstate = classify_probe(&vo, CandidateKind::Manifest, false);
                    if vstate == CandidateValidationState::Drm {
                        candidate.validation_status =
                            Some("drm: variant #EXT-X-KEY".to_string());
                        CandidateValidationState::Drm
                    } else {
                        state
                    }
                }
                Err(_) => state,
            }
        } else {
            state
        }
    } else {
        state
    };

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
            retry_after_secs: None,
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
    fn video_candidate_serving_image_is_not_usable() {
        // A Video candidate that resolves to image/* is a poster/thumbnail leaked
        // past extraction (JSON-LD VideoObject with an extensionless contentUrl
        // pointing at the still frame). Must not pass as Usable — verify is the
        // last line of defense against a mis-extracted candidate.
        let o = outcome(200, Some("image/jpeg"), Some(40_000), None);
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::SuspectAd
        );
        // Same for Audio.
        let a = outcome(200, Some("image/png"), Some(40_000), None);
        assert_eq!(
            classify_probe(&a, CandidateKind::Audio, false),
            CandidateValidationState::SuspectAd
        );
    }

    #[test]
    fn image_candidate_serving_image_is_usable() {
        // The mismatch guard must not punish a genuine Image candidate.
        let o = outcome(200, Some("image/jpeg"), Some(40_000), None);
        assert_eq!(
            classify_probe(&o, CandidateKind::Image, false),
            CandidateValidationState::Usable
        );
    }

    #[test]
    fn ogg_application_content_type_is_usable() {
        // archive.org .ogv/.ogg derivatives are served as application/ogg, not
        // video/*; they were falling through to SuspectAd.
        let o = outcome(200, Some("application/ogg"), Some(50_000_000), None);
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::Usable
        );
        let legacy = outcome(200, Some("application/x-ogg"), Some(50_000_000), None);
        assert_eq!(
            classify_probe(&legacy, CandidateKind::Video, false),
            CandidateValidationState::Usable
        );
    }

    #[test]
    fn binary_octet_stream_video_is_usable() {
        // PeerTube object-storage backends (makertube/your-objectstorage,
        // tuba.hyperreal.top) serve real fragmented MP4s (ftyp/iso5 magic) as
        // `binary/octet-stream`, a legacy alias of application/octet-stream. These
        // are genuine playable files (200/206) that were wrongly demoted to
        // SuspectAd across many federated instances. A 206 with a real byte body
        // mirrors the live shape.
        let o = outcome(206, Some("binary/octet-stream"), Some(1_457_988_116), None);
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
    fn forbidden_with_s3_access_denied_is_failed() {
        // TED py.tedcdn.com fallback MP4s 403 with an S3 AccessDenied XML body;
        // no profile/login fixes a private object, so this is Failed, not
        // NeedsProfile.
        let body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
            <Error><Code>AccessDenied</Code><Message>Access Denied</Message></Error>";
        let o = outcome(403, Some("application/xml"), None, Some(body));
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::Failed
        );
    }

    #[test]
    fn forbidden_signature_mismatch_is_failed() {
        let body = "<Error><Code>SignatureDoesNotMatch</Code></Error>";
        let o = outcome(403, Some("application/xml"), None, Some(body));
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::Failed
        );
    }

    #[test]
    fn unauthorized_with_transient_throttle_is_suspect_ad() {
        // Odysee player.odycdn.com /streams/free/... is public no-login content
        // but 401s with this anti-bot body and flips to 429 under a Referer.
        // No profile fixes a throttle -> SuspectAd (recoverable), not NeedsProfile.
        let o = outcome(
            401,
            Some("text/plain"),
            None,
            Some("this content cannot be accessed at the moment"),
        );
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::SuspectAd
        );
    }

    #[test]
    fn forbidden_try_again_later_is_suspect_ad() {
        let o = outcome(403, Some("text/plain"), None, Some("Try again later"));
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::SuspectAd
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
    fn persistent_429_is_suspect_not_failed() {
        // A 429 surviving the rate-limit retries means "throttled, resource
        // exists" — must not be buried as terminal Failed (regression: wikimedia
        // upload variants 429 under burst but HEAD 200 once the window clears).
        let o = outcome(429, None, None, None);
        assert_eq!(
            classify_probe(&o, CandidateKind::Video, false),
            CandidateValidationState::SuspectAd
        );
    }

    #[test]
    fn drm_tag_past_16kib_still_detected() {
        // A multivariant master whose DRM marker sits well past the old 16 KiB
        // media-probe cap. The manifest cap must be large enough that the body
        // captured for classification still includes it (regression guard for
        // the cap; classify itself scans whatever it's given).
        assert!(MANIFEST_BODY_CAP > 16 * 1024 + 32 * 1024);
        let mut body = String::from("#EXTM3U\n");
        // ~24 KiB of clear variant lines before any key tag.
        while body.len() < 24 * 1024 {
            body.push_str("#EXT-X-STREAM-INF:BANDWIDTH=800000\nlow.m3u8\n");
        }
        body.push_str("#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,KEYFORMAT=\"com.apple.streamingkeydelivery\"\n");
        assert!(body.len() > 16 * 1024 && body.len() < MANIFEST_BODY_CAP);
        let o = outcome(200, Some("application/vnd.apple.mpegurl"), None, Some(&body));
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
    fn manifest_kind_octet_stream_still_drm_parsed() {
        // A Manifest-kind candidate whose body is a DRM-protected playlist but is
        // served as application/octet-stream with no .m3u8/.mpd in the URL *path*
        // (the marker may sit in the query string for mac_cms/hanime, which match
        // url.contains(".m3u8")). Neither content_type_is_manifest nor
        // url_path_has_manifest_ext fires, so without the kind gate this would skip
        // classify_manifest and fall through to the octet-stream media arm as
        // Usable — leaking a protected stream. The kind gate must still DRM-parse.
        let body = "#EXTM3U\n#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,KEYFORMAT=\"com.apple.streamingkeydelivery\",URI=\"skd://x\"\n#EXT-X-STREAM-INF:BANDWIDTH=800000\nlow.m3u8\n";
        // outcome() hardcodes final_url to a .mp4 path, so the path-ext check is
        // guaranteed to miss — isolating the kind gate.
        let o = outcome(200, Some("application/octet-stream"), None, Some(body));
        assert_eq!(
            classify_probe(&o, CandidateKind::Manifest, false),
            CandidateValidationState::Drm
        );
    }

    #[test]
    fn manifest_kind_octet_stream_clean_is_usable() {
        // Same path, clean (no DRM) manifest: the kind gate parses it and a clean
        // playlist is Usable — the gate must not blanket-demote manifests.
        let body = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000\nlow.m3u8\n";
        let o = outcome(200, Some("application/octet-stream"), None, Some(body));
        assert_eq!(
            classify_probe(&o, CandidateKind::Manifest, false),
            CandidateValidationState::Usable
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
    fn transient_status_set() {
        // 429 + retryable 5xx get one retry before a terminal verdict.
        for s in [429u16, 500, 502, 503, 504] {
            assert!(is_transient_status(s), "{s} should be transient");
        }
        // 2xx/3xx and definitive 4xx are NOT retried.
        for s in [200u16, 206, 301, 400, 401, 403, 404, 410, 451, 501] {
            assert!(!is_transient_status(s), "{s} should NOT be transient");
        }
    }

    #[test]
    fn first_hls_variant_resolves_relative_and_skips_comments() {
        let base = Url::parse("https://cdn.example/hls/master.m3u8").unwrap();
        // Relative variant URI -> resolved against the master's dir.
        let rel = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000\nlow/v.m3u8\n";
        assert_eq!(
            first_hls_variant_url(&base, rel).unwrap().as_str(),
            "https://cdn.example/hls/low/v.m3u8"
        );
        // Comment/blank lines between the tag and the URI are skipped.
        let gap = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000\n\n#comment\nv1.m3u8\n";
        assert_eq!(
            first_hls_variant_url(&base, gap).unwrap().as_str(),
            "https://cdn.example/hls/v1.m3u8"
        );
        // A media playlist (no #EXT-X-STREAM-INF) yields no variant.
        let media = "#EXTM3U\n#EXTINF:6,\nseg0.ts\n";
        assert!(first_hls_variant_url(&base, media).is_none());
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
