//! Real-browser adapter.
//!
//! Universal fallback: drives the Playwright sidecar to load the page, observe
//! the requests the page's own JavaScript makes, and return the media URLs it
//! resolved. Handles JS-built / signed URLs and login-walled authorized content
//! via the persistent profile. Records a `browser_sessions` row for the trace.

use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{observability::BrowserSession, Result};

use super::{ExtractContext, ExtractResult, SourceExtractor};

pub struct BrowserExtractor;

#[async_trait]
impl SourceExtractor for BrowserExtractor {
    fn name(&self) -> &'static str {
        "browser_probe"
    }

    fn matches(&self, ctx: &ExtractContext) -> bool {
        ctx.browser.is_some()
    }

    async fn extract(&self, ctx: &ExtractContext) -> Result<ExtractResult> {
        let Some(browser) = &ctx.browser else {
            return Ok(ExtractResult::default());
        };

        let started_at = OffsetDateTime::now_utc();
        let outcome = browser
            .probe_session(
                ctx.job_id,
                &ctx.source_url,
                &ctx.profile_id,
                ctx.platform_hint,
                &ctx.output_names(),
            )
            .await?;

        let session = BrowserSession {
            id: Uuid::new_v4(),
            job_id: ctx.job_id,
            profile_id: ctx.profile_id.clone(),
            user_agent: outcome.user_agent.clone(),
            viewport: None,
            locale: None,
            timezone: None,
            headed: false,
            final_url: Some(outcome.final_url.clone()),
            page_title: outcome.title.clone(),
            event_count: outcome.event_count as i64,
            candidate_count: outcome.candidates.len() as i64,
            playback_triggered: outcome.playback_triggered,
            timed_out: outcome.timed_out,
            warnings_json: serde_json::json!(outcome.warnings),
            console_errors_json: serde_json::json!(outcome.console_errors),
            screenshot_path: None,
            started_at,
            ended_at: Some(OffsetDateTime::now_utc()),
        };

        Ok(ExtractResult {
            candidates: outcome.candidates,
            warnings: outcome.warnings,
            browser_session: Some(session),
            page_snapshot: outcome.page_snapshot,
        })
    }
}
