use std::path::Path;
use std::process::Stdio;

use serde::Deserialize;
use tokio::task;

use crate::domain::video_policy::RenditionProfile;
use crate::error::{AppError, AppResult};
use crate::infrastructure::config::TranscoderConfig;

#[derive(Clone)]
pub struct MediaProcessor {
    ffmpeg_binary: String,
    ffprobe_binary: String,
}

impl MediaProcessor {
    pub fn new(config: &TranscoderConfig) -> Self {
        Self {
            ffmpeg_binary: config.ffmpeg_binary.clone(),
            ffprobe_binary: config.ffprobe_binary.clone(),
        }
    }

    pub async fn transcode_segment(
        &self,
        source_path: &Path,
        output_root: &Path,
        profile: &RenditionProfile,
        source_width: u32,
        source_height: u32,
        segment_index: i32,
    ) -> AppResult<TranscodedSegment> {
        let scaled_resolution =
            compute_scaled_resolution(source_width, source_height, profile.width, profile.height)?;
        let source_rotation_degrees = self.probe_source_rotation_degrees(source_path).await?;
        let video_filter = build_video_filter(
            source_rotation_degrees,
            scaled_resolution.width,
            scaled_resolution.height,
        );
        let rendition_directory = output_root.join(&profile.name).join("segments");
        tokio::fs::create_dir_all(&rendition_directory).await?;
        let output_segment_file_name = format!("segment_{segment_index:05}.ts");
        let output_segment_path = rendition_directory.join(&output_segment_file_name);
        let gop = profile
            .max_frame_rate
            .saturating_mul(profile.segment_duration_seconds);

        let ffmpeg_binary = self.ffmpeg_binary.clone();
        let source_path = source_path.to_path_buf();
        let output_segment_path_for_command = output_segment_path.clone();
        let max_frame_rate = profile.max_frame_rate;
        let video_bitrate_kbps = profile.video_bitrate_kbps;
        let audio_bitrate_kbps = profile.audio_bitrate_kbps;
        let video_filter_for_command = video_filter.clone();
        let ffmpeg_output = task::spawn_blocking(move || {
            std::process::Command::new(ffmpeg_binary)
                .arg("-y")
                .arg("-noautorotate")
                .arg("-i")
                .arg(source_path)
                .arg("-vf")
                .arg(video_filter_for_command)
                .arg("-c:v")
                .arg("libx264")
                .arg("-preset")
                .arg("veryfast")
                .arg("-profile:v")
                .arg("main")
                .arg("-pix_fmt")
                .arg("yuv420p")
                .arg("-r")
                .arg(max_frame_rate.to_string())
                .arg("-g")
                .arg(gop.to_string())
                .arg("-keyint_min")
                .arg(gop.to_string())
                .arg("-sc_threshold")
                .arg("0")
                .arg("-b:v")
                .arg(format!("{video_bitrate_kbps}k"))
                .arg("-maxrate")
                .arg(format!("{}k", video_bitrate_kbps + 80))
                .arg("-bufsize")
                .arg(format!("{}k", video_bitrate_kbps * 2))
                .arg("-c:a")
                .arg("aac")
                .arg("-b:a")
                .arg(format!("{audio_bitrate_kbps}k"))
                .arg("-ar")
                .arg("48000")
                .arg("-ac")
                .arg("2")
                // Standalone MPEG-TS outputs need zero-ish timeline starts so the
                // assembled HLS playlist does not stall after the first segment.
                .arg("-muxpreload")
                .arg("0")
                .arg("-muxdelay")
                .arg("0")
                .arg("-f")
                .arg("mpegts")
                .arg(output_segment_path_for_command)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
        })
        .await
        .map_err(|error| {
            AppError::internal_with_context("failed to join ffmpeg task", error.to_string())
        })??;

        if !ffmpeg_output.status.success() {
            return Err(AppError::internal_with_context(
                "ffmpeg failed to build rendition HLS artifacts",
                String::from_utf8_lossy(&ffmpeg_output.stderr),
            ));
        }

        if tokio::fs::metadata(&output_segment_path).await.is_err() {
            return Err(AppError::internal(
                "ffmpeg completed without producing a transcoded segment",
            ));
        }

        Ok(TranscodedSegment {
            rendition: profile.name.clone(),
            codec: profile.video_codec.clone(),
            container: profile.segment_container.clone(),
            output_segment_file_name,
            output_width: scaled_resolution.width,
            output_height: scaled_resolution.height,
            target_video_bitrate_kbps: profile.video_bitrate_kbps,
            target_audio_bitrate_kbps: profile.audio_bitrate_kbps,
        })
    }

    async fn probe_source_rotation_degrees(&self, source_path: &Path) -> AppResult<i32> {
        let ffprobe_binary = self.ffprobe_binary.clone();
        let source_path_for_command = source_path.to_path_buf();
        let output = task::spawn_blocking(move || {
            std::process::Command::new(ffprobe_binary)
                .arg("-v")
                .arg("error")
                .arg("-select_streams")
                .arg("v:0")
                .arg("-show_entries")
                .arg("stream_tags=rotate:stream_side_data=rotation")
                .arg("-of")
                .arg("json")
                .arg(source_path_for_command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
        })
        .await
        .map_err(|error| {
            AppError::internal_with_context("failed to join ffprobe rotation task", error.to_string())
        })??;

        if !output.status.success() {
            return Err(AppError::internal_with_context(
                "ffprobe failed to inspect source segment rotation",
                String::from_utf8_lossy(&output.stderr),
            ));
        }

        let parsed: RotationProbeResponse =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                AppError::internal_with_context(
                    "failed to parse ffprobe source rotation JSON",
                    error.to_string(),
                )
            })?;

        Ok(parsed
            .streams
            .into_iter()
            .find_map(|stream| {
                stream
                    .side_data_list
                    .into_iter()
                    .find_map(|side_data| side_data.rotation)
                    .or_else(|| {
                        stream
                            .tags
                            .and_then(|tags| tags.rotate)
                            .and_then(|value| value.parse::<i32>().ok())
                    })
            })
            .unwrap_or(0))
    }
}

fn build_video_filter(rotation_degrees: i32, scaled_width: u32, scaled_height: u32) -> String {
    let scale_filter = format!("scale={scaled_width}:{scaled_height}");

    match rotation_degrees.rem_euclid(360) {
        90 => format!("transpose=cclock,{scale_filter}"),
        180 => format!("transpose=clock,transpose=clock,{scale_filter}"),
        270 => format!("transpose=clock,{scale_filter}"),
        _ => scale_filter,
    }
}

#[derive(Debug, Deserialize)]
struct RotationProbeResponse {
    #[serde(default)]
    streams: Vec<RotationProbeStream>,
}

#[derive(Debug, Deserialize)]
struct RotationProbeStream {
    tags: Option<RotationProbeTags>,
    #[serde(default)]
    side_data_list: Vec<RotationProbeSideData>,
}

#[derive(Debug, Deserialize)]
struct RotationProbeTags {
    rotate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RotationProbeSideData {
    rotation: Option<i32>,
}

pub fn compute_scaled_resolution(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> AppResult<ScaledResolution> {
    if source_width == 0 || source_height == 0 {
        return Err(AppError::internal("source dimensions must be positive"));
    }

    let scale = f64::min(
        f64::min(
            target_width as f64 / source_width as f64,
            target_height as f64 / source_height as f64,
        ),
        1.0,
    );

    let scaled_width = normalize_even_dimension((source_width as f64 * scale).floor() as u32);
    let scaled_height = normalize_even_dimension((source_height as f64 * scale).floor() as u32);

    Ok(ScaledResolution {
        width: scaled_width,
        height: scaled_height,
    })
}

fn normalize_even_dimension(value: u32) -> u32 {
    let clamped = value.max(2);

    if clamped % 2 == 0 {
        clamped
    } else {
        clamped.saturating_sub(1).max(2)
    }
}

#[derive(Debug, Clone)]
pub struct ScaledResolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct TranscodedSegment {
    pub rendition: String,
    pub codec: String,
    pub container: String,
    pub output_segment_file_name: String,
    pub output_width: u32,
    pub output_height: u32,
    pub target_video_bitrate_kbps: u32,
    pub target_audio_bitrate_kbps: u32,
}

#[cfg(test)]
mod tests {
    use super::{build_video_filter, compute_scaled_resolution};

    #[test]
    fn compute_scaled_resolution_keeps_baseline_dimensions_when_source_is_large_enough() {
        let scaled = compute_scaled_resolution(1920, 1080, 640, 360).expect("scaled resolution");

        assert_eq!(scaled.width, 640);
        assert_eq!(scaled.height, 360);
    }

    #[test]
    fn compute_scaled_resolution_does_not_upscale_small_sources() {
        let scaled = compute_scaled_resolution(320, 180, 640, 360).expect("scaled resolution");

        assert_eq!(scaled.width, 320);
        assert_eq!(scaled.height, 180);
    }

    #[test]
    fn compute_scaled_resolution_preserves_portrait_orientation() {
        let scaled = compute_scaled_resolution(2160, 3840, 3840, 2160).expect("scaled resolution");

        assert_eq!(scaled.width, 1214);
        assert_eq!(scaled.height, 2160);
    }

    #[test]
    fn build_video_filter_applies_rotation_before_scaling() {
        assert_eq!(build_video_filter(0, 640, 360), "scale=640:360");
        assert_eq!(
            build_video_filter(-90, 640, 360),
            "transpose=clock,scale=640:360"
        );
        assert_eq!(
            build_video_filter(90, 640, 360),
            "transpose=cclock,scale=640:360"
        );
    }
}
