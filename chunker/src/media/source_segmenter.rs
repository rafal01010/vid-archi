use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::task;

use crate::domain::video_policy::AdaptiveRenditionProfile;
use crate::error::{AppError, AppResult};
use crate::infrastructure::config::ChunkerConfig;

#[derive(Clone)]
pub struct SourceSegmenter {
    ffmpeg_binary: String,
    ffprobe_binary: String,
}

impl SourceSegmenter {
    pub fn new(config: &ChunkerConfig) -> Self {
        Self {
            ffmpeg_binary: config.ffmpeg_binary.clone(),
            ffprobe_binary: config.ffprobe_binary.clone(),
        }
    }

    pub async fn build_intermediate_rendition(
        &self,
        source_path: &Path,
        output_directory: &Path,
        rendition: &AdaptiveRenditionProfile,
        segment_duration_seconds: u32,
    ) -> AppResult<PreparedIntermediateRendition> {
        let rendition_directory = output_directory.join(&rendition.name);
        tokio::fs::create_dir_all(&rendition_directory).await?;

        let variant_playlist_path = rendition_directory.join(format!("{}.m3u8", rendition.name));
        let segment_pattern = rendition_directory.join("segment_%05d.ts");
        let ffmpeg_output = self
            .encode_source_to_hls_segments(
                source_path,
                &variant_playlist_path,
                &segment_pattern,
                rendition,
                segment_duration_seconds,
            )
            .await?;

        if !ffmpeg_output.status.success() {
            return Err(AppError::internal_with_context(
                "ffmpeg failed to build the intermediate rendition",
                String::from_utf8_lossy(&ffmpeg_output.stderr),
            ));
        }

        if tokio::fs::metadata(&variant_playlist_path).await.is_err() {
            return Err(AppError::internal(
                "ffmpeg completed without producing the intermediate rendition playlist",
            ));
        }

        let mut segment_paths = collect_segment_paths(&rendition_directory).await?;
        segment_paths.sort();

        if segment_paths.is_empty() {
            return Err(AppError::internal(
                "ffmpeg completed without producing intermediate rendition segments",
            ));
        }

        let mut prepared_segments = Vec::with_capacity(segment_paths.len());
        for (segment_index, path) in segment_paths.into_iter().enumerate() {
            let duration_seconds = self.probe_duration_seconds(&path).await?;
            prepared_segments.push(PreparedIntermediateSegment {
                segment_index: segment_index as i32,
                path,
                duration_seconds,
            });
        }

        Ok(PreparedIntermediateRendition {
            rendition_name: rendition.name.clone(),
            width: rendition.width,
            height: rendition.height,
            video_bitrate_kbps: rendition.video_bitrate_kbps,
            audio_bitrate_kbps: rendition.audio_bitrate_kbps,
            variant_playlist_path,
            segments: prepared_segments,
        })
    }

    async fn encode_source_to_hls_segments(
        &self,
        source_path: &Path,
        variant_playlist_path: &Path,
        segment_pattern: &Path,
        rendition: &AdaptiveRenditionProfile,
        segment_duration_seconds: u32,
    ) -> AppResult<std::process::Output> {
        let ffmpeg_binary = self.ffmpeg_binary.clone();
        let source_path_for_command = source_path.to_path_buf();
        let variant_playlist_path_for_command = variant_playlist_path.to_path_buf();
        let segment_pattern_for_command = segment_pattern.to_path_buf();
        let rendition_width = rendition.width;
        let rendition_height = rendition.height;
        let video_bitrate_kbps = rendition.video_bitrate_kbps;
        let audio_bitrate_kbps = rendition.audio_bitrate_kbps;

        task::spawn_blocking(move || {
            std::process::Command::new(ffmpeg_binary)
                .arg("-y")
                .arg("-i")
                .arg(source_path_for_command)
                .arg("-vf")
                .arg(format!(
                    "scale=w={}:h={}:force_original_aspect_ratio=decrease",
                    rendition_width, rendition_height
                ))
                .arg("-c:v")
                .arg("libx264")
                .arg("-preset")
                .arg("veryfast")
                .arg("-profile:v")
                .arg("main")
                .arg("-pix_fmt")
                .arg("yuv420p")
                .arg("-r")
                .arg("30")
                .arg("-g")
                .arg((30 * segment_duration_seconds).to_string())
                .arg("-keyint_min")
                .arg((30 * segment_duration_seconds).to_string())
                .arg("-sc_threshold")
                .arg("0")
                .arg("-b:v")
                .arg(format!("{}k", video_bitrate_kbps))
                .arg("-maxrate")
                .arg(format!("{}k", video_bitrate_kbps + 80))
                .arg("-bufsize")
                .arg(format!("{}k", video_bitrate_kbps * 2))
                .arg("-c:a")
                .arg("aac")
                .arg("-b:a")
                .arg(format!("{}k", audio_bitrate_kbps))
                .arg("-ar")
                .arg("48000")
                .arg("-ac")
                .arg("2")
                .arg("-f")
                .arg("hls")
                .arg("-hls_time")
                .arg(segment_duration_seconds.to_string())
                .arg("-hls_playlist_type")
                .arg("vod")
                .arg("-hls_segment_filename")
                .arg(segment_pattern_for_command)
                .arg("-hls_flags")
                .arg("independent_segments")
                .arg(variant_playlist_path_for_command)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
        })
        .await
        .map_err(|error| {
            AppError::internal_with_context(
                "failed to join ffmpeg intermediate rendition task",
                error.to_string(),
            )
        })?
        .map_err(AppError::from)
    }

    async fn probe_duration_seconds(&self, source_path: &Path) -> AppResult<f64> {
        let ffprobe_binary = self.ffprobe_binary.clone();
        let source_path_for_command = source_path.to_path_buf();
        let output = task::spawn_blocking(move || {
            std::process::Command::new(ffprobe_binary)
                .arg("-v")
                .arg("error")
                .arg("-show_entries")
                .arg("format=duration")
                .arg("-of")
                .arg("default=noprint_wrappers=1:nokey=1")
                .arg(source_path_for_command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
        })
        .await
        .map_err(|error| {
            AppError::internal_with_context("failed to join ffprobe duration task", error.to_string())
        })??;

        if !output.status.success() {
            return Err(AppError::internal_with_context(
                "ffprobe failed to inspect the generated intermediate segment duration",
                String::from_utf8_lossy(&output.stderr),
            ));
        }

        let parsed = String::from_utf8_lossy(&output.stdout);
        let duration_seconds = parsed.trim().parse::<f64>().map_err(|error| {
            AppError::internal_with_context(
                "failed to parse ffprobe duration output",
                error.to_string(),
            )
        })?;

        if duration_seconds <= 0.0 {
            return Err(AppError::internal(
                "generated intermediate segment duration must be positive",
            ));
        }

        Ok(duration_seconds)
    }
}

async fn collect_segment_paths(output_directory: &Path) -> AppResult<Vec<PathBuf>> {
    let mut entries = tokio::fs::read_dir(output_directory).await?;
    let mut segment_paths = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let is_segment = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("ts"))
            .unwrap_or(false);

        if is_segment {
            segment_paths.push(path);
        }
    }

    Ok(segment_paths)
}

#[derive(Debug, Clone)]
pub struct PreparedIntermediateRendition {
    pub rendition_name: String,
    pub width: u32,
    pub height: u32,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub variant_playlist_path: PathBuf,
    pub segments: Vec<PreparedIntermediateSegment>,
}

#[derive(Debug, Clone)]
pub struct PreparedIntermediateSegment {
    pub segment_index: i32,
    pub path: PathBuf,
    pub duration_seconds: f64,
}
