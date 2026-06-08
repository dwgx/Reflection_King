use std::{path::Path, time::Duration};

use futures_util::StreamExt;
use reqwest::{
    header::{HeaderMap, CONTENT_TYPE},
    Client, StatusCode,
};
use tokio::{fs::File, io::AsyncWriteExt};

use crate::{
    url_policy::{parse_and_validate_url, validate_url},
    Result, RkError,
};

const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone)]
pub struct Downloader {
    client: Client,
    max_bytes: u64,
}

impl Downloader {
    pub fn new(timeout: Duration, max_bytes: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("ReflectionKing/0.1")
            .build()?;

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
        headers: HeaderMap,
    ) -> Result<()> {
        let mut url = parse_and_validate_url(source_url)?;

        for redirect_count in 0..=MAX_REDIRECTS {
            validate_url(&url)?;

            let response = self
                .client
                .get(url.clone())
                .headers(headers.clone())
                .send()
                .await?;
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
