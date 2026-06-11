//! yt-dlp adapter.
//!
//! Wraps the constrained `yt-dlp --dump-single-json` probe. This is the broad
//! coverage tier: it reuses the community's 1800+ site extractors instead of
//! hand-writing each site.

use async_trait::async_trait;

use crate::Result;

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
        let candidates = probe
            .probe(ctx.job_id, &ctx.source_url, &ctx.outputs)
            .await?;
        Ok(ExtractResult::candidates(candidates))
    }
}
