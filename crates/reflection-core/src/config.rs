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
    pub browser_internal_token: Option<String>,
    pub browser_default_profile_id: String,
    pub browser_probe_timeout: Duration,
    pub yt_dlp_path: Option<PathBuf>,
    pub yt_dlp_timeout: Duration,
    pub yt_dlp_max_json_bytes: usize,
    pub you_get_path: Option<PathBuf>,
    pub lux_path: Option<PathBuf>,
    pub streamlink_path: Option<PathBuf>,
    pub external_probe_timeout: Duration,
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
        let browser_internal_token = env::var("RK_BROWSER_INTERNAL_TOKEN")
            .ok()
            .filter(|value| !value.is_empty());
        let browser_default_profile_id =
            sanitize_profile_id(&env_value("RK_BROWSER_DEFAULT_PROFILE", "admin_default"));
        let browser_probe_timeout_secs = env_value("RK_BROWSER_PROBE_TIMEOUT_SECONDS", "90")
            .parse::<u64>()
            .map_err(|error| {
                RkError::BadRequest(format!("invalid RK_BROWSER_PROBE_TIMEOUT_SECONDS: {error}"))
            })?;
        let yt_dlp_path = env::var("RK_YTDLP_PATH")
            .ok()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let yt_dlp_timeout_secs = env_value("RK_YTDLP_TIMEOUT_SECONDS", "45")
            .parse::<u64>()
            .map_err(|error| {
                RkError::BadRequest(format!("invalid RK_YTDLP_TIMEOUT_SECONDS: {error}"))
            })?;
        let yt_dlp_max_json_mb = env_value("RK_YTDLP_MAX_JSON_MB", "8")
            .parse::<usize>()
            .map_err(|error| {
                RkError::BadRequest(format!("invalid RK_YTDLP_MAX_JSON_MB: {error}"))
            })?;
        let api_key = env::var("RK_API_KEY")
            .ok()
            .filter(|value| !value.is_empty());
        let you_get_path = env::var("RK_YOU_GET_PATH")
            .ok()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let lux_path = env::var("RK_LUX_PATH")
            .ok()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let streamlink_path = env::var("RK_STREAMLINK_PATH")
            .ok()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let external_probe_timeout_secs = env_value("RK_EXTERNAL_PROBE_TIMEOUT_SECONDS", "45")
            .parse::<u64>()
            .map_err(|error| {
                RkError::BadRequest(format!(
                    "invalid RK_EXTERNAL_PROBE_TIMEOUT_SECONDS: {error}"
                ))
            })?;

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
            browser_internal_token,
            browser_default_profile_id,
            browser_probe_timeout: Duration::from_secs(browser_probe_timeout_secs),
            yt_dlp_path,
            yt_dlp_timeout: Duration::from_secs(yt_dlp_timeout_secs),
            yt_dlp_max_json_bytes: yt_dlp_max_json_mb.saturating_mul(1024).saturating_mul(1024),
            you_get_path,
            lux_path,
            streamlink_path,
            external_probe_timeout: Duration::from_secs(external_probe_timeout_secs),
            api_key,
        })
    }
}

fn env_value(key: &str, default_value: &str) -> String {
    env::var(key).unwrap_or_else(|_| default_value.to_string())
}

fn sanitize_profile_id(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .take(64)
        .collect();
    if sanitized.is_empty() {
        "admin_default".to_string()
    } else {
        sanitized
    }
}
