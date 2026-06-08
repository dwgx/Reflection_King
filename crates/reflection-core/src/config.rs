use std::{env, net::SocketAddr, path::PathBuf, time::Duration};

use crate::{Result, RkError};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_address: SocketAddr,
    pub public_base_url: String,
    pub storage_dir: PathBuf,
    pub max_download_bytes: u64,
    pub download_timeout: Duration,
    pub job_ttl_hours: u64,
    pub max_concurrent_jobs: usize,
    pub ffmpeg_path: PathBuf,
    pub browser_probe_url: Option<String>,
    pub browser_probe_timeout: Duration,
    pub api_key: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let bind_address = env_value("RK_BIND_ADDRESS", "0.0.0.0:8787")
            .parse()
            .map_err(|error| RkError::BadRequest(format!("invalid RK_BIND_ADDRESS: {error}")))?;

        let public_base_url = env_value("RK_PUBLIC_BASE_URL", "http://localhost:8787")
            .trim_end_matches('/')
            .to_string();

        let storage_dir = PathBuf::from(env_value("RK_STORAGE_DIR", "storage"));
        let max_download_mb = env_value("RK_MAX_DOWNLOAD_MB", "300")
            .parse::<u64>()
            .map_err(|error| RkError::BadRequest(format!("invalid RK_MAX_DOWNLOAD_MB: {error}")))?;
        let download_timeout_secs = env_value("RK_DOWNLOAD_TIMEOUT_SECONDS", "300")
            .parse::<u64>()
            .map_err(|error| {
                RkError::BadRequest(format!("invalid RK_DOWNLOAD_TIMEOUT_SECONDS: {error}"))
            })?;
        let job_ttl_hours = env_value("RK_JOB_TTL_HOURS", "24")
            .parse::<u64>()
            .map_err(|error| RkError::BadRequest(format!("invalid RK_JOB_TTL_HOURS: {error}")))?;
        let max_concurrent_jobs = env_value("RK_MAX_CONCURRENT_JOBS", "2")
            .parse::<usize>()
            .map_err(|error| {
                RkError::BadRequest(format!("invalid RK_MAX_CONCURRENT_JOBS: {error}"))
            })?
            .max(1);
        let ffmpeg_path = PathBuf::from(env_value("RK_FFMPEG_PATH", "ffmpeg"));
        let browser_probe_url = env::var("RK_BROWSER_PROBE_URL")
            .ok()
            .filter(|value| !value.is_empty())
            .map(|value| value.trim_end_matches('/').to_string());
        let browser_probe_timeout_secs = env_value("RK_BROWSER_PROBE_TIMEOUT_SECONDS", "90")
            .parse::<u64>()
            .map_err(|error| {
                RkError::BadRequest(format!("invalid RK_BROWSER_PROBE_TIMEOUT_SECONDS: {error}"))
            })?;
        let api_key = env::var("RK_API_KEY")
            .ok()
            .filter(|value| !value.is_empty());

        Ok(Self {
            bind_address,
            public_base_url,
            storage_dir,
            max_download_bytes: max_download_mb.saturating_mul(1024).saturating_mul(1024),
            download_timeout: Duration::from_secs(download_timeout_secs),
            job_ttl_hours,
            max_concurrent_jobs,
            ffmpeg_path,
            browser_probe_url,
            browser_probe_timeout: Duration::from_secs(browser_probe_timeout_secs),
            api_key,
        })
    }
}

fn env_value(key: &str, default_value: &str) -> String {
    env::var(key).unwrap_or_else(|_| default_value.to_string())
}
