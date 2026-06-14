use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    time::Duration,
};

use reqwest::header::{HeaderMap, REFERER, USER_AGENT};
use tokio::{process::Command, time as tokio_time};

use crate::{Result, RkError};

#[derive(Debug, Clone)]
pub struct Transcoder {
    ffmpeg_path: PathBuf,
    timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaStreamInfo {
    pub has_audio: bool,
    pub has_video: bool,
}

impl Transcoder {
    pub fn new(ffmpeg_path: impl Into<PathBuf>) -> Self {
        Self {
            ffmpeg_path: ffmpeg_path.into(),
            timeout: Duration::from_secs(300),
        }
    }

    pub fn with_timeout(ffmpeg_path: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            ffmpeg_path: ffmpeg_path.into(),
            timeout,
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

    pub async fn probe_media(&self, input: &Path) -> Result<MediaStreamInfo> {
        self.probe_input(input.as_os_str(), &[]).await
    }

    pub async fn media_url_to_mp4(&self, input_url: &str, output: &Path) -> Result<()> {
        self.media_url_to_mp4_with_headers(input_url, output, &HeaderMap::new())
            .await
    }

    pub async fn probe_url_with_headers(
        &self,
        input_url: &str,
        headers: &HeaderMap,
    ) -> Result<MediaStreamInfo> {
        let input_args = ffmpeg_http_args(headers);
        self.probe_input(input_url, &input_args).await
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

    pub async fn images_to_mp4(
        &self,
        concat_file: &Path,
        output: &Path,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let mut command = Command::new(&self.ffmpeg_path);
        let video_filter = format!(
            "fps=30,scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p"
        );
        let output = run_process_with_timeout(
            command
                .arg("-y")
                .arg("-hide_banner")
                .arg("-loglevel")
                .arg("error")
                .arg("-f")
                .arg("concat")
                .arg("-safe")
                .arg("0")
                .arg("-i")
                .arg(concat_file)
                .arg("-vf")
                .arg(video_filter)
                .arg("-c:v")
                .arg("libx264")
                .arg("-preset")
                .arg("veryfast")
                .arg("-crf")
                .arg("23")
                .arg("-movflags")
                .arg("+faststart")
                .arg("-an")
                .arg(output),
            self.timeout,
            "ffmpeg image slideshow",
        )
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

        let output = run_process_with_timeout(
            command
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
                .arg(output),
            self.timeout,
            "ffmpeg audio transcode",
        )
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
        let input = input.as_ref();
        if self
            .media_input_to_mp4_copy(input, output, input_args)
            .await
            .is_ok()
        {
            return Ok(());
        }
        tokio::fs::remove_file(output).await.ok();
        self.media_input_to_mp4_transcode(input, output, input_args)
            .await
    }

    async fn media_input_to_mp4_copy(
        &self,
        input: &OsStr,
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

        let output = run_process_with_timeout(
            command
                .arg("-i")
                .arg(input)
                .arg("-map")
                .arg("0:v:0?")
                .arg("-map")
                .arg("0:a:0?")
                .arg("-c")
                .arg("copy")
                .arg("-movflags")
                .arg("+faststart")
                .arg(output),
            self.timeout,
            "ffmpeg copy remux",
        )
        .await?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(RkError::Transcode(if stderr.is_empty() {
            "ffmpeg copy remux exited with failure".to_string()
        } else {
            stderr
        }))
    }

    async fn media_input_to_mp4_transcode(
        &self,
        input: &OsStr,
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

        let output = run_process_with_timeout(
            command
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
                .arg(output),
            self.timeout,
            "ffmpeg video transcode",
        )
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
        let video_input = video_input.as_ref();
        let audio_input = audio_input.as_ref();
        if self
            .media_inputs_to_mp4_copy(
                video_input,
                video_input_args,
                audio_input,
                audio_input_args,
                output,
            )
            .await
            .is_ok()
        {
            return Ok(());
        }
        tokio::fs::remove_file(output).await.ok();
        self.media_inputs_to_mp4_transcode(
            video_input,
            video_input_args,
            audio_input,
            audio_input_args,
            output,
        )
        .await
    }

    async fn media_inputs_to_mp4_copy(
        &self,
        video_input: &OsStr,
        video_input_args: &[OsString],
        audio_input: &OsStr,
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

        let output = run_process_with_timeout(
            command
                .arg("-map")
                .arg("0:v:0")
                .arg("-map")
                .arg("1:a:0")
                .arg("-c")
                .arg("copy")
                .arg("-shortest")
                .arg("-movflags")
                .arg("+faststart")
                .arg(output),
            self.timeout,
            "ffmpeg multi-input copy remux",
        )
        .await?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(RkError::Transcode(if stderr.is_empty() {
            "ffmpeg copy remux exited with failure".to_string()
        } else {
            stderr
        }))
    }

    async fn media_inputs_to_mp4_transcode(
        &self,
        video_input: &OsStr,
        video_input_args: &[OsString],
        audio_input: &OsStr,
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

        let output = run_process_with_timeout(
            command
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
                .arg(output),
            self.timeout,
            "ffmpeg multi-input transcode",
        )
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

    async fn probe_input(
        &self,
        input: impl AsRef<std::ffi::OsStr>,
        input_args: &[OsString],
    ) -> Result<MediaStreamInfo> {
        let mut command = Command::new(ffprobe_path(&self.ffmpeg_path));
        command
            .arg("-v")
            .arg("error")
            .arg("-show_entries")
            .arg("stream=codec_type")
            .arg("-of")
            .arg("csv=p=0");

        for arg in input_args {
            command.arg(arg);
        }

        let output = run_process_with_timeout(
            command.arg("-i").arg(input),
            self.timeout.min(Duration::from_secs(60)),
            "ffprobe stream probe",
        )
        .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(RkError::Transcode(if stderr.is_empty() {
                "ffprobe exited with failure".to_string()
            } else {
                stderr
            }));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(MediaStreamInfo {
            has_audio: stdout.lines().any(|line| line.trim() == "audio"),
            has_video: stdout.lines().any(|line| line.trim() == "video"),
        })
    }
}

async fn run_process_with_timeout(
    command: &mut Command,
    timeout: Duration,
    label: &str,
) -> Result<std::process::Output> {
    command.kill_on_drop(true);
    tokio_time::timeout(timeout, command.output())
        .await
        .map_err(|_| RkError::Transcode(format!("{label} timed out")))?
        .map_err(RkError::Io)
}

fn ffprobe_path(ffmpeg_path: &Path) -> PathBuf {
    let file_name = ffmpeg_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let probe_name = if file_name.ends_with(".exe") {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };

    ffmpeg_path
        .parent()
        .map(|parent| parent.join(probe_name))
        .unwrap_or_else(|| PathBuf::from(probe_name))
}

fn normalize_audio_bitrate(value: &str) -> &str {
    if value == "auto" || value.ends_with('p') {
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

pub fn concat_demuxer_file(images: &[PathBuf], seconds_per_image: f32) -> Result<String> {
    let Some(last) = images.last() else {
        return Err(RkError::Transcode(
            "image slideshow requires at least one image".to_string(),
        ));
    };

    let mut lines = String::new();
    for image in images {
        lines.push_str("file '");
        lines.push_str(&escape_concat_path(image));
        lines.push_str("'\n");
        lines.push_str(&format!("duration {seconds_per_image:.3}\n"));
    }
    lines.push_str("file '");
    lines.push_str(&escape_concat_path(last));
    lines.push_str("'\n");
    Ok(lines)
}

fn escape_concat_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_file_repeats_last_image_for_duration() {
        let images = vec![PathBuf::from("C:/tmp/a.jpg"), PathBuf::from("C:/tmp/b.png")];
        let list = concat_demuxer_file(&images, 2.5).unwrap();

        assert!(list.contains("file 'C:/tmp/a.jpg'\nduration 2.500"));
        assert!(list.ends_with("file 'C:/tmp/b.png'\n"));
        assert_eq!(list.matches("file '").count(), 3);
    }

    #[test]
    fn concat_file_rejects_empty_slideshow() {
        let error = concat_demuxer_file(&[], 2.5).unwrap_err().to_string();

        assert!(error.contains("requires at least one image"));
    }
}
