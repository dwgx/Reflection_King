use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use reqwest::header::{HeaderMap, REFERER, USER_AGENT};
use tokio::process::Command;

use crate::{Result, RkError};

#[derive(Debug, Clone)]
pub struct Transcoder {
    ffmpeg_path: PathBuf,
}

impl Transcoder {
    pub fn new(ffmpeg_path: impl Into<PathBuf>) -> Self {
        Self {
            ffmpeg_path: ffmpeg_path.into(),
        }
    }

    pub async fn audio_to_mp3(&self, input: &Path, output: &Path, bitrate: &str) -> Result<()> {
        self.audio_input_to_mp3(input.as_os_str(), output, bitrate, &[])
            .await
    }

    pub async fn media_url_to_mp3_with_headers(
        &self,
        input_url: &str,
        output: &Path,
        bitrate: &str,
        headers: &HeaderMap,
    ) -> Result<()> {
        let input_args = ffmpeg_http_args(headers);
        self.audio_input_to_mp3(input_url, output, bitrate, &input_args)
            .await
    }

    pub async fn media_to_mp4(&self, input: &Path, output: &Path) -> Result<()> {
        self.media_input_to_mp4(input.as_os_str(), output, &[])
            .await
    }

    pub async fn media_url_to_mp4(&self, input_url: &str, output: &Path) -> Result<()> {
        self.media_url_to_mp4_with_headers(input_url, output, &HeaderMap::new())
            .await
    }

    pub async fn media_url_to_mp4_with_headers(
        &self,
        input_url: &str,
        output: &Path,
        headers: &HeaderMap,
    ) -> Result<()> {
        let input_args = ffmpeg_http_args(headers);
        self.media_input_to_mp4(input_url, output, &input_args)
            .await
    }

    pub async fn media_urls_to_mp4_with_headers(
        &self,
        video_url: &str,
        video_headers: &HeaderMap,
        audio_url: &str,
        audio_headers: &HeaderMap,
        output: &Path,
    ) -> Result<()> {
        let video_args = ffmpeg_http_args(video_headers);
        let audio_args = ffmpeg_http_args(audio_headers);
        self.media_inputs_to_mp4(video_url, &video_args, audio_url, &audio_args, output)
            .await
    }

    async fn audio_input_to_mp3(
        &self,
        input: impl AsRef<std::ffi::OsStr>,
        output: &Path,
        bitrate: &str,
        input_args: &[OsString],
    ) -> Result<()> {
        let bitrate = normalize_audio_bitrate(bitrate);
        let mut command = Command::new(&self.ffmpeg_path);
        command
            .arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error");

        for arg in input_args {
            command.arg(arg);
        }

        let output = command
            .arg("-i")
            .arg(input)
            .arg("-vn")
            .arg("-map")
            .arg("0:a:0")
            .arg("-map_metadata")
            .arg("-1")
            .arg("-codec:a")
            .arg("libmp3lame")
            .arg("-b:a")
            .arg(bitrate)
            .arg("-ar")
            .arg("44100")
            .arg("-ac")
            .arg("2")
            .arg(output)
            .output()
            .await?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(RkError::Transcode(if stderr.is_empty() {
            "ffmpeg exited with failure".to_string()
        } else {
            stderr
        }))
    }

    async fn media_input_to_mp4(
        &self,
        input: impl AsRef<std::ffi::OsStr>,
        output: &Path,
        input_args: &[OsString],
    ) -> Result<()> {
        let mut command = Command::new(&self.ffmpeg_path);
        command
            .arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error");

        for arg in input_args {
            command.arg(arg);
        }

        let output = command
            .arg("-i")
            .arg(input)
            .arg("-map")
            .arg("0:v:0?")
            .arg("-map")
            .arg("0:a:0?")
            .arg("-c:v")
            .arg("libx264")
            .arg("-preset")
            .arg("veryfast")
            .arg("-crf")
            .arg("23")
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("160k")
            .arg("-movflags")
            .arg("+faststart")
            .arg(output)
            .output()
            .await?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(RkError::Transcode(if stderr.is_empty() {
            "ffmpeg exited with failure".to_string()
        } else {
            stderr
        }))
    }

    async fn media_inputs_to_mp4(
        &self,
        video_input: impl AsRef<std::ffi::OsStr>,
        video_input_args: &[OsString],
        audio_input: impl AsRef<std::ffi::OsStr>,
        audio_input_args: &[OsString],
        output: &Path,
    ) -> Result<()> {
        let mut command = Command::new(&self.ffmpeg_path);
        command
            .arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error");

        for arg in video_input_args {
            command.arg(arg);
        }
        command.arg("-i").arg(video_input);

        for arg in audio_input_args {
            command.arg(arg);
        }
        command.arg("-i").arg(audio_input);

        let output = command
            .arg("-map")
            .arg("0:v:0")
            .arg("-map")
            .arg("1:a:0")
            .arg("-c:v")
            .arg("libx264")
            .arg("-preset")
            .arg("veryfast")
            .arg("-crf")
            .arg("23")
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("160k")
            .arg("-shortest")
            .arg("-movflags")
            .arg("+faststart")
            .arg(output)
            .output()
            .await?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(RkError::Transcode(if stderr.is_empty() {
            "ffmpeg exited with failure".to_string()
        } else {
            stderr
        }))
    }
}

fn normalize_audio_bitrate(value: &str) -> &str {
    if value == "auto" {
        "192k"
    } else {
        value
    }
}

fn ffmpeg_http_args(headers: &HeaderMap) -> Vec<OsString> {
    let mut args = Vec::new();

    if let Some(value) = header_value(headers, USER_AGENT) {
        args.push(OsString::from("-user_agent"));
        args.push(OsString::from(value));
    }

    if let Some(value) = header_value(headers, REFERER) {
        args.push(OsString::from("-referer"));
        args.push(OsString::from(value));
    }

    let header_blob = headers
        .iter()
        .filter(|(name, _)| {
            name.as_str() != USER_AGENT.as_str() && name.as_str() != REFERER.as_str()
        })
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| format!("{}: {}\r\n", name.as_str(), value))
        })
        .collect::<String>();

    if !header_blob.is_empty() {
        args.push(OsString::from("-headers"));
        args.push(OsString::from(header_blob));
    }

    args
}

fn header_value(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
}
