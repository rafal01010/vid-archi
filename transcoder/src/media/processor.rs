use std::path::Path;
use std::process::Stdio;

use tokio::task;

use crate::domain::video_policy::RenditionProfile;
use crate::error::{AppError, AppResult};
use crate::infrastructure::config::TranscoderConfig;

#[derive(Clone)]
pub struct MediaProcessor {
    ffmpeg_binary: String,
}

impl MediaProcessor {
    pub fn new(config: &TranscoderConfig) -> Self {
        Self {
            ffmpeg_binary: config.ffmpeg_binary.clone(),
        }
    }

    pub async fn package_rendition_hls(
        &self,
        source_path: &Path,
        output_root: &Path,
        profile: &RenditionProfile,
        source_width: u32,
        source_height: u32,
    ) -> AppResult<PackagedRendition> {
        let scaled_resolution =
            compute_scaled_resolution(source_width, source_height, profile.width, profile.height)?;
        let rendition_directory = output_root.join(&profile.name);
        tokio::fs::create_dir_all(&rendition_directory).await?;
        let variant_playlist_path = rendition_directory.join(&profile.variant_playlist_file_name);
        let segment_pattern = rendition_directory.join("segment_%03d.ts");
        let gop = profile
            .max_frame_rate
            .saturating_mul(profile.segment_duration_seconds);

        let ffmpeg_binary = self.ffmpeg_binary.clone();
        let source_path = source_path.to_path_buf();
        let variant_playlist_path_for_command = variant_playlist_path.clone();
        let segment_pattern_for_command = segment_pattern.clone();
        let max_frame_rate = profile.max_frame_rate;
        let video_bitrate_kbps = profile.video_bitrate_kbps;
        let audio_bitrate_kbps = profile.audio_bitrate_kbps;
        let segment_duration_seconds = profile.segment_duration_seconds;
        let scaled_width = scaled_resolution.width;
        let scaled_height = scaled_resolution.height;
        let ffmpeg_output = task::spawn_blocking(move || {
            std::process::Command::new(ffmpeg_binary)
                .arg("-y")
                .arg("-i")
                .arg(source_path)
                .arg("-vf")
                .arg(format!("scale={scaled_width}:{scaled_height}"))
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
                .arg("-hls_time")
                .arg(segment_duration_seconds.to_string())
                // Emit a complete VOD package before we mark the video streamable.
                // That keeps seeks/skips valid across the whole baseline timeline.
                .arg("-hls_playlist_type")
                .arg("vod")
                .arg("-hls_segment_filename")
                .arg(segment_pattern_for_command)
                .arg("-f")
                .arg("hls")
                .arg(variant_playlist_path_for_command)
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

        let segment_count = count_segment_files(&rendition_directory).await?;
        if segment_count == 0 {
            return Err(AppError::internal(
                "ffmpeg completed without producing HLS segments",
            ));
        }

        Ok(PackagedRendition {
            rendition: profile.name.clone(),
            codec: profile.video_codec.clone(),
            container: profile.segment_container.clone(),
            playlist_file_name: profile.variant_playlist_file_name.clone(),
            output_width: scaled_resolution.width,
            output_height: scaled_resolution.height,
            target_video_bitrate_kbps: profile.video_bitrate_kbps,
            target_audio_bitrate_kbps: profile.audio_bitrate_kbps,
            segment_count,
        })
    }
}

async fn count_segment_files(rendition_directory: &Path) -> AppResult<usize> {
    let mut entries = tokio::fs::read_dir(rendition_directory).await?;
    let mut segment_count = 0usize;

    while let Some(entry) = entries.next_entry().await? {
        let is_segment = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("ts"))
            .unwrap_or(false);

        if is_segment {
            segment_count += 1;
        }
    }

    Ok(segment_count)
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
pub struct PackagedRendition {
    pub rendition: String,
    pub codec: String,
    pub container: String,
    pub playlist_file_name: String,
    pub output_width: u32,
    pub output_height: u32,
    pub target_video_bitrate_kbps: u32,
    pub target_audio_bitrate_kbps: u32,
    pub segment_count: usize,
}

#[cfg(test)]
mod tests {
    use super::compute_scaled_resolution;

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
}
