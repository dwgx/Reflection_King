use async_trait::async_trait;

use crate::{external_tools::ExternalToolKind, Result};

use super::{ExtractContext, ExtractResult, SourceExtractor};

pub struct ExternalToolExtractor {
    kind: ExternalToolKind,
}

impl ExternalToolExtractor {
    pub fn new(kind: ExternalToolKind) -> Self {
        Self { kind }
    }
}

#[async_trait]
impl SourceExtractor for ExternalToolExtractor {
    fn name(&self) -> &'static str {
        self.kind.as_str()
    }

    fn matches(&self, ctx: &ExtractContext) -> bool {
        ctx.external_tools
            .iter()
            .any(|probe| probe.kind() == self.kind)
    }

    async fn extract(&self, ctx: &ExtractContext) -> Result<ExtractResult> {
        let Some(probe) = ctx
            .external_tools
            .iter()
            .find(|probe| probe.kind() == self.kind)
        else {
            return Ok(ExtractResult::default());
        };
        let candidates = probe
            .probe(ctx.job_id, &ctx.source_url, &ctx.outputs)
            .await?;
        Ok(ExtractResult::candidates(candidates))
    }
}
