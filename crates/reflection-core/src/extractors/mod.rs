//! Source extraction framework.
//!
//! Discovery is modeled as a chain of small adapters. Each [`SourceExtractor`]
//! turns an input page/media URL into media candidates and nothing else — it
//! never writes files or spawns ffmpeg. A [`SourceResolver`] runs an ordered
//! chain and returns the first extractor that produces candidates, recording
//! every attempt so the job trace shows exactly who was tried and why.
//!
//! The 3-tier chain (per the approved design):
//!   direct/manifest -> dedicated fast extractor -> yt-dlp -> real browser.

mod browser;
mod direct;
mod external_tool;
mod generic;
mod hanime;
mod mac_cms;
pub mod verify;
mod yt_dlp;

pub use browser::BrowserExtractor;
pub use direct::DirectExtractor;
pub use external_tool::ExternalToolExtractor;
pub use generic::GenericExtractor;
pub use hanime::HanimeExtractor;
pub use mac_cms::MacCmsEpisodeExtractor;
pub use yt_dlp::YtDlpExtractor;

use async_trait::async_trait;
use std::collections::HashSet;
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

use crate::{
    browser_probe::BrowserProbeClient,
    external_probe::YtDlpProbe,
    external_tools::{ExternalToolKind, ExternalToolProbe},
    models::{AuthMode, DiscoveryMode, MediaCandidate, OutputKind, PlatformHint},
    observability::{BrowserSession, ErrorClass},
    Result,
};

/// Everything an extractor needs for one job. Service handles are cheap clones
/// (both probe clients are `Clone`), so the context is self-owned and carries no
/// borrows across `await`.
#[derive(Clone)]
pub struct ExtractContext {
    pub job_id: Uuid,
    pub source_url: String,
    pub url: Url,
    pub outputs: Vec<OutputKind>,
    pub profile_id: String,
    pub discovery: DiscoveryMode,
    pub platform_hint: PlatformHint,
    pub auth_mode: AuthMode,
    pub page_archive_capture_cdp_enabled: bool,
    pub page_archive_save_mhtml_enabled: bool,
    pub page_archive_save_har_enabled: bool,
    pub page_archive_cdp_body_max_bytes: u64,
    pub page_archive_cdp_body_total_bytes: u64,
    pub yt_dlp: Option<YtDlpProbe>,
    pub external_tools: Vec<ExternalToolProbe>,
    pub browser: Option<BrowserProbeClient>,
}

impl ExtractContext {
    pub fn host(&self) -> Option<&str> {
        self.url.host_str()
    }

    pub fn output_names(&self) -> Vec<String> {
        self.outputs
            .iter()
            .map(|output| output.as_str().to_string())
            .collect()
    }
}

/// What an extractor produces: candidates plus any browser session it opened and
/// any non-fatal warnings. Extractors stay free of database concerns; the caller
/// persists candidates, sessions, and events.
#[derive(Default)]
pub struct ExtractResult {
    pub candidates: Vec<MediaCandidate>,
    pub warnings: Vec<String>,
    pub browser_session: Option<BrowserSession>,
    pub page_snapshot: Option<crate::browser_probe::PageSnapshot>,
}

impl ExtractResult {
    pub fn candidates(candidates: Vec<MediaCandidate>) -> Self {
        Self {
            candidates,
            warnings: Vec::new(),
            browser_session: None,
            page_snapshot: None,
        }
    }
}

/// A small adapter that turns a URL into candidates. Implementations must not
/// download files or run ffmpeg.
#[async_trait]
pub trait SourceExtractor: Send + Sync {
    /// Stable identifier used in the trace and `resolved_extractor`.
    fn name(&self) -> &'static str;

    /// Whether this extractor is applicable to the context (host match, required
    /// service configured, etc.).
    fn matches(&self, ctx: &ExtractContext) -> bool;

    /// Produce candidates. Returning an empty candidate list (not an error) lets
    /// the resolver fall through to the next extractor in the chain.
    async fn extract(&self, ctx: &ExtractContext) -> Result<ExtractResult>;
}

/// One extractor attempt, recorded for the job trace.
#[derive(Debug, Clone)]
pub struct AttemptLog {
    pub extractor: String,
    pub candidate_count: usize,
    pub warnings: Vec<String>,
    pub error: Option<String>,
    pub error_class: ErrorClass,
    pub duration_ms: i64,
}

/// Aggregate outcome of running a resolver chain.
#[derive(Default)]
pub struct ResolveOutcome {
    /// Name of the extractor that produced candidates, if any.
    pub winner: Option<String>,
    /// Extractor names that matched and were tried, in order.
    pub chain: Vec<String>,
    pub attempts: Vec<AttemptLog>,
    pub candidates: Vec<MediaCandidate>,
    pub warnings: Vec<String>,
    pub browser_sessions: Vec<BrowserSession>,
    pub page_snapshots: Vec<crate::browser_probe::PageSnapshot>,
}

impl ResolveOutcome {
    /// `street_voice>browser_probe`-style summary of the tried chain.
    pub fn chain_label(&self) -> String {
        self.chain.join(">")
    }
}

/// Runs an ordered chain of extractors and returns the first one that yields
/// candidates. The chain composition is chosen from the job's discovery mode.
pub struct SourceResolver {
    extractors: Vec<Box<dyn SourceExtractor>>,
}

impl SourceResolver {
    pub fn new(extractors: Vec<Box<dyn SourceExtractor>>) -> Self {
        Self { extractors }
    }

    /// Build the extractor chain appropriate for a discovery mode.
    ///
    /// - `Direct`: direct file/manifest URL only.
    /// - `External`: yt-dlp only.
    /// - `Browser`: real browser only.
    /// - `Auto`: direct/manifest fast path, then yt-dlp, then browser.
    ///   Dedicated per-site extractors (e.g. StreetVoice) insert ahead of yt-dlp.
    pub fn for_discovery(discovery: DiscoveryMode) -> Self {
        let extractors: Vec<Box<dyn SourceExtractor>> = match discovery {
            DiscoveryMode::Direct => vec![Box::new(DirectExtractor)],
            DiscoveryMode::External => vec![Box::new(YtDlpExtractor)],
            DiscoveryMode::Browser => vec![Box::new(BrowserExtractor)],
            DiscoveryMode::Auto => vec![
                Box::new(DirectExtractor),
                Box::new(HanimeExtractor),
                Box::new(MacCmsEpisodeExtractor),
                Box::new(GenericExtractor),
                Box::new(YtDlpExtractor),
                Box::new(ExternalToolExtractor::new(ExternalToolKind::YouGet)),
                Box::new(ExternalToolExtractor::new(ExternalToolKind::Lux)),
                Box::new(ExternalToolExtractor::new(ExternalToolKind::Streamlink)),
                Box::new(BrowserExtractor),
            ],
        };
        Self::new(extractors)
    }

    /// Run the chain. Explicit modes keep the first extractor that yields
    /// candidates. `Auto` aggregates all matched extractors so platform-specific
    /// browser evidence and external JSON extractors can reinforce each other.
    pub async fn resolve(&self, ctx: &ExtractContext) -> ResolveOutcome {
        let mut outcome = ResolveOutcome::default();
        let aggregate = ctx.discovery == DiscoveryMode::Auto;
        let mut seen_urls = HashSet::new();
        let verify_cfg = verify::VerifyConfig::from_env();

        for extractor in &self.extractors {
            if !extractor.matches(ctx) {
                continue;
            }
            let name = extractor.name().to_string();

            // SAFE short-circuit: before running an expensive extractor (browser /
            // external tools), verify the cheap candidates gathered so far. Skip the
            // expensive extractor ONLY if at least one candidate is confirmed
            // `Usable`. Never break on an `Untested` candidate (audit guard).
            if aggregate && verify_cfg.enabled && is_expensive_extractor(&name) {
                verify::verify_top_n(&mut outcome.candidates, &verify_cfg).await;
                if verify::has_usable(&outcome.candidates) {
                    break;
                }
            }

            outcome.chain.push(name.clone());

            let started = OffsetDateTime::now_utc();
            let result = extractor.extract(ctx).await;
            let duration_ms = ((OffsetDateTime::now_utc() - started).whole_milliseconds()) as i64;

            match result {
                Ok(mut result) => {
                    let candidate_count = result.candidates.len();
                    if let Some(session) = result.browser_session.take() {
                        outcome.browser_sessions.push(session);
                    }
                    if let Some(snapshot) = result.page_snapshot.take() {
                        outcome.page_snapshots.push(snapshot);
                    }
                    outcome.attempts.push(AttemptLog {
                        extractor: name.clone(),
                        candidate_count,
                        warnings: result.warnings.clone(),
                        error: None,
                        error_class: ErrorClass::None,
                        duration_ms,
                    });
                    outcome.warnings.extend(result.warnings);

                    if candidate_count > 0 {
                        if outcome.winner.is_none() {
                            outcome.winner = Some(name.clone());
                        }
                        if aggregate {
                            for mut candidate in result.candidates {
                                let key = candidate.url.clone();
                                if seen_urls.insert(key) {
                                    candidate.evidence_count = candidate.evidence_count.max(1);
                                    outcome.candidates.push(candidate);
                                } else if let Some(existing) = outcome
                                    .candidates
                                    .iter_mut()
                                    .find(|existing| existing.url == candidate.url)
                                {
                                    existing.evidence_count += 1;
                                    existing.score = existing.score.max(candidate.score) + 5;
                                }
                            }
                        } else {
                            outcome.candidates = result.candidates;
                            break;
                        }
                    }
                }
                Err(error) => {
                    outcome.attempts.push(AttemptLog {
                        extractor: name,
                        candidate_count: 0,
                        warnings: Vec::new(),
                        error: Some(error.to_string()),
                        error_class: classify(&error),
                        duration_ms,
                    });
                }
            }
        }

        // Final verification pass (idempotent: already-verified candidates are
        // skipped). Covers the no-short-circuit path and non-Auto modes.
        verify::verify_top_n(&mut outcome.candidates, &verify_cfg).await;
        if verify_cfg.enabled {
            verify::sort_verified(&mut outcome.candidates);
        } else {
            outcome.candidates.sort_by_key(|candidate| -candidate.score);
        }
        outcome.candidates.truncate(verify_cfg.truncate_max);
        outcome
    }
}

/// Expensive extractors run a real browser or shell out to external tools; the
/// verification short-circuit aims to skip these once a cheap candidate is
/// confirmed playable. Names match `SourceExtractor::name()`.
fn is_expensive_extractor(name: &str) -> bool {
    matches!(
        name,
        "browser_probe" | "yt_dlp" | "you_get" | "lux" | "streamlink"
    )
}

/// Map a backend error to an [`ErrorClass`] for attempt records. Kept here so the
/// core crate has no dependency on the API crate's classifier.
fn classify(error: &crate::RkError) -> ErrorClass {
    use crate::RkError;
    match error {
        RkError::UrlPolicy(_) => ErrorClass::Blocked,
        RkError::Browser(_) => ErrorClass::Blocked,
        RkError::DownloadTooLarge { .. } => ErrorClass::TooLarge,
        RkError::Transcode(_) | RkError::Json(_) => ErrorClass::Parse,
        RkError::Http(http) if http.is_timeout() => ErrorClass::Timeout,
        RkError::Http(http) if http.is_connect() => ErrorClass::Dns,
        RkError::Http(http) => match http.status() {
            Some(status) if status.is_server_error() => ErrorClass::Http5xx,
            Some(status) if status.is_client_error() => ErrorClass::Http4xx,
            _ => ErrorClass::Internal,
        },
        RkError::Source(message) => {
            let lowered = message.to_ascii_lowercase();
            if lowered.contains("timed out") {
                ErrorClass::Timeout
            } else if lowered.contains("drm") {
                ErrorClass::DrmBlocked
            } else {
                ErrorClass::Blocked
            }
        }
        _ => ErrorClass::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CandidateKind, CandidateProtection, CandidateValidationState};
    use crate::RkError;

    fn ctx(url: &str) -> ExtractContext {
        ctx_with_discovery(url, DiscoveryMode::Auto)
    }

    // Build a yt-dlp probe from the optional RK_CHAIN_YTDLP env var (path to the
    // yt-dlp binary). Returns None when unset or the path does not exist, so the
    // catalog harness silently runs the Direct+Generic chain only — CI without
    // yt-dlp installed stays green. yt-dlp itself reads ALL_PROXY/HTTPS_PROXY from
    // the environment, so routing through the home-cloud SOCKS proxy needs no
    // extra wiring here (export ALL_PROXY=socks5h://127.0.0.1:1080 before the run).
    fn chain_yt_dlp_probe() -> Option<YtDlpProbe> {
        let path = std::env::var("RK_CHAIN_YTDLP").ok()?;
        let path = std::path::PathBuf::from(path);
        if !path.exists() {
            return None;
        }
        Some(YtDlpProbe::new(
            path,
            std::time::Duration::from_secs(60),
            16 * 1024 * 1024,
        ))
    }

    fn ctx_with_discovery(url: &str, discovery: DiscoveryMode) -> ExtractContext {
        ExtractContext {
            job_id: Uuid::new_v4(),
            source_url: url.to_string(),
            url: Url::parse(url).unwrap(),
            outputs: vec![OutputKind::Audio],
            profile_id: "p".to_string(),
            discovery,
            platform_hint: PlatformHint::Auto,
            auth_mode: AuthMode::None,
            page_archive_capture_cdp_enabled: true,
            page_archive_save_mhtml_enabled: true,
            page_archive_save_har_enabled: true,
            page_archive_cdp_body_max_bytes: 2 * 1024 * 1024,
            page_archive_cdp_body_total_bytes: 64 * 1024 * 1024,
            yt_dlp: chain_yt_dlp_probe(),
            external_tools: Vec::new(),
            browser: None,
        }
    }

    fn sample_candidate(job_id: Uuid) -> MediaCandidate {
        MediaCandidate {
            id: Uuid::new_v4(),
            job_id,
            url: "https://cdn.example.com/a.mp3".to_string(),
            kind: CandidateKind::Audio,
            extractor: "mock".to_string(),
            method: "test".to_string(),
            status: None,
            content_type: None,
            content_length: None,
            resource_type: None,
            initiator_url: None,
            quality_label: None,
            score: 1,
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
            validation_state: Some(CandidateValidationState::Untested),
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

    struct Mock {
        name: &'static str,
        count: usize,
        fail: bool,
    }

    #[async_trait]
    impl SourceExtractor for Mock {
        fn name(&self) -> &'static str {
            self.name
        }
        fn matches(&self, _ctx: &ExtractContext) -> bool {
            true
        }
        async fn extract(&self, ctx: &ExtractContext) -> Result<ExtractResult> {
            if self.fail {
                return Err(RkError::Source("boom".to_string()));
            }
            let candidates = (0..self.count)
                .map(|_| sample_candidate(ctx.job_id))
                .collect();
            Ok(ExtractResult::candidates(candidates))
        }
    }

    #[test]
    fn direct_matches_media_urls_not_pages() {
        assert!(DirectExtractor.matches(&ctx("https://cdn.example.com/song.mp3")));
        assert!(DirectExtractor.matches(&ctx("https://cdn.example.com/stream/master.m3u8?token=x")));
        assert!(
            !DirectExtractor.matches(&ctx("https://www.streetvoice.cn/SpaceStaion/songs/863335/"))
        );
    }

    #[test]
    fn discovery_modes_build_expected_chains() {
        let names = |r: &SourceResolver| r.extractors.iter().map(|e| e.name()).collect::<Vec<_>>();
        assert_eq!(
            names(&SourceResolver::for_discovery(DiscoveryMode::Direct)),
            vec!["direct"]
        );
        assert_eq!(
            names(&SourceResolver::for_discovery(DiscoveryMode::External)),
            vec!["yt_dlp"]
        );
        assert_eq!(
            names(&SourceResolver::for_discovery(DiscoveryMode::Browser)),
            vec!["browser_probe"]
        );
        assert_eq!(
            names(&SourceResolver::for_discovery(DiscoveryMode::Auto)),
            vec![
                "direct",
                "hanime1",
                "mac_cms",
                "generic",
                "yt_dlp",
                "you_get",
                "lux",
                "streamlink",
                "browser_probe"
            ]
        );
    }

    #[tokio::test]
    async fn resolver_picks_first_non_empty_and_records_fallthrough() {
        let resolver = SourceResolver::new(vec![
            Box::new(Mock {
                name: "empty",
                count: 0,
                fail: false,
            }),
            Box::new(Mock {
                name: "boom",
                count: 0,
                fail: true,
            }),
            Box::new(Mock {
                name: "winner",
                count: 2,
                fail: false,
            }),
            Box::new(Mock {
                name: "never",
                count: 9,
                fail: false,
            }),
        ]);

        let outcome = resolver
            .resolve(&ctx_with_discovery(
                "https://example.com/page",
                DiscoveryMode::Direct,
            ))
            .await;

        assert_eq!(outcome.winner.as_deref(), Some("winner"));
        assert_eq!(outcome.candidates.len(), 2);
        // The 4th extractor is never reached once one wins.
        assert_eq!(outcome.chain, vec!["empty", "boom", "winner"]);
        assert_eq!(outcome.attempts.len(), 3);
        assert!(outcome.attempts[1]
            .error
            .as_deref()
            .unwrap()
            .contains("boom"));
        assert_eq!(outcome.attempts[1].error_class, ErrorClass::Blocked);
    }

    #[tokio::test]
    async fn auto_resolver_aggregates_candidates() {
        let resolver = SourceResolver::new(vec![
            Box::new(Mock {
                name: "first",
                count: 1,
                fail: false,
            }),
            Box::new(Mock {
                name: "second",
                count: 1,
                fail: false,
            }),
        ]);

        let outcome = resolver.resolve(&ctx("https://example.com/page")).await;

        assert_eq!(outcome.winner.as_deref(), Some("first"));
        assert_eq!(outcome.chain, vec!["first", "second"]);
        assert_eq!(outcome.candidates.len(), 1);
        assert_eq!(outcome.candidates[0].evidence_count, 2);
    }

    #[tokio::test]
    async fn resolver_reports_empty_when_nothing_matches_or_produces() {
        let resolver = SourceResolver::new(vec![
            Box::new(Mock {
                name: "a",
                count: 0,
                fail: false,
            }),
            Box::new(Mock {
                name: "b",
                count: 0,
                fail: true,
            }),
        ]);
        let outcome = resolver.resolve(&ctx("https://example.com/page")).await;
        assert!(outcome.winner.is_none());
        assert!(outcome.candidates.is_empty());
        assert_eq!(outcome.chain_label(), "a>b");
    }

    #[test]
    fn cloudflare_challenge_errors_are_blocked() {
        let error = RkError::Source(
            "page is blocked by a Cloudflare security challenge; open the job browser login session"
                .to_string(),
        );

        assert_eq!(classify(&error), ErrorClass::Blocked);
    }

    // --- Full-chain live integration tests (network). Run explicitly:
    //   RK_VERIFY_ENABLED=1 cargo test -p reflection-core --release \
    //     chain_live -- --ignored --nocapture
    // Exercises the real discovery+verify path end to end: a public page is
    // scanned by GenericExtractor (og:video / JSON-LD / bare media URLs), the
    // produced candidates are HTTP-probed by the verify stage, and the winner
    // must come back Usable. archive.org/details pages reliably embed og:video
    // + twitter:player pointing at real mp4s (see VERIFICATION-DESIGN.md 8.2).

    async fn run_chain(url: &str) -> ResolveOutcome {
        // Mirror the production Auto chain order (Direct -> Generic -> YtDlp).
        // YtDlpExtractor only fires when ctx.yt_dlp is Some (RK_CHAIN_YTDLP set),
        // so adding it unconditionally is safe: without the env var it no-ops and
        // the chain behaves exactly as the Direct+Generic-only harness did. With
        // it set, the catalog harness can finally exercise the post-generic leg
        // that covers JS-SPA / API-gated sites (Odysee/Vimeo/Dailymotion/...).
        let resolver = SourceResolver::new(vec![
            Box::new(DirectExtractor),
            Box::new(GenericExtractor),
            Box::new(YtDlpExtractor),
        ]);
        resolver.resolve(&ctx(url)).await
    }

    #[tokio::test]
    #[ignore]
    async fn chain_live_archive_details_discovers_and_verifies() {
        for page in [
            "https://archive.org/details/Sintel",
            "https://archive.org/details/BigBuckBunny_328",
            "https://archive.org/details/ElephantsDream",
        ] {
            let outcome = run_chain(page).await;
            let usable = outcome
                .candidates
                .iter()
                .filter(|c| c.validation_state == Some(CandidateValidationState::Usable))
                .count();
            println!(
                "CHAIN {page}: {} candidates, {} Usable, winner={:?}",
                outcome.candidates.len(),
                usable,
                outcome.winner
            );
            for c in outcome.candidates.iter().take(3) {
                println!(
                    "  - {:?} score={} {:?} status={:?} ct={:?} {}",
                    c.validation_state, c.score, c.kind, c.status, c.content_type, c.url
                );
            }
            assert!(
                !outcome.candidates.is_empty(),
                "{page}: generic scan produced no candidates"
            );
            // With RK_VERIFY_ENABLED=1 the top candidate must be probed and
            // confirmed playable; the verified sort floats it to the front.
            if std::env::var("RK_VERIFY_ENABLED").is_ok() {
                assert!(
                    usable >= 1,
                    "{page}: no candidate verified Usable (states: {:?})",
                    outcome
                        .candidates
                        .iter()
                        .map(|c| c.validation_state)
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    outcome.candidates[0].validation_state,
                    Some(CandidateValidationState::Usable),
                    "{page}: top candidate after verified sort is not Usable"
                );
            }
        }
    }

    // Catalog-driven batch harness. Reads newline-delimited page URLs from the
    // file named by RK_CHAIN_CATALOG (# comments + blank lines ignored), runs
    // the full discovery+verify chain over each, and prints per-state aggregate
    // stats so the loop can grow the catalog to 100-200 sites WITHOUT
    // recompiling. Diagnostic, not a pass/fail gate: it never asserts on
    // individual sites (real pages rot / go offline); it only fails if the
    // catalog is unreadable or every single page produced zero candidates
    // (which would signal the chain itself broke). Repo-tracked catalogs live in
    // loop/ (see loop/README.md). The test cwd is the crate dir, not the repo
    // root, so pass an absolute path. Run:
    //   RK_CHAIN_CATALOG=$PWD/loop/catalog-mixed.txt RK_VERIFY_ENABLED=1 \
    //     cargo test -p reflection-core --release chain_live_catalog \
    //     -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn chain_live_catalog() {
        let path = match std::env::var("RK_CHAIN_CATALOG") {
            Ok(p) => p,
            Err(_) => {
                println!("RK_CHAIN_CATALOG not set; skipping catalog batch");
                return;
            }
        };
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read catalog {path}: {e}"));
        let pages: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        assert!(!pages.is_empty(), "catalog {path} has no usable URLs");

        let verify_on = std::env::var("RK_VERIFY_ENABLED").is_ok();
        let mut state_counts: std::collections::BTreeMap<String, usize> = Default::default();
        // Pages whose TOP candidate landed in a non-Usable terminal state, keyed
        // by that state — turns the aggregate breakdown into an actionable list
        // (which sites to inspect for a NeedsProfile/Failed/Drm/etc. verdict).
        let mut pages_by_top_state: std::collections::BTreeMap<String, Vec<&str>> =
            Default::default();
        let mut pages_with_cands = 0usize;
        let mut pages_top_usable = 0usize;
        let mut empty_pages: Vec<&str> = Vec::new();
        // Candidate-level URL lists for the diagnostic terminal states, keyed by
        // state — finer-grained than pages_by_top_state (a page's top may be
        // Usable while a lower candidate is NeedsProfile/Expired).
        let mut diag_candidates: std::collections::BTreeMap<String, Vec<String>> =
            Default::default();

        for (i, page) in pages.iter().enumerate() {
            let outcome = run_chain(page).await;
            let n = outcome.candidates.len();
            if n > 0 {
                pages_with_cands += 1;
            } else {
                empty_pages.push(page);
            }
            for c in &outcome.candidates {
                let key = match c.validation_state {
                    Some(s) => format!("{s:?}"),
                    None => "Untested".to_string(),
                };
                *state_counts.entry(key).or_default() += 1;
                // Capture the actual candidate URL for the diagnostic states so a
                // 401/403->NeedsProfile or an Expired/RegionBlocked verdict can be
                // traced to the exact stream, not just the page it came from.
                // SuspectAd included with its validation_status (the "why") so the
                // soft-block / empty-manifest / unconfirmed-2xx buckets can be
                // told apart and reviewed for recoverable misclassifications.
                if matches!(
                    c.validation_state,
                    Some(CandidateValidationState::NeedsProfile)
                        | Some(CandidateValidationState::Expired)
                        | Some(CandidateValidationState::RegionBlocked)
                        | Some(CandidateValidationState::Failed)
                        | Some(CandidateValidationState::SuspectAd)
                ) {
                    let status = c
                        .status
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "-".into());
                    let ct = c.content_type.as_deref().unwrap_or("-");
                    diag_candidates
                        .entry(format!("{:?}", c.validation_state.unwrap()))
                        .or_default()
                        .push(format!("[{status} {ct}] {} <- {}", c.url, page));
                }
            }
            let top_state = outcome.candidates.first().and_then(|c| c.validation_state);
            let top_usable = top_state == Some(CandidateValidationState::Usable);
            if top_usable {
                pages_top_usable += 1;
            } else if let Some(s) = top_state {
                pages_by_top_state
                    .entry(format!("{s:?}"))
                    .or_default()
                    .push(page);
            }
            println!(
                "[{:>3}/{}] cands={} top={:?} {}",
                i + 1,
                pages.len(),
                n,
                top_state,
                page
            );
        }

        println!(
            "\n=== CATALOG SUMMARY ({} pages, verify={}) ===",
            pages.len(),
            verify_on
        );
        println!(
            "pages with >=1 candidate : {pages_with_cands}/{}",
            pages.len()
        );
        println!(
            "pages top-candidate Usable: {pages_top_usable}/{}",
            pages.len()
        );
        println!("candidate state breakdown : {state_counts:?}");
        // Per-state page lists for every non-Usable top verdict, so a reviewer
        // can jump straight to the sites behind each Drm/Failed/NeedsProfile/...
        for (state, urls) in &pages_by_top_state {
            println!("pages top-candidate {state} ({}):", urls.len());
            for u in urls {
                println!("  - {u}");
            }
        }
        if !empty_pages.is_empty() {
            println!("ZERO-candidate pages ({}):", empty_pages.len());
            for p in &empty_pages {
                println!("  - {p}");
            }
        }
        // Candidate-level URLs behind each diagnostic terminal state (every
        // occurrence, not just page-top), to ground classifier tuning in the
        // exact streams that produced NeedsProfile/Expired/RegionBlocked/Failed.
        for (state, urls) in &diag_candidates {
            println!("candidates {state} ({}):", urls.len());
            for u in urls {
                println!("  * {u}");
            }
        }

        // Only hard-fail if the chain is wholly broken (nothing discovered
        // anywhere). Per-site rot is expected and reported, not asserted.
        assert!(
            pages_with_cands > 0,
            "catalog batch produced ZERO candidates across all {} pages — chain broken?",
            pages.len()
        );
    }
}
