use reqwest::{
    header::{HeaderMap, CONTENT_TYPE},
    Client, StatusCode,
};
use url::Url;

use crate::{
    url_policy::{parse_and_validate_url, validate_url},
    Result, RkError,
};

const MAX_MANIFEST_BYTES: usize = 512 * 1024;
const MAX_HLS_CHILD_URLS: usize = 512;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestKind {
    Hls,
    Dash,
}

pub async fn validate_manifest_url(
    client: &Client,
    manifest_url: &str,
    headers: HeaderMap,
) -> Result<()> {
    let kind = manifest_kind(manifest_url)
        .ok_or_else(|| RkError::Source("unsupported manifest URL".to_string()))?;
    match kind {
        ManifestKind::Hls => validate_hls_manifest(client, manifest_url, headers).await,
        ManifestKind::Dash => Err(RkError::Source(
            "DASH manifest acquisition is blocked until MPD child URL policy validation is implemented"
                .to_string(),
        )),
    }
}

async fn validate_hls_manifest(
    client: &Client,
    manifest_url: &str,
    headers: HeaderMap,
) -> Result<()> {
    let base_url = parse_and_validate_url(manifest_url)?;
    let body = fetch_manifest_text(client, base_url.clone(), headers).await?;
    if !body.trim_start().starts_with("#EXTM3U") {
        return Err(RkError::Source("HLS manifest missing #EXTM3U".to_string()));
    }

    let child_urls = hls_child_urls(&base_url, &body)?;
    for child in &child_urls {
        validate_url(child)?;
    }

    Ok(())
}

async fn fetch_manifest_text(
    client: &Client,
    start_url: Url,
    headers: HeaderMap,
) -> Result<String> {
    let mut url = start_url;
    for redirect_count in 0..=MAX_REDIRECTS {
        validate_url(&url)?;
        let response = client
            .get(url.clone())
            .headers(headers.clone())
            .send()
            .await?;
        let status = response.status();

        if status.is_redirection() {
            if redirect_count == MAX_REDIRECTS {
                return Err(RkError::Source(
                    "manifest had too many redirects".to_string(),
                ));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| RkError::Source("manifest redirect without Location".to_string()))?
                .to_str()
                .map_err(|error| RkError::Source(format!("invalid manifest redirect: {error}")))?;
            url = url.join(location)?;
            continue;
        }

        if status != StatusCode::OK {
            return Err(RkError::Source(format!("manifest returned HTTP {status}")));
        }

        if let Some(length) = response.content_length() {
            if length as usize > MAX_MANIFEST_BYTES {
                return Err(RkError::Source(format!(
                    "manifest exceeds {MAX_MANIFEST_BYTES} bytes"
                )));
            }
        }

        if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
            let content_type = content_type
                .to_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if content_type.contains("text/html") {
                return Err(RkError::Source(
                    "manifest URL returned HTML instead of playlist text".to_string(),
                ));
            }
        }

        let bytes = response.bytes().await?;
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(RkError::Source(format!(
                "manifest exceeds {MAX_MANIFEST_BYTES} bytes"
            )));
        }
        return String::from_utf8(bytes.to_vec())
            .map_err(|error| RkError::Source(format!("manifest is not UTF-8 text: {error}")));
    }

    Err(RkError::Source(
        "manifest had too many redirects".to_string(),
    ))
}

fn hls_child_urls(base_url: &Url, body: &str) -> Result<Vec<Url>> {
    let mut urls = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(value) = line.strip_prefix("#EXT-X-KEY:") {
            if let Some(uri) = hls_attribute(value, "URI") {
                push_child_url(base_url, &uri, &mut urls)?;
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-MAP:") {
            if let Some(uri) = hls_attribute(value, "URI") {
                push_child_url(base_url, &uri, &mut urls)?;
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        push_child_url(base_url, line, &mut urls)?;
    }
    Ok(urls)
}

fn push_child_url(base_url: &Url, raw: &str, urls: &mut Vec<Url>) -> Result<()> {
    if urls.len() >= MAX_HLS_CHILD_URLS {
        return Err(RkError::Source(format!(
            "HLS manifest references more than {MAX_HLS_CHILD_URLS} child URLs"
        )));
    }
    let child = base_url.join(raw.trim())?;
    validate_url(&child)?;
    urls.push(child);
    Ok(())
}

fn hls_attribute(value: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = value.find(&needle)? + needle.len();
    let rest = &value[start..];
    if let Some(rest) = rest.strip_prefix('"') {
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    } else {
        let end = rest.find(',').unwrap_or(rest.len());
        Some(rest[..end].trim().to_string())
    }
}

fn manifest_kind(url: &str) -> Option<ManifestKind> {
    let without_query = url.split(['?', '#']).next().unwrap_or(url);
    if without_query.to_ascii_lowercase().ends_with(".m3u8") {
        Some(ManifestKind::Hls)
    } else if without_query.to_ascii_lowercase().ends_with(".mpd") {
        Some(ManifestKind::Dash)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hls_child_urls_resolve_relative_and_key_uris() {
        let base = Url::parse("https://example.com/path/master.m3u8").unwrap();
        let body = r#"#EXTM3U
#EXT-X-KEY:METHOD=AES-128,URI="keys/key.bin"
#EXT-X-MAP:URI="init.mp4"
#EXTINF:4,
seg-1.ts
#EXTINF:4,
../seg-2.ts?token=abc
"#;

        let urls = hls_child_urls(&base, body).unwrap();

        assert_eq!(urls.len(), 4);
        assert_eq!(urls[0].as_str(), "https://example.com/path/keys/key.bin");
        assert_eq!(urls[1].as_str(), "https://example.com/path/init.mp4");
        assert_eq!(urls[2].as_str(), "https://example.com/path/seg-1.ts");
        assert_eq!(urls[3].as_str(), "https://example.com/seg-2.ts?token=abc");
    }

    #[test]
    fn hls_child_urls_reject_private_children() {
        let base = Url::parse("https://example.com/master.m3u8").unwrap();
        let error = hls_child_urls(&base, "http://127.0.0.1/private.ts").unwrap_err();

        assert!(error.to_string().contains("URL policy denied request"));
    }
}
