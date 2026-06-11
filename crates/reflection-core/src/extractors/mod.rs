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
mod yt_dlp;

pub use browser::BrowserExtractor;
pub use direct::DirectExtractor;
pub use yt_dlp::YtDlpExtractor;

use async_trait::async_trait;
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

use crate::{
    browser_probe::BrowserProbeClient,
    external_probe::YtDlpProbe,
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
    pub platform_hint: PlatformHint,
    pub auth_mode: AuthMode,
    pub yt_dlp: Option<YtDlpProbe>,
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
}

impl ExtractResult {
    pub fn candidates(candidates: Vec<MediaCandidate>) -> Self {
        Self {
            candidates,
            warnings: Vec::new(),
            browser_session: None,
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
    /// - `External`: yt-dlp only.
    /// - `Browser`: real browser only.
    /// - `Direct`/`Auto`: direct/manifest fast path, then yt-dlp, then browser.
    ///   Dedicated per-site extractors (e.g. StreetVoice) insert ahead of yt-dlp.
    pub fn for_discovery(discovery: DiscoveryMode) -> Self {
        let extractors: Vec<Box<dyn SourceExtractor>> = match discovery {
            DiscoveryMode::External => vec![Box::new(YtDlpExtractor)],
            DiscoveryMode::Browser => vec![Box::new(BrowserExtractor)],
            DiscoveryMode::Direct | DiscoveryMode::Auto => vec![
                Box::new(DirectExtractor),
                Box::new(YtDlpExtractor),
                Box::new(BrowserExtractor),
            ],
        };
        Self::new(extractors)
    }

    /// Run the chain. The first extractor that matches and returns a non-empty
    /// candidate list wins; matched extractors that error or return nothing are
    /// recorded and the chain falls through to the next.
    pub async fn resolve(&self, ctx: &ExtractContext) -> ResolveOutcome {
        let mut outcome = ResolveOutcome::default();

        for extractor in &self.extractors {
            if !extractor.matches(ctx) {
                continue;
            }
            let name = extractor.name().to_string();
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
                        outcome.candidates = result.candidates;
                        outcome.winner = Some(name);
                        break;
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

        outcome
    }
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
    use crate::models::CandidateKind;
    use crate::RkError;

    fn ctx(url: &str) -> ExtractContext {
        ExtractContext {
            job_id: Uuid::new_v4(),
            source_url: url.to_string(),
            url: Url::parse(url).unwrap(),
            outputs: vec![OutputKind::Audio],
            profile_id: "p".to_string(),
            platform_hint: PlatformHint::Auto,
            auth_mode: AuthMode::None,
            yt_dlp: None,
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
            names(&SourceResolver::for_discovery(DiscoveryMode::External)),
            vec!["yt_dlp"]
        );
        assert_eq!(
            names(&SourceResolver::for_discovery(DiscoveryMode::Browser)),
            vec!["browser_probe"]
        );
        assert_eq!(
            names(&SourceResolver::for_discovery(DiscoveryMode::Auto)),
            vec!["direct", "yt_dlp", "browser_probe"]
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

        let outcome = resolver.resolve(&ctx("https://example.com/page")).await;

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
}
