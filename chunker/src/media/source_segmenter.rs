use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::task;

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

    pub async fn segment_source(
        &self,
        source_path: &Path,
        output_directory: &Path,
        segment_duration_seconds: u32,
        _source_rotation_degrees: i32,
    ) -> AppResult<Vec<PreparedSourceSegment>> {
        tokio::fs::create_dir_all(output_directory).await?;

        let ffmpeg_output = self
            .segment_source_into_intermediate_chunks(
                source_path,
                output_directory,
                segment_duration_seconds,
            )
            .await?;

        if !ffmpeg_output.status.success() {
            return Err(AppError::internal_with_context(
                "ffmpeg failed to split the uploaded source video into segments",
                String::from_utf8_lossy(&ffmpeg_output.stderr),
            ));
        }

        let mut segment_paths = collect_segment_paths(output_directory).await?;
        segment_paths.sort();

        if segment_paths.is_empty() {
            return Err(AppError::internal(
                "ffmpeg completed without producing source segments",
            ));
        }

        let mut segments = Vec::with_capacity(segment_paths.len());
        for (segment_index, path) in segment_paths.into_iter().enumerate() {
            let duration_seconds = self.probe_duration_seconds(&path).await?;
            segments.push(PreparedSourceSegment {
                segment_index: segment_index as i32,
                path,
                duration_seconds,
            });
        }

        Ok(segments)
    }

    async fn segment_source_into_intermediate_chunks(
        &self,
        source_path: &Path,
        output_directory: &Path,
        segment_duration_seconds: u32,
    ) -> AppResult<std::process::Output> {
        let ffmpeg_binary = self.ffmpeg_binary.clone();
        let source_path_for_command = source_path.to_path_buf();
        let output_pattern = output_directory.join("segment_%05d.mkv");

        task::spawn_blocking(move || {
            std::process::Command::new(ffmpeg_binary)
                .arg("-y")
                .arg("-noautorotate")
                .arg("-i")
                .arg(source_path_for_command)
                .arg("-c:v")
                .arg("libx264")
                .arg("-preset")
                .arg("ultrafast")
                .arg("-crf")
                .arg("18")
                .arg("-pix_fmt")
                .arg("yuv420p")
                .arg("-sc_threshold")
                .arg("0")
                .arg("-force_key_frames")
                .arg(format!("expr:gte(t,n_forced*{segment_duration_seconds})"))
                .arg("-x264-params")
                .arg("open-gop=0")
                .arg("-c:a")
                .arg("aac")
                .arg("-b:a")
                .arg("192k")
                .arg("-ar")
                .arg("48000")
                .arg("-ac")
                .arg("2")
                .arg("-f")
                .arg("segment")
                .arg("-segment_time")
                .arg(segment_duration_seconds.to_string())
                .arg("-reset_timestamps")
                .arg("1")
                .arg(output_pattern)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
        })
        .await
        .map_err(|error| {
            AppError::internal_with_context(
                "failed to join ffmpeg intermediate chunking task",
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
                "ffprobe failed to inspect the generated source segment duration",
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
                "generated source segment duration must be positive",
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
            .map(|value| value.eq_ignore_ascii_case("mkv"))
            .unwrap_or(false);

        if is_segment {
            segment_paths.push(path);
        }
    }

    Ok(segment_paths)
}

#[derive(Debug, Clone)]
pub struct PreparedSourceSegment {
    pub segment_index: i32,
    pub path: PathBuf,
    pub duration_seconds: f64,
}
