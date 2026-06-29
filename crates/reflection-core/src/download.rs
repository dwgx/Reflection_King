use std::{path::Path, time::Duration};

use futures_util::StreamExt;
use reqwest::{
    header::{HeaderMap, HeaderValue, CONTENT_TYPE, REFERER, USER_AGENT},
    Client, StatusCode, Url,
};
use tokio::{fs::File, io::AsyncWriteExt};

use crate::{
    policy_http::{policy_client, validate_response_url_and_peer},
    url_policy::{parse_and_validate_url, validate_url},
    Result, RkError,
};

const MAX_REDIRECTS: usize = 5;

/// Default browser-like User-Agent. Many media/CDN hosts (e.g. Wikimedia
/// `upload.*`) reject requests with no/obviously non-browser User-Agent with
/// HTTP 403, so we present a common desktop UA unless the caller overrides it.
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

#[derive(Debug, Clone)]
pub struct Downloader {
    client: Client,
    max_bytes: u64,
}

impl Downloader {
    pub fn new(timeout: Duration, max_bytes: u64) -> Result<Self> {
        let client = policy_client(timeout, BROWSER_USER_AGENT)?;

        Ok(Self { client, max_bytes })
    }

    pub async fn download_to_file(&self, source_url: &str, output_path: &Path) -> Result<()> {
        self.download_to_file_with_headers(source_url, output_path, HeaderMap::new())
            .await
    }

    pub async fn download_to_file_with_headers(
        &self,
        source_url: &str,
        output_path: &Path,
        mut headers: HeaderMap,
    ) -> Result<()> {
        let mut url = parse_and_validate_url(source_url)?;
        apply_default_request_headers(&url, &mut headers);

        for redirect_count in 0..=MAX_REDIRECTS {
            validate_url(&url)?;

            let response = self
                .client
                .get(url.clone())
                .headers(headers.clone())
                .send()
                .await?;
            validate_response_url_and_peer(&response)?;
            let status = response.status();

            if status.is_redirection() {
                if redirect_count == MAX_REDIRECTS {
                    return Err(RkError::Source("too many redirects".to_string()));
                }

                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .ok_or_else(|| RkError::Source("redirect without Location".to_string()))?
                    .to_str()
                    .map_err(|error| RkError::Source(format!("invalid redirect: {error}")))?;
                url = url.join(location)?;
                continue;
            }

            if status != StatusCode::OK {
                return Err(RkError::Source(format!("remote returned HTTP {status}")));
            }

            if let Some(length) = response.content_length() {
                if length > self.max_bytes {
                    return Err(RkError::DownloadTooLarge {
                        max_bytes: self.max_bytes,
                    });
                }
            }

            if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
                if content_type
                    .to_str()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains("text/html")
                {
                    return Err(RkError::Source(
                        "source returned HTML; use a direct media URL".to_string(),
                    ));
                }
            }

            return self.stream_response_to_file(response, output_path).await;
        }

        Err(RkError::Source("too many redirects".to_string()))
    }

    async fn stream_response_to_file(
        &self,
        response: reqwest::Response,
        path: &Path,
    ) -> Result<()> {
        let mut file = File::create(path).await?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > self.max_bytes {
                return Err(RkError::DownloadTooLarge {
                    max_bytes: self.max_bytes,
                });
            }
            file.write_all(&chunk).await?;
        }

        file.flush().await?;
        Ok(())
    }
}

/// Fill in a browser-like `User-Agent` and same-origin `Referer` when the
/// caller has not set them. Hotlink-protected hosts commonly reject requests
/// that lack these headers. Caller-supplied headers always win.
fn apply_default_request_headers(url: &Url, headers: &mut HeaderMap) {
    if !headers.contains_key(USER_AGENT) {
        headers.insert(USER_AGENT, HeaderValue::from_static(BROWSER_USER_AGENT));
    }

    if !headers.contains_key(REFERER) {
        if let Some(referer) = same_origin_referer(url) {
            if let Ok(value) = HeaderValue::from_str(&referer) {
                headers.insert(REFERER, value);
            }
        }
    }
}

/// Build a `scheme://host[:port]/` origin string for use as a same-origin
/// Referer. Returns `None` for URLs without a host.
fn same_origin_referer(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    let scheme = url.scheme();
    match url.port() {
        Some(port) => Some(format!("{scheme}://{host}:{port}/")),
        None => Some(format!("{scheme}://{host}/")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_browser_ua_and_referer_when_absent() {
        let url = Url::parse("https://upload.wikimedia.org/wikipedia/commons/a/b.jpg").unwrap();
        let mut headers = HeaderMap::new();
        apply_default_request_headers(&url, &mut headers);
        assert_eq!(
            headers.get(USER_AGENT).unwrap(),
            HeaderValue::from_static(BROWSER_USER_AGENT)
        );
        assert_eq!(
            headers.get(REFERER).unwrap(),
            "https://upload.wikimedia.org/"
        );
    }

    #[test]
    fn preserves_caller_supplied_headers() {
        let url = Url::parse("https://example.com/v.mp4").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("custom-agent/9"));
        headers.insert(REFERER, HeaderValue::from_static("https://ref.example/"));
        apply_default_request_headers(&url, &mut headers);
        assert_eq!(headers.get(USER_AGENT).unwrap(), "custom-agent/9");
        assert_eq!(headers.get(REFERER).unwrap(), "https://ref.example/");
    }
}
