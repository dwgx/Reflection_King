//! Generic headless extractor.
//!
//! The broad-coverage tier that does *not* need an external tool or a real
//! browser: it fetches the page HTML once and discovers media the way a browser
//! would by reading the markup the site already ships — Open Graph / Twitter
//! card tags, JSON-LD `VideoObject`/`AudioObject`, inline state JSON
//! (`window.__playinfo__`, `__NEXT_DATA__`, ...), and bare manifest/media URLs
//! sprinkled in the page. It never downloads media or runs ffmpeg; it only
//! turns a page into [`MediaCandidate`]s.
//!
//! Discovery strategy and field mapping are adapted from yt-dlp's `GenericIE`
//! (Unlicense / public domain) and you-get's `universal` extractor (MIT); both
//! permit reuse. No GPL sources were copied.
//!
//! This runs in the `Auto` chain after `direct` and before `yt_dlp`, so cheap
//! static discovery happens before spawning an external probe.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::{header::HeaderMap, StatusCode};
use serde_json::{json, Value};
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

use crate::{
    models::{CandidateKind, CandidateProtection, CandidateValidationState, MediaCandidate},
    policy_http::{policy_client, validate_response_url_and_peer},
    url_policy::validate_url,
    Result, RkError,
};

use super::{ExtractContext, ExtractResult, SourceExtractor};

const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_HTML_BYTES: usize = 4 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
/// Cap on candidates a single generic pass emits, so a pathological page can't
/// flood the resolver.
const MAX_CANDIDATES: usize = 40;
/// How many levels of embed/oEmbed players to follow. 1 covers the common
/// "watch page → embed iframe → media" shape (e.g. PeerTube) without letting a
/// chain of nested players run away.
const MAX_FOLLOW_DEPTH: usize = 1;
/// Cap on oEmbed/iframe targets followed per page.
const MAX_FOLLOW_PER_PAGE: usize = 6;

pub struct GenericExtractor;

#[async_trait]
impl SourceExtractor for GenericExtractor {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn matches(&self, ctx: &ExtractContext) -> bool {
        // Applicable to any http(s) page. It is ordered last-resort-ish in the
        // auto chain (after direct), so matching broadly is intentional.
        matches!(ctx.url.scheme(), "http" | "https")
    }

    async fn extract(&self, ctx: &ExtractContext) -> Result<ExtractResult> {
        let mut candidates: Vec<MediaCandidate> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        // De-dup media URLs across every page we visit; reinforce on repeats.
        let mut seen_urls = std::collections::HashSet::new();
        // Pages already fetched, so an embed cycle can't loop us forever.
        let mut seen_pages = std::collections::HashSet::new();
        // Worklist of (page URL, depth). Depth 0 is the requested page; embeds
        // and oEmbed players are followed up to `MAX_FOLLOW_DEPTH`.
        let mut queue: std::collections::VecDeque<(Url, usize)> =
            std::collections::VecDeque::new();
        queue.push_back((ctx.url.clone(), 0));

        while let Some((page_url, depth)) = queue.pop_front() {
            if candidates.len() >= MAX_CANDIDATES {
                break;
            }
            if !seen_pages.insert(page_url.to_string()) {
                continue;
            }
            let html = match fetch_page(&page_url).await {
                Ok(html) => html,
                Err(error) => {
                    warnings.push(format!("generic fetch failed ({page_url}): {error}"));
                    continue;
                }
            };

            let mut collector = Collector::new(ctx, page_url.clone());
            collector.scan(&html);

            // Merge this page's candidates into the global set with de-dup.
            for candidate in std::mem::take(&mut collector.candidates) {
                if seen_urls.insert(candidate.url.clone()) {
                    candidates.push(candidate);
                } else if let Some(existing) =
                    candidates.iter_mut().find(|c| c.url == candidate.url)
                {
                    existing.evidence_count += 1;
                    existing.score += 5;
                }
            }
            warnings.append(&mut collector.warnings);

            // Queue embeds/oEmbed players for the next depth level.
            if depth < MAX_FOLLOW_DEPTH {
                for embed in std::mem::take(&mut collector.follow) {
                    if let Ok(target) = page_url.join(&embed) {
                        if matches!(target.scheme(), "http" | "https") {
                            queue.push_back((target, depth + 1));
                        }
                    }
                }
                for endpoint in std::mem::take(&mut collector.oembed_endpoints) {
                    match fetch_oembed_target(&page_url, &endpoint).await {
                        Ok(Some(target)) => queue.push_back((target, depth + 1)),
                        Ok(None) => {}
                        Err(error) => warnings.push(format!("oembed fetch failed: {error}")),
                    }
                }
                // Harvest media URLs directly from detected public JSON APIs.
                for endpoint in std::mem::take(&mut collector.api_endpoints) {
                    match fetch_api_media(ctx, &page_url, &endpoint).await {
                        Ok(api_candidates) => {
                            for candidate in api_candidates {
                                if seen_urls.insert(candidate.url.clone()) {
                                    candidates.push(candidate);
                                } else if let Some(existing) =
                                    candidates.iter_mut().find(|c| c.url == candidate.url)
                                {
                                    existing.evidence_count += 1;
                                    existing.score += 5;
                                }
                            }
                        }
                        Err(error) => warnings.push(format!("api fetch failed: {error}")),
                    }
                }
            }
        }

        candidates.sort_by_key(|candidate| -candidate.score);
        candidates.truncate(MAX_CANDIDATES);

        Ok(ExtractResult {
            candidates,
            warnings,
            browser_session: None,
            page_snapshot: None,
        })
    }
}

/// The full set of headers a current Chrome sends on a top-level navigation,
/// in Chrome's emission order. Real sites increasingly gate on the presence and
/// consistency of `sec-ch-ua` / `sec-fetch-*` (a bare UA with none of these
/// reads as a bot). Used for fetching *public* pages; the `sec-ch-ua` brand list
/// is kept consistent with `BROWSER_USER_AGENT`'s Chrome major version.
fn browser_navigation_headers() -> HeaderMap {
    use reqwest::header::{HeaderName, HeaderValue};
    let mut headers = HeaderMap::new();
    let pairs: &[(HeaderName, &str)] = &[
        (
            reqwest::header::ACCEPT,
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
        (reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9"),
        // NOTE: no manual Accept-Encoding here on purpose. reqwest is built
        // with the gzip/brotli/deflate features, so it injects its own
        // Accept-Encoding and transparently decompresses the response.
        // Setting the header here would DISABLE that auto-decoding and hand
        // us raw compressed bytes. Some sites (e.g. bilibili) gzip-encode
        // regardless of the request header, so leaning on reqwest's
        // auto-decompression is what lets the static scan see real markup.
        (
            HeaderName::from_static("sec-ch-ua"),
            "\"Chromium\";v=\"124\", \"Google Chrome\";v=\"124\", \"Not-A.Brand\";v=\"99\"",
        ),
        (HeaderName::from_static("sec-ch-ua-mobile"), "?0"),
        (HeaderName::from_static("sec-ch-ua-platform"), "\"Windows\""),
        (HeaderName::from_static("upgrade-insecure-requests"), "1"),
        (HeaderName::from_static("sec-fetch-site"), "none"),
        (HeaderName::from_static("sec-fetch-mode"), "navigate"),
        (HeaderName::from_static("sec-fetch-user"), "?1"),
        (HeaderName::from_static("sec-fetch-dest"), "document"),
    ];
    for (name, value) in pairs {
        headers.insert(name.clone(), HeaderValue::from_static(value));
    }
    headers
}

/// Fetch page HTML through the SSRF-validated policy client, following a bounded
/// number of redirects manually (the policy client disables auto-redirect so
/// each hop is re-validated). Mirrors `manifest::fetch_manifest_text`.
async fn fetch_page(start_url: &Url) -> Result<String> {
    let client = policy_client(FETCH_TIMEOUT, BROWSER_USER_AGENT)?;
    let headers = browser_navigation_headers();

    let mut url = start_url.clone();
    for redirect_count in 0..=MAX_REDIRECTS {
        validate_url(&url)?;
        let response = client.get(url.clone()).headers(headers.clone()).send().await?;
        validate_response_url_and_peer(&response)?;
        let status = response.status();

        if status.is_redirection() {
            if redirect_count == MAX_REDIRECTS {
                return Err(RkError::Source("generic page had too many redirects".to_string()));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| RkError::Source("generic redirect without Location".to_string()))?
                .to_str()
                .map_err(|error| RkError::Source(format!("invalid redirect target: {error}")))?;
            url = url.join(location)?;
            continue;
        }

        if status != StatusCode::OK {
            return Err(RkError::Source(format!("generic page returned HTTP {status}")));
        }

        // Only parse HTML-ish bodies; a non-HTML body here means `direct` should
        // have handled it (or there is nothing to discover).
        let is_html = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|ct| {
                let ct = ct.to_ascii_lowercase();
                ct.contains("text/html") || ct.contains("application/xhtml")
            })
            .unwrap_or(true);
        if !is_html {
            return Err(RkError::Source("generic page is not HTML".to_string()));
        }

        let bytes = response.bytes().await?;
        let slice = if bytes.len() > MAX_HTML_BYTES {
            &bytes[..MAX_HTML_BYTES]
        } else {
            &bytes[..]
        };
        return Ok(String::from_utf8_lossy(slice).into_owned());
    }
    Err(RkError::Source("generic page fetch exhausted redirects".to_string()))
}

/// Fetch an oEmbed endpoint and return the player/media URL it advertises, if
/// any. Per the oEmbed spec the JSON response carries either a direct `url`
/// (photo/link types) or an `html` snippet (video/rich types) whose `<iframe
/// src>` is the player. Every hop is SSRF-validated via the policy client.
async fn fetch_oembed_target(base: &Url, endpoint: &str) -> Result<Option<Url>> {
    let endpoint_url = base.join(endpoint)?;
    validate_url(&endpoint_url)?;
    let client = policy_client(FETCH_TIMEOUT, BROWSER_USER_AGENT)?;
    let response = client
        .get(endpoint_url.clone())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await?;
    validate_response_url_and_peer(&response)?;
    if response.status() != StatusCode::OK {
        return Ok(None);
    }
    let body = response.bytes().await?;
    let slice = if body.len() > MAX_HTML_BYTES { &body[..MAX_HTML_BYTES] } else { &body[..] };
    let value: Value = match serde_json::from_slice(slice) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    // Prefer an explicit direct url; otherwise pull the iframe src from `html`.
    if let Some(url) = value.get("url").and_then(Value::as_str) {
        let decoded = decode_url_escapes(url.trim());
        if let Ok(target) = endpoint_url.join(&decoded) {
            if matches!(target.scheme(), "http" | "https") {
                return Ok(Some(target));
            }
        }
    }
    if let Some(html) = value.get("html").and_then(Value::as_str) {
        let lower = html.to_ascii_lowercase();
        if let Some(rel) = lower.find("<iframe") {
            let end = lower[rel..].find('>').map(|e| rel + e + 1).unwrap_or(lower.len());
            let tag = &html[rel..end];
            let tag_lower = &lower[rel..end];
            if let Some(src) = attr_value(tag, tag_lower, "src") {
                let decoded = decode_url_escapes(src.trim());
                if let Ok(target) = endpoint_url.join(&decoded) {
                    if matches!(target.scheme(), "http" | "https") {
                        return Ok(Some(target));
                    }
                }
            }
        }
    }
    Ok(None)
}

/// Fetch a detected public JSON API and harvest media candidates from it by
/// reusing the recursive URL walk. Used for SPA platforms (PeerTube et al.)
/// whose pages only ship a player and serve media via a documented REST API.
async fn fetch_api_media(
    ctx: &ExtractContext,
    base: &Url,
    endpoint: &str,
) -> Result<Vec<MediaCandidate>> {
    let api_url = base.join(endpoint)?;
    validate_url(&api_url)?;
    let client = policy_client(FETCH_TIMEOUT, BROWSER_USER_AGENT)?;
    let response = client
        .get(api_url.clone())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await?;
    validate_response_url_and_peer(&response)?;
    if response.status() != StatusCode::OK {
        return Ok(Vec::new());
    }
    let body = response.bytes().await?;
    let slice = if body.len() > MAX_HTML_BYTES { &body[..MAX_HTML_BYTES] } else { &body[..] };
    let value: Value = match serde_json::from_slice(slice) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };
    let mut collector = Collector::new(ctx, api_url);
    collector.harvest_json_urls(&value, 0);
    Ok(std::mem::take(&mut collector.candidates))
}

/// Accumulates de-duplicated candidates discovered from one page.
struct Collector<'a> {
    ctx: &'a ExtractContext,
    base: Url,
    candidates: Vec<MediaCandidate>,
    seen: std::collections::HashSet<String>,
    warnings: Vec<String>,
    /// Embed/iframe URLs to fetch and re-scan at the next depth level.
    follow: Vec<String>,
    /// oEmbed discovery endpoints (`<link type=application/json+oembed>`).
    oembed_endpoints: Vec<String>,
    /// JSON API endpoints to fetch and harvest media URLs from (e.g. a detected
    /// PeerTube instance's public `/api/v1/videos/{id}`).
    api_endpoints: Vec<String>,
}

impl<'a> Collector<'a> {
    fn new(ctx: &'a ExtractContext, base: Url) -> Self {
        Self {
            ctx,
            base,
            candidates: Vec::new(),
            seen: std::collections::HashSet::new(),
            warnings: Vec::new(),
            follow: Vec::new(),
            oembed_endpoints: Vec::new(),
            api_endpoints: Vec::new(),
        }
    }

    /// Run every discovery pass over the page in priority order.
    fn scan(&mut self, html: &str) {
        self.scan_meta_tags(html); // og:* / twitter:*
        self.scan_json_ld(html); // application/ld+json VideoObject/AudioObject
        self.scan_inline_state(html); // window.__playinfo__ / __NEXT_DATA__ / ...
        self.scan_media_tags(html); // <video>/<audio>/<source> HTML5 embeds
        self.scan_bare_urls(html); // manifest + media URLs in raw markup
        self.scan_oembed(html); // <link rel=alternate type=application/json+oembed>
        self.scan_iframes(html); // <iframe src> embedded players
        self.detect_known_apis(); // shape-based public API discovery (PeerTube, ...)
    }

    /// Detect well-known self-hosted media platforms by URL *shape* (not domain)
    /// and queue their public REST API for media harvesting. Currently PeerTube,
    /// whose SPA serves media via `/api/v1/videos/{id}` — the page itself only
    /// ships an embed player, so static scanning alone never reaches the stream.
    /// PeerTube is federated across thousands of instances, so shape-matching is
    /// the only domain-agnostic way to support it generically.
    fn detect_known_apis(&mut self) {
        let path = self.base.path();
        // `/videos/embed/{id}` or `/videos/watch/{id}` or `/w/{id}`.
        let id = path
            .strip_prefix("/videos/embed/")
            .or_else(|| path.strip_prefix("/videos/watch/"))
            .or_else(|| path.strip_prefix("/w/"))
            .map(|rest| rest.split('/').next().unwrap_or(rest).trim());
        if let Some(id) = id {
            if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                if let Ok(api) = self.base.join(&format!("/api/v1/videos/{id}")) {
                    let s = api.to_string();
                    if !self.api_endpoints.contains(&s) {
                        self.api_endpoints.push(s);
                    }
                }
            }
        }
    }

    /// Record an embed/player URL to follow at the next depth level.
    fn follow_embed(&mut self, raw: &str) {
        let trimmed = raw.trim();
        if trimmed.is_empty() || self.follow.len() >= MAX_FOLLOW_PER_PAGE {
            return;
        }
        let decoded = decode_url_escapes(trimmed);
        if let Ok(absolute) = self.base.join(&decoded) {
            if matches!(absolute.scheme(), "http" | "https") {
                let s = absolute.to_string();
                if !self.follow.iter().any(|e| e == &s) {
                    self.follow.push(s);
                }
            }
        }
    }

    /// Resolve a possibly-relative URL against the page and add it if new.
    fn add(&mut self, raw: &str, kind: CandidateKind, method: &str, score: i64) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        // Decode the most common HTML/JS escapes seen in attribute and inline
        // JSON contexts.
        let decoded = decode_url_escapes(trimmed);
        let absolute = match self.base.join(&decoded) {
            Ok(url) => url,
            Err(_) => return,
        };
        if !matches!(absolute.scheme(), "http" | "https") {
            return;
        }
        let url_string = absolute.to_string();
        if !self.seen.insert(url_string.clone()) {
            // Already discovered (possibly by a higher-priority pass): reinforce.
            if let Some(existing) = self.candidates.iter_mut().find(|c| c.url == url_string) {
                existing.evidence_count += 1;
                existing.score += 5;
            }
            return;
        }
        if self.candidates.len() >= MAX_CANDIDATES {
            return;
        }
        self.candidates
            .push(self.build_candidate(url_string, kind, method, score));
    }

    fn build_candidate(
        &self,
        url: String,
        kind: CandidateKind,
        method: &str,
        score: i64,
    ) -> MediaCandidate {
        MediaCandidate {
            id: Uuid::new_v4(),
            job_id: self.ctx.job_id,
            url,
            kind,
            extractor: "generic".to_string(),
            method: method.to_string(),
            status: None,
            content_type: default_content_type(kind),
            content_length: None,
            resource_type: Some("generic_discovery".to_string()),
            initiator_url: Some(self.ctx.source_url.clone()),
            quality_label: None,
            score,
            requires_authorization: false,
            platform: Some(self.ctx.platform_hint),
            route: Some("generic".to_string()),
            extractor_confidence: Some(60),
            protection: Some(CandidateProtection::None),
            requires_profile: false,
            ttl_hint_seconds: None,
            ad_risk: false,
            evidence_count: 1,
            paired_candidate_ids: Vec::new(),
            failure_reason: None,
            validation_state: Some(CandidateValidationState::Untested),
            metadata_json: json!({ "source": "generic", "method": method }),
            created_at: OffsetDateTime::now_utc(),
            score_breakdown_json: json!({
                "engine": "generic",
                "method": method,
                "base": score,
                "total": score,
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

    // --- discovery passes -------------------------------------------------

    /// Open Graph (`og:video`, `og:audio`) and Twitter card media tags. When the
    /// URL has a media extension it is a direct, high-confidence candidate. When
    /// it points at a player/embed page (no media extension — e.g. PeerTube's
    /// `og:video` is `/videos/embed/<id>`), follow it so a deeper pass can find
    /// the real stream. `og:video:url` / `twitter:player` are common embed forms.
    fn scan_meta_tags(&mut self, html: &str) {
        const VIDEO_PROPS: &[&str] =
            &["og:video", "og:video:url", "og:video:secure_url", "twitter:player:stream"];
        const AUDIO_PROPS: &[&str] = &["og:audio", "og:audio:url", "og:audio:secure_url"];
        const EMBED_PROPS: &[&str] = &["og:video:iframe", "twitter:player"];

        for prop in VIDEO_PROPS {
            for content in meta_contents(html, prop) {
                match classify_url(&content) {
                    Some(kind) => self.add(&content, kind, "og", 60),
                    None => self.follow_embed(&content), // player/embed page
                }
            }
        }
        for prop in AUDIO_PROPS {
            for content in meta_contents(html, prop) {
                match classify_url(&content) {
                    Some(kind) => self.add(&content, kind, "og", 58),
                    None => self.follow_embed(&content),
                }
            }
        }
        for prop in EMBED_PROPS {
            for content in meta_contents(html, prop) {
                self.follow_embed(&content);
            }
        }
    }

    /// JSON-LD blocks: parse each `<script type="application/ld+json">` body and
    /// pull `contentUrl` from `VideoObject` / `AudioObject` nodes (possibly
    /// nested under `@graph` or arrays). Mirrors yt-dlp `extract_video_object`.
    fn scan_json_ld(&mut self, html: &str) {
        for block in script_bodies(html, "application/ld+json") {
            let Ok(value) = serde_json::from_str::<Value>(block.trim()) else {
                continue;
            };
            self.walk_json_ld(&value);
        }
    }

    fn walk_json_ld(&mut self, value: &Value) {
        match value {
            Value::Array(items) => {
                for item in items {
                    self.walk_json_ld(item);
                }
            }
            Value::Object(map) => {
                if let Some(graph) = map.get("@graph") {
                    self.walk_json_ld(graph);
                }
                let ty = map.get("@type").and_then(Value::as_str).unwrap_or("");
                let kind = match ty {
                    "VideoObject" => Some(CandidateKind::Video),
                    "AudioObject" => Some(CandidateKind::Audio),
                    _ => None,
                };
                if let Some(kind) = kind {
                    if let Some(url) = map.get("contentUrl").and_then(Value::as_str) {
                        self.add(url, classify_url(url).unwrap_or(kind), "json_ld", 80);
                    }
                    // `embedUrl` points at a player page; follow it for the stream.
                    if let Some(embed) = map.get("embedUrl").and_then(Value::as_str) {
                        match classify_url(embed) {
                            Some(k) => self.add(embed, k, "json_ld", 78),
                            None => self.follow_embed(embed),
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Inline state objects various frameworks ship in `<script>`. We extract the
    /// JSON blob for a handful of well-known globals and harvest any media URLs
    /// inside it (manifest/media URLs are found by the recursive value walk).
    fn scan_inline_state(&mut self, html: &str) {
        const GLOBALS: &[&str] = &[
            "window.__playinfo__",
            "window.__INITIAL_STATE__",
            "window.__NUXT__",
            "window.__data",
            "ytInitialPlayerResponse",
            // Player state objects used by other SPA players. acfun ships its
            // play info under `window.videoInfo` / `window.pageInfo`
            // (`currentVideoInfo.ksPlayJson`); harvesting these structurally is
            // higher-confidence than catching the same URLs via the bare-URL
            // sniff pass.
            "window.videoInfo",
            "window.pageInfo",
            "window.videoResource",
        ];
        for global in GLOBALS {
            if let Some(blob) = inline_assignment_json(html, global) {
                if let Ok(value) = serde_json::from_str::<Value>(&blob) {
                    self.harvest_json_urls(&value, 0);
                }
            }
        }
        // `__NEXT_DATA__` is shipped as a typed script element, not an assignment.
        for block in script_bodies(html, "application/json") {
            if let Ok(value) = serde_json::from_str::<Value>(block.trim()) {
                self.harvest_json_urls(&value, 0);
            }
        }
    }

    /// Recursively walk a JSON value and add any string that looks like a media
    /// or manifest URL. Bounded depth keeps a deep state tree from blowing the
    /// stack or the candidate budget.
    fn harvest_json_urls(&mut self, value: &Value, depth: usize) {
        if depth > 12 || self.candidates.len() >= MAX_CANDIDATES {
            return;
        }
        match value {
            Value::String(s) => {
                // JSON-in-JSON: some players embed their play descriptor as a
                // *string* whose content is itself a JSON object holding the
                // media URLs (e.g. acfun's `ksPlayJson`). Check this BEFORE
                // classify_url: that helper blindly reads a trailing extension
                // and would mis-classify the whole blob as a media URL, then
                // bail before we ever look inside. A value that starts with
                // `{`/`[` is never itself a URL, so try parsing it once more
                // and recurse. Depth-bounded by the caller's guard.
                let t = s.trim_start();
                if t.starts_with('{') || t.starts_with('[') {
                    if (s.contains("m3u8")
                        || s.contains(".mpd")
                        || s.contains(".mp4")
                        || s.contains("playUrl")
                        || s.contains("backupUrl"))
                    {
                        if let Ok(inner) = serde_json::from_str::<Value>(s) {
                            self.harvest_json_urls(&inner, depth + 1);
                        }
                    }
                    return;
                }
                if let Some(kind) = classify_url(s) {
                    if s.starts_with("http") || s.starts_with("//") {
                        let score = match kind {
                            CandidateKind::Manifest => 75,
                            CandidateKind::Video => 70,
                            CandidateKind::Audio => 68,
                            // Skip images here: JSON blobs are full of avatar and
                            // thumbnail URLs that bury the real media. `direct`
                            // handles a genuine image target on its own.
                            _ => return,
                        };
                        self.add(s, kind, "inline_json", score);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.harvest_json_urls(item, depth + 1);
                }
            }
            Value::Object(map) => {
                for item in map.values() {
                    self.harvest_json_urls(item, depth + 1);
                }
            }
            _ => {}
        }
    }

    /// Last resort: scan raw markup for bare `.m3u8` / `.mpd` / media-file URLs
    /// (including percent-encoded and backslash-escaped forms). Adapted from
    /// you-get's `universal` extractor.
    /// HTML5 media embeds: `<video src>`, `<audio src>`, and `<source src>`
    /// (the latter nested inside a `<video>`/`<audio>`). This is the most basic
    /// way a page ships playable media, yet a bare-URL sniff misses the common
    /// case where the `src` carries no media extension and the format is only
    /// declared via the `type` attribute (e.g.
    /// `<source src="/hls/master" type="application/vnd.apple.mpegurl">`). We
    /// honour `type` first, then fall back to the URL extension. `poster`
    /// (a thumbnail) is never read, so it cannot bury the real media.
    fn scan_media_tags(&mut self, html: &str) {
        let lower = html.to_ascii_lowercase();
        for tag_name in ["<video", "<audio", "<source"] {
            let mut from = 0;
            while let Some(rel) = lower[from..].find(tag_name) {
                let start = from + rel;
                // Guard against matching a longer tag that merely starts with
                // these bytes (none today, but keep the boundary cheap & safe).
                let after = lower.as_bytes().get(start + tag_name.len());
                let boundary = matches!(after, Some(b) if b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
                let end = lower[start..].find('>').map(|e| start + e + 1).unwrap_or(lower.len());
                from = end;
                if !boundary {
                    continue;
                }
                let tag = &html[start..end];
                let tag_lower = &lower[start..end];

                let src = attr_value(tag, tag_lower, "src");
                let Some(src) = src else { continue };
                let s = src.trim();
                if s.is_empty() || s.starts_with("about:") || s.starts_with("javascript:") {
                    continue;
                }
                // Prefer the declared MIME type; fall back to the URL extension.
                let kind = attr_value(tag, tag_lower, "type")
                    .and_then(|t| classify_media_type(&t))
                    .or_else(|| classify_url(s));
                let Some(kind) = kind else { continue };
                let score = match kind {
                    CandidateKind::Manifest => 74,
                    CandidateKind::Video => 66,
                    CandidateKind::Audio => 64,
                    // A <video>/<source> never legitimately points at an image
                    // as its media; skip to avoid poster-style noise.
                    _ => continue,
                };
                self.add(s, kind, "media_tag", score);
            }
        }
    }

    fn scan_bare_urls(&mut self, html: &str) {
        for token in url_like_tokens(html) {
            if let Some(kind) = classify_url(&token) {
                let score = match kind {
                    CandidateKind::Manifest => 72,
                    CandidateKind::Video => 64,
                    CandidateKind::Audio => 62,
                    _ => continue, // skip bare images at this tier; too noisy
                };
                self.add(&token, kind, "sniff", score);
            }
        }
    }

    /// oEmbed discovery: `<link rel="alternate" type="application/json+oembed"
    /// href="...">`. The endpoint is queued for an async fetch by the driver;
    /// its JSON response (`type=video`/`rich` with an `html` iframe, or a direct
    /// `url`) yields a player URL to follow. Per the oEmbed spec.
    fn scan_oembed(&mut self, html: &str) {
        let lower = html.to_ascii_lowercase();
        let mut from = 0;
        while let Some(rel) = lower[from..].find("<link") {
            let start = from + rel;
            let end = lower[start..].find('>').map(|e| start + e + 1).unwrap_or(lower.len());
            let tag = &html[start..end];
            let tag_lower = &lower[start..end];
            from = end;

            let is_oembed = tag_lower.contains("json+oembed")
                || (tag_lower.contains("oembed") && tag_lower.contains("type="));
            if !is_oembed {
                continue;
            }
            if let Some(href) = attr_value(tag, tag_lower, "href") {
                let decoded = decode_url_escapes(href.trim());
                if let Ok(absolute) = self.base.join(&decoded) {
                    if matches!(absolute.scheme(), "http" | "https")
                        && self.oembed_endpoints.len() < MAX_FOLLOW_PER_PAGE
                    {
                        self.oembed_endpoints.push(absolute.to_string());
                    }
                }
            }
        }
    }

    /// `<iframe src="...">` embedded players. Followed at the next depth level so
    /// the player page can be scanned for its own media. Filters obvious
    /// ad/tracking and non-player frames cheaply by skipping empty/`about:` srcs.
    fn scan_iframes(&mut self, html: &str) {
        let lower = html.to_ascii_lowercase();
        let mut from = 0;
        while let Some(rel) = lower[from..].find("<iframe") {
            let start = from + rel;
            let end = lower[start..].find('>').map(|e| start + e + 1).unwrap_or(lower.len());
            let tag = &html[start..end];
            let tag_lower = &lower[start..end];
            from = end;

            // Prefer a real src; fall back to lazy-load attributes some players use.
            let src = attr_value(tag, tag_lower, "src")
                .or_else(|| attr_value(tag, tag_lower, "data-src"))
                .or_else(|| attr_value(tag, tag_lower, "data-litespeed-src"));
            if let Some(src) = src {
                let s = src.trim();
                if !s.is_empty() && !s.starts_with("about:") && !s.starts_with("javascript:") {
                    self.follow_embed(s);
                }
            }
        }
    }
}

// --- lightweight HTML/JSON scanning (no extra deps) -----------------------

/// All `content="..."` values of `<meta>` tags whose `property`/`name` equals
/// `key`. Order-insensitive to attribute layout: it locates each `<meta ...>`
/// tag and checks both attributes inside it.
fn meta_contents(html: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<meta") {
        let start = from + rel;
        let end = lower[start..].find('>').map(|e| start + e + 1).unwrap_or(lower.len());
        let tag = &html[start..end];
        let tag_lower = &lower[start..end];
        from = end;

        let names_match = attr_value(tag, tag_lower, "property")
            .map(|v| v.eq_ignore_ascii_case(key))
            .unwrap_or(false)
            || attr_value(tag, tag_lower, "name")
                .map(|v| v.eq_ignore_ascii_case(key))
                .unwrap_or(false);
        if names_match {
            if let Some(content) = attr_value(tag, tag_lower, "content") {
                out.push(content);
            }
        }
    }
    out
}

/// Value of attribute `attr` within a single tag string. `tag` is the original
/// (case-preserved) slice; `tag_lower` is its lowercase twin for locating the
/// attribute name case-insensitively.
fn attr_value(tag: &str, tag_lower: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=");
    let mut search = 0;
    while let Some(rel) = tag_lower[search..].find(&needle) {
        let at = search + rel;
        // Ensure the char before the attr name is a boundary (space/quote/<).
        let preceding_ok = at == 0
            || tag_lower.as_bytes()[at - 1].is_ascii_whitespace()
            || tag_lower.as_bytes()[at - 1] == b'"'
            || tag_lower.as_bytes()[at - 1] == b'\'';
        let value_start = at + needle.len();
        search = value_start;
        if !preceding_ok {
            continue;
        }
        let rest = &tag[value_start..];
        let quote = rest.chars().next()?;
        if quote == '"' || quote == '\'' {
            let body = &rest[1..];
            if let Some(close) = body.find(quote) {
                return Some(body[..close].to_string());
            }
        } else {
            // Unquoted attribute: read until whitespace or '>'.
            let endi = rest.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(rest.len());
            return Some(rest[..endi].to_string());
        }
    }
    None
}

/// Bodies of every `<script type="...">` element whose type contains `type_attr`.
fn script_bodies(html: &str, type_attr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<script") {
        let open_start = from + rel;
        let Some(open_end_rel) = lower[open_start..].find('>') else {
            break;
        };
        let open_end = open_start + open_end_rel + 1;
        let open_tag_lower = &lower[open_start..open_end];

        let Some(close_rel) = lower[open_end..].find("</script") else {
            break;
        };
        let body_end = open_end + close_rel;
        let body = &html[open_end..body_end];
        from = body_end;

        if open_tag_lower.contains("type=") && open_tag_lower.contains(type_attr) {
            out.push(body.to_string());
        }
    }
    out
}

/// Extract the JSON object/array literal assigned to a global, e.g. the `{...}`
/// in `window.__playinfo__ = {...}`. Returns the balanced literal as a string.
fn inline_assignment_json(html: &str, global: &str) -> Option<String> {
    let at = html.find(global)?;
    let rest = &html[at + global.len()..];
    // Skip ` = ` (and optional `var`/`const` already left of `global`).
    let eq = rest.find('=')?;
    let after = rest[eq + 1..].trim_start();
    let open = after.chars().next()?;
    let (open_c, close_c) = match open {
        '{' => ('{', '}'),
        '[' => ('[', ']'),
        _ => return None,
    };
    balanced_slice(after, open_c, close_c)
}

/// Return the substring covering a balanced `open`/`close` run starting at the
/// first character, respecting string literals so braces inside strings don't
/// unbalance the count.
fn balanced_slice(text: &str, open_c: char, close_c: char) -> Option<String> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut string_quote = b'"';
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == string_quote {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => {
                in_string = true;
                string_quote = b;
            }
            c if c == open_c as u8 => depth += 1,
            c if c == close_c as u8 => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Tokens in raw markup that look like absolute URLs, split on quote/whitespace
/// delimiters. Backslash-escaped slashes are normalised by the caller via
/// `decode_url_escapes`.
fn url_like_tokens(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for marker in ["http://", "https://"] {
        let mut from = 0;
        while let Some(rel) = html[from..].find(marker) {
            let start = from + rel;
            let tail = &html[start..];
            let endi = tail
                .find(|c: char| {
                    c == '"' || c == '\'' || c == '<' || c == '>' || c == ' ' || c == '\n'
                        || c == '\t' || c == '\r' || c == ')' || c == '}'
                })
                .unwrap_or(tail.len());
            let token = &tail[..endi];
            from = start + endi.max(marker.len());
            if token.len() > marker.len() && seen.insert(token.to_string()) {
                out.push(token.to_string());
            }
        }
    }
    out
}

/// Decode the handful of escapes common in HTML attributes and inline JSON so a
/// URL joins/parses correctly: `\/`, `&amp;`, `&#x2F;`, `/`.
fn decode_url_escapes(input: &str) -> String {
    let mut s = input.replace("\\/", "/");
    if s.contains('&') {
        s = s
            .replace("&amp;", "&")
            .replace("&#x2F;", "/")
            .replace("&#47;", "/")
            .replace("&#38;", "&");
    }
    if s.contains("\\u") {
        s = s.replace("\\u002F", "/").replace("\\u002f", "/").replace("\\u0026", "&");
    }
    s
}

/// Classify a URL by extension into a candidate kind. Returns `None` for URLs
/// with no media-ish extension.
/// Map an HTML `type`/MIME attribute to a candidate kind. Lets `<source>`
/// elements declare HLS/DASH manifests whose `src` carries no file extension.
fn classify_media_type(mime: &str) -> Option<CandidateKind> {
    let m = mime.trim().to_ascii_lowercase();
    // Strip any `; codecs="..."` parameter.
    let base = m.split(';').next().unwrap_or(&m).trim();
    match base {
        "application/vnd.apple.mpegurl"
        | "application/x-mpegurl"
        | "audio/mpegurl"
        | "audio/x-mpegurl"
        | "application/dash+xml" => Some(CandidateKind::Manifest),
        _ if base.starts_with("video/") => Some(CandidateKind::Video),
        _ if base.starts_with("audio/") => Some(CandidateKind::Audio),
        _ => None,
    }
}

fn classify_url(url: &str) -> Option<CandidateKind> {
    // Strip query/fragment before looking at the extension.
    let path = url.split(['?', '#']).next().unwrap_or(url).to_ascii_lowercase();
    let ext = path.rsplit('/').next().and_then(|seg| seg.rsplit_once('.')).map(|(_, e)| e)?;
    match ext {
        "m3u8" | "mpd" => Some(CandidateKind::Manifest),
        // `.ogv` (Ogg video) and `.ogm` are common on archive.org / Wikimedia
        // alongside `.ogg` (audio); without them an og:video pointing at a
        // `.ogv` mis-classifies as None and gets treated as an embed page to
        // follow rather than a direct video (observed: archive.org Daffy Duck).
        "mp4" | "m4v" | "webm" | "flv" | "mov" | "mkv" | "m4s" | "ogv" | "ogm" => {
            Some(CandidateKind::Video)
        }
        "mp3" | "m4a" | "aac" | "wav" | "flac" | "opus" | "ogg" => Some(CandidateKind::Audio),
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" => Some(CandidateKind::Image),
        _ => None,
    }
}

fn default_content_type(kind: CandidateKind) -> Option<String> {
    let value = match kind {
        CandidateKind::Manifest => "application/vnd.apple.mpegurl",
        CandidateKind::Video => "video/mp4",
        CandidateKind::Audio => "audio/mpeg",
        CandidateKind::Image => "image/jpeg",
        _ => return None,
    };
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live network test: run the real extractor against a public PeerTube watch
    /// page (ships og:video + JSON-LD VideoObject). `#[ignore]` so normal `cargo
    /// test` stays offline; run with `cargo test -p reflection-core -- --ignored
    /// generic_live`.
    #[tokio::test]
    #[ignore]
    async fn generic_live_discovers_peertube_media() {
        let url = "https://framatube.org/w/9c9de5e8-0a1e-484a-b099-e80766180a6d";
        let ctx = ExtractContext {
            job_id: Uuid::new_v4(),
            source_url: url.to_string(),
            url: Url::parse(url).unwrap(),
            outputs: vec![crate::models::OutputKind::Video],
            profile_id: "p".to_string(),
            discovery: crate::models::DiscoveryMode::Auto,
            platform_hint: crate::models::PlatformHint::Auto,
            auth_mode: crate::models::AuthMode::None,
            page_archive_capture_cdp_enabled: false,
            page_archive_save_mhtml_enabled: false,
            page_archive_save_har_enabled: false,
            page_archive_cdp_body_max_bytes: 0,
            page_archive_cdp_body_total_bytes: 0,
            yt_dlp: None,
            external_tools: Vec::new(),
            browser: None,
        };
        let result = GenericExtractor.extract(&ctx).await.expect("extract ok");
        for c in &result.candidates {
            eprintln!("[{}] score={} kind={:?} {}", c.method, c.score, c.kind, c.url);
        }
        assert!(
            !result.candidates.is_empty(),
            "expected at least one media candidate from a real PeerTube page; warnings={:?}",
            result.warnings
        );
    }

    #[test]
    fn meta_tags_extract_og_video() {
        let html = r#"<meta property="og:video" content="https://cdn.example.com/v.mp4">"#;
        let got = meta_contents(html, "og:video");
        assert_eq!(got, vec!["https://cdn.example.com/v.mp4"]);
    }

    #[test]
    fn meta_tags_handle_attr_order_and_single_quotes() {
        let html = "<meta content='https://x/y.m3u8' name='twitter:player:stream' />";
        let got = meta_contents(html, "twitter:player:stream");
        assert_eq!(got, vec!["https://x/y.m3u8"]);
    }

    #[test]
    fn inline_assignment_balances_braces_in_strings() {
        let html = r#"<script>window.__playinfo__={"u":"a}b","n":1}</script>"#;
        let blob = inline_assignment_json(html, "window.__playinfo__").unwrap();
        assert_eq!(blob, r#"{"u":"a}b","n":1}"#);
    }

    #[test]
    fn classify_url_reads_extension_past_query() {
        assert_eq!(classify_url("https://x/a.m3u8?token=1"), Some(CandidateKind::Manifest));
        assert_eq!(classify_url("https://x/a.mp4#t=10"), Some(CandidateKind::Video));
        assert_eq!(classify_url("https://x/page.html"), None);
    }

    #[test]
    fn classify_url_ogg_family() {
        // `.ogv`/`.ogm` are video (archive.org / Wikimedia), `.ogg` is audio.
        // Spaces in the path don't change the extension read.
        assert_eq!(classify_url("https://x/a.ogv"), Some(CandidateKind::Video));
        assert_eq!(classify_url("https://x/a.ogm"), Some(CandidateKind::Video));
        assert_eq!(classify_url("https://x/Daffy Duck (1939).ogv"), Some(CandidateKind::Video));
        assert_eq!(classify_url("https://x/a.ogg"), Some(CandidateKind::Audio));
    }

    #[test]
    fn decode_handles_backslash_and_entities() {
        assert_eq!(decode_url_escapes(r"https:\/\/x\/a.mp4"), "https://x/a.mp4");
        assert_eq!(decode_url_escapes("https://x/a?b=1&amp;c=2"), "https://x/a?b=1&c=2");
    }

    #[test]
    fn script_bodies_filter_by_type() {
        let html = r#"<script type="application/ld+json">{"x":1}</script><script>nope</script>"#;
        let got = script_bodies(html, "application/ld+json");
        assert_eq!(got, vec![r#"{"x":1}"#]);
    }

    #[test]
    fn url_like_tokens_splits_on_delimiters() {
        let html = r#"a "https://x/a.mp4" b https://y/b.m3u8) c"#;
        let got = url_like_tokens(html);
        assert!(got.contains(&"https://x/a.mp4".to_string()));
        assert!(got.contains(&"https://y/b.m3u8".to_string()));
    }

    fn test_ctx(url: &str) -> ExtractContext {
        ExtractContext {
            job_id: Uuid::new_v4(),
            source_url: url.to_string(),
            url: Url::parse(url).unwrap(),
            outputs: vec![crate::models::OutputKind::Video],
            profile_id: "p".to_string(),
            discovery: crate::models::DiscoveryMode::Auto,
            platform_hint: crate::models::PlatformHint::Auto,
            auth_mode: crate::models::AuthMode::None,
            page_archive_capture_cdp_enabled: false,
            page_archive_save_mhtml_enabled: false,
            page_archive_save_har_enabled: false,
            page_archive_cdp_body_max_bytes: 0,
            page_archive_cdp_body_total_bytes: 0,
            yt_dlp: None,
            external_tools: Vec::new(),
            browser: None,
        }
    }

    #[test]
    fn og_video_with_media_ext_is_candidate_not_followed() {
        let ctx = test_ctx("https://site.example/watch/1");
        let mut c = Collector::new(&ctx, ctx.url.clone());
        c.scan_meta_tags(r#"<meta property="og:video" content="https://cdn.example/v.mp4">"#);
        assert_eq!(c.candidates.len(), 1);
        assert!(c.follow.is_empty());
    }

    #[test]
    fn og_video_embed_page_is_followed_not_candidate() {
        let ctx = test_ctx("https://framatube.org/w/abc");
        let mut c = Collector::new(&ctx, ctx.url.clone());
        // PeerTube shape: og:video points at a player page with no media ext.
        c.scan_meta_tags(r#"<meta property="og:video" content="https://framatube.org/videos/embed/abc">"#);
        assert!(c.candidates.is_empty());
        assert_eq!(c.follow, vec!["https://framatube.org/videos/embed/abc".to_string()]);
    }

    #[test]
    fn oembed_link_is_discovered() {
        let ctx = test_ctx("https://site.example/watch/1");
        let mut c = Collector::new(&ctx, ctx.url.clone());
        c.scan_oembed(
            r#"<link rel="alternate" type="application/json+oembed" href="https://site.example/oembed?url=x">"#,
        );
        assert_eq!(
            c.oembed_endpoints,
            vec!["https://site.example/oembed?url=x".to_string()]
        );
    }

    #[test]
    fn iframe_src_is_followed_and_junk_skipped() {
        let ctx = test_ctx("https://site.example/watch/1");
        let mut c = Collector::new(&ctx, ctx.url.clone());
        c.scan_iframes(
            r#"<iframe src="https://player.example/e/9"></iframe><iframe src="about:blank"></iframe>"#,
        );
        assert_eq!(c.follow, vec!["https://player.example/e/9".to_string()]);
    }

    #[test]
    fn media_tag_source_uses_type_when_url_has_no_extension() {
        let ctx = test_ctx("https://site.example/watch");
        let mut c = Collector::new(&ctx, ctx.url.clone());
        // HLS source whose src carries no extension; only `type` declares it.
        c.scan_media_tags(
            r#"<video poster="https://site.example/thumb.jpg" controls>
                 <source src="https://cdn.example/hls/master" type="application/vnd.apple.mpegurl">
               </video>"#,
        );
        assert_eq!(c.candidates.len(), 1, "expected exactly the HLS source");
        let cand = &c.candidates[0];
        assert_eq!(cand.url, "https://cdn.example/hls/master");
        assert_eq!(cand.kind, CandidateKind::Manifest);
        assert_eq!(cand.method, "media_tag");
    }

    #[test]
    fn media_tag_video_src_falls_back_to_extension_and_skips_poster() {
        let ctx = test_ctx("https://site.example/watch");
        let mut c = Collector::new(&ctx, ctx.url.clone());
        c.scan_media_tags(
            r#"<video src="https://cdn.example/clip.mp4" poster="https://cdn.example/p.png"></video>"#,
        );
        // Only the mp4 src; the poster image must never become a candidate.
        assert_eq!(c.candidates.len(), 1);
        assert_eq!(c.candidates[0].url, "https://cdn.example/clip.mp4");
        assert_eq!(c.candidates[0].kind, CandidateKind::Video);
    }

    #[test]
    fn json_in_json_string_is_reparsed_for_media() {
        let ctx = test_ctx("https://www.acfun.cn/v/ac1");
        let mut c = Collector::new(&ctx, ctx.url.clone());
        // Outer object holds `ksPlayJson` whose *string* value is itself JSON
        // carrying the real HLS url (acfun's shape).
        let inner = r#"{\"adaptationSet\":[{\"representation\":[{\"url\":\"https://v.acfun.cn/p/abc-hls_1080p.m3u8\"}]}]}"#;
        let outer = format!(r#"{{"currentVideoInfo":{{"ksPlayJson":"{inner}"}}}}"#);
        let value: serde_json::Value = serde_json::from_str(&outer).unwrap();
        c.harvest_json_urls(&value, 0);
        assert!(
            c.candidates
                .iter()
                .any(|cand| cand.url == "https://v.acfun.cn/p/abc-hls_1080p.m3u8"
                    && cand.method == "inline_json"),
            "expected the m3u8 nested in the ksPlayJson string to be harvested, got {:?}",
            c.candidates.iter().map(|x| &x.url).collect::<Vec<_>>()
        );
    }

    #[test]
    fn json_ld_embed_url_is_followed() {
        let ctx = test_ctx("https://site.example/watch/1");
        let mut c = Collector::new(&ctx, ctx.url.clone());
        c.scan_json_ld(
            r#"<script type="application/ld+json">{"@type":"VideoObject","embedUrl":"https://player.example/e/5"}</script>"#,
        );
        assert!(c.candidates.is_empty());
        assert_eq!(c.follow, vec!["https://player.example/e/5".to_string()]);
    }

    #[test]
    fn peertube_embed_shape_queues_public_api() {
        let ctx = test_ctx("https://framatube.org/videos/embed/kkGMgK9ZtnKfYAgnEtQxbv");
        let mut c = Collector::new(&ctx, ctx.url.clone());
        c.detect_known_apis();
        assert_eq!(
            c.api_endpoints,
            vec!["https://framatube.org/api/v1/videos/kkGMgK9ZtnKfYAgnEtQxbv".to_string()]
        );
    }

    #[test]
    fn non_peertube_path_queues_no_api() {
        let ctx = test_ctx("https://site.example/some/other/page");
        let mut c = Collector::new(&ctx, ctx.url.clone());
        c.detect_known_apis();
        assert!(c.api_endpoints.is_empty());
    }
}
