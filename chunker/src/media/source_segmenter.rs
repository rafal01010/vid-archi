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
        let total_duration_seconds = self.probe_duration_seconds(source_path).await?;
        let segment_duration_seconds = segment_duration_seconds as f64;
        let mut segments = Vec::new();
        let mut segment_start_seconds = 0.0;
        let mut segment_index = 0_i32;

        while segment_start_seconds < total_duration_seconds {
            let requested_duration_seconds =
                (total_duration_seconds - segment_start_seconds).min(segment_duration_seconds);
            let output_path = output_directory.join(format!("segment_{segment_index:05}.mkv"));
            let ffmpeg_output = self
                .create_independent_source_segment(
                    source_path,
                    &output_path,
                    segment_start_seconds,
                    requested_duration_seconds,
                )
                .await?;

            if !ffmpeg_output.status.success() {
                return Err(AppError::internal_with_context(
                    "ffmpeg failed to split the uploaded source video into segments",
                    String::from_utf8_lossy(&ffmpeg_output.stderr),
                ));
            }

            if tokio::fs::metadata(&output_path).await.is_err() {
                return Err(AppError::internal(
                    "ffmpeg completed without producing a source segment",
                ));
            }

            let duration_seconds = self.probe_duration_seconds(&output_path).await?;
            segments.push(PreparedSourceSegment {
                segment_index,
                path: output_path,
                duration_seconds,
            });

            segment_start_seconds += segment_duration_seconds;
            segment_index += 1;
        }

        if segments.is_empty() {
            return Err(AppError::internal(
                "ffmpeg completed without producing source segments",
            ));
        }

        Ok(segments)
    }

    async fn create_independent_source_segment(
        &self,
        source_path: &Path,
        output_path: &Path,
        segment_start_seconds: f64,
        segment_duration_seconds: f64,
    ) -> AppResult<std::process::Output> {
        let ffmpeg_binary = self.ffmpeg_binary.clone();
        let source_path_for_command = source_path.to_path_buf();
        let output_path_for_command = output_path.to_path_buf();

        task::spawn_blocking(move || {
            std::process::Command::new(ffmpeg_binary)
                .arg("-y")
                .arg("-i")
                .arg(source_path_for_command)
                .arg("-ss")
                .arg(format!("{segment_start_seconds:.3}"))
                .arg("-t")
                .arg(format!("{segment_duration_seconds:.3}"))
                .arg("-map")
                .arg("0:v:0")
                .arg("-map")
                .arg("0:a?")
                .arg("-map_metadata")
                .arg("-1")
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
                .arg("-metadata:s:v:0")
                .arg("rotate=0")
                .arg("-avoid_negative_ts")
                .arg("make_zero")
                .arg(output_path_for_command)
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

#[derive(Debug, Clone)]
pub struct PreparedSourceSegment {
    pub segment_index: i32,
    pub path: PathBuf,
    pub duration_seconds: f64,
}
