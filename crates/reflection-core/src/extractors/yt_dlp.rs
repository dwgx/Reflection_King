//! yt-dlp adapter.
//!
//! Wraps the constrained `yt-dlp --dump-single-json` probe. This is the broad
//! coverage tier: it reuses the community's 1800+ site extractors instead of
//! hand-writing each site.

use async_trait::async_trait;

use crate::{models::AuthMode, Result};

use super::{ExtractContext, ExtractResult, SourceExtractor};

pub struct YtDlpExtractor;

#[async_trait]
impl SourceExtractor for YtDlpExtractor {
    fn name(&self) -> &'static str {
        "yt_dlp"
    }

    fn matches(&self, ctx: &ExtractContext) -> bool {
        // yt-dlp can attempt almost any http(s) page; applicable whenever it is
        // configured.
        ctx.yt_dlp.is_some()
    }

    async fn extract(&self, ctx: &ExtractContext) -> Result<ExtractResult> {
        let Some(probe) = &ctx.yt_dlp else {
            return Ok(ExtractResult::default());
        };
        let headers = if should_use_profile_headers(ctx.auth_mode) {
            if let Some(browser) = &ctx.browser {
                browser
                    .headers_for_url(&ctx.profile_id, &ctx.source_url, Some(&ctx.source_url))
                    .await
                    .unwrap_or_default()
            } else {
                Default::default()
            }
        } else {
            Default::default()
        };
        let candidates = probe
            .probe_with_headers(ctx.job_id, &ctx.source_url, &ctx.outputs, &headers)
            .await?;
        Ok(ExtractResult::candidates(candidates))
    }
}

fn should_use_profile_headers(auth_mode: AuthMode) -> bool {
    matches!(
        auth_mode,
        AuthMode::Auto | AuthMode::Profile | AuthMode::Cookies
    )
}
