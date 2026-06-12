//! yt-dlp adapter.
//!
//! Wraps the constrained `yt-dlp --dump-single-json` probe. This is the broad
//! coverage tier: it reuses the community's 1800+ site extractors instead of
//! hand-writing each site.

use async_trait::async_trait;

use crate::{browser_probe::BrowserCookie, models::AuthMode, Result};

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
        let (headers, cookies_file) = if should_use_profile_headers(ctx.auth_mode) {
            if let Some(browser) = &ctx.browser {
                let headers = browser
                    .headers_for_url(&ctx.profile_id, &ctx.source_url, Some(&ctx.source_url))
                    .await
                    .unwrap_or_default();
                let cookies = browser
                    .cookies_for_url(&ctx.profile_id, &ctx.source_url)
                    .await
                    .unwrap_or_default();
                (
                    headers,
                    write_temp_cookies_file(ctx.job_id, &cookies).await?,
                )
            } else {
                (Default::default(), None)
            }
        } else {
            (Default::default(), None)
        };
        let result = probe
            .probe_with_headers_and_cookies_file(
                ctx.job_id,
                &ctx.source_url,
                &ctx.outputs,
                &headers,
                cookies_file.as_deref(),
            )
            .await;
        if let Some(path) = cookies_file {
            tokio::fs::remove_file(path).await.ok();
        }
        let candidates = result?;
        Ok(ExtractResult::candidates(candidates))
    }
}

fn should_use_profile_headers(auth_mode: AuthMode) -> bool {
    matches!(
        auth_mode,
        AuthMode::Auto | AuthMode::Profile | AuthMode::Cookies
    )
}

async fn write_temp_cookies_file(
    job_id: uuid::Uuid,
    cookies: &[BrowserCookie],
) -> Result<Option<std::path::PathBuf>> {
    if cookies.is_empty() {
        return Ok(None);
    }
    let path = std::env::temp_dir().join(format!("reflection-king-{job_id}.cookies.txt"));
    tokio::fs::write(&path, netscape_cookie_file(cookies)).await?;
    Ok(Some(path))
}

fn netscape_cookie_file(cookies: &[BrowserCookie]) -> String {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for cookie in cookies {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            cookie.domain,
            if cookie.domain.starts_with('.') {
                "TRUE"
            } else {
                "FALSE"
            },
            cookie.path,
            if cookie.secure { "TRUE" } else { "FALSE" },
            cookie_expires(cookie.expires),
            sanitize_cookie_field(&cookie.name),
            sanitize_cookie_field(&cookie.value),
        ));
    }
    out
}

fn cookie_expires(value: f64) -> i64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.floor() as i64
    }
}

fn sanitize_cookie_field(value: &str) -> String {
    value.replace(['\t', '\r', '\n'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netscape_cookie_file_sanitizes_values() {
        let cookies = vec![BrowserCookie {
            name: "SESS\tDATA".to_string(),
            value: "abc\r\n123".to_string(),
            domain: ".douyin.com".to_string(),
            path: "/".to_string(),
            expires: 1_781_234_567.9,
            http_only: true,
            secure: true,
        }];

        let file = netscape_cookie_file(&cookies);

        assert!(file.starts_with("# Netscape HTTP Cookie File\n"));
        assert!(file.contains(".douyin.com\tTRUE\t/\tTRUE\t1781234567\tSESSDATA\tabc123\n"));
        assert!(!file.contains('\r'));
    }
}
