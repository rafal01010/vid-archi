use std::path::Path;
use std::process::Stdio;

use serde::Deserialize;
use tokio::task;

use crate::error::{AppError, AppResult};
use crate::infrastructure::config::ChunkerConfig;

#[derive(Clone)]
pub struct SourceProbe {
    ffprobe_binary: String,
}

impl SourceProbe {
    pub fn new(config: &ChunkerConfig) -> Self {
        Self {
            ffprobe_binary: config.ffprobe_binary.clone(),
        }
    }

    pub async fn probe(&self, source_path: &Path) -> AppResult<ProbedSourceVideo> {
        let ffprobe_binary = self.ffprobe_binary.clone();
        let source_path = source_path.to_path_buf();
        let output = task::spawn_blocking(move || {
            std::process::Command::new(ffprobe_binary)
                .arg("-v")
                .arg("error")
                .arg("-select_streams")
                .arg("v:0")
                .arg("-show_entries")
                .arg("stream=width,height:stream_tags=rotate:stream_side_data=rotation")
                .arg("-of")
                .arg("json")
                .arg(source_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
        })
        .await
        .map_err(|error| {
            AppError::internal_with_context("failed to join ffprobe task", error.to_string())
        })??;

        if !output.status.success() {
            return Err(AppError::internal_with_context(
                "ffprobe failed to inspect the uploaded video",
                String::from_utf8_lossy(&output.stderr),
            ));
        }

        let parsed: ProbeResponse = serde_json::from_slice(&output.stdout).map_err(|error| {
            AppError::internal_with_context(
                "failed to parse ffprobe JSON output",
                error.to_string(),
            )
        })?;
        let stream = parsed
            .streams
            .into_iter()
            .find(|stream| stream.width.is_some() && stream.height.is_some())
            .ok_or_else(|| AppError::internal("ffprobe did not return source dimensions"))?;
        let normalized_dimensions = normalize_source_dimensions(&stream)?;

        Ok(normalized_dimensions)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProbedSourceVideo {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize)]
struct ProbeResponse {
    #[serde(default)]
    streams: Vec<ProbeStream>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    width: Option<u32>,
    height: Option<u32>,
    tags: Option<ProbeStreamTags>,
    #[serde(default)]
    side_data_list: Vec<ProbeSideData>,
}

#[derive(Debug, Deserialize)]
struct ProbeStreamTags {
    rotate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeSideData {
    rotation: Option<i32>,
}

fn normalize_source_dimensions(stream: &ProbeStream) -> AppResult<ProbedSourceVideo> {
    let width = stream
        .width
        .ok_or_else(|| AppError::internal("ffprobe did not return source width"))?;
    let height = stream
        .height
        .ok_or_else(|| AppError::internal("ffprobe did not return source height"))?;

    let rotation_degrees = stream
        .side_data_list
        .iter()
        .find_map(|side_data| side_data.rotation)
        .or_else(|| {
            stream
                .tags
                .as_ref()
                .and_then(|tags| tags.rotate.as_deref())
                .and_then(|value| value.parse::<i32>().ok())
        })
        .unwrap_or(0);

    let requires_swap = rotation_degrees.rem_euclid(180) != 0;

    if requires_swap {
        return Ok(ProbedSourceVideo {
            width: height,
            height: width,
        });
    }

    Ok(ProbedSourceVideo { width, height })
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_source_dimensions, ProbedSourceVideo, ProbeSideData, ProbeStream, ProbeStreamTags,
    };

    #[test]
    fn normalize_source_dimensions_keeps_unrotated_dimensions() {
        let stream = ProbeStream {
            width: Some(3840),
            height: Some(2160),
            tags: None,
            side_data_list: Vec::new(),
        };

        let video = normalize_source_dimensions(&stream).expect("normalized dimensions");

        assert_eq!(video.width, 3840);
        assert_eq!(video.height, 2160);
    }

    #[test]
    fn normalize_source_dimensions_swaps_for_side_data_rotation() {
        let stream = ProbeStream {
            width: Some(3840),
            height: Some(2160),
            tags: None,
            side_data_list: vec![ProbeSideData {
                rotation: Some(90),
            }],
        };

        let video = normalize_source_dimensions(&stream).expect("normalized dimensions");

        assert_eq!(
            video,
            ProbedSourceVideo {
                width: 2160,
                height: 3840,
            }
        );
    }

    #[test]
    fn normalize_source_dimensions_swaps_for_rotate_tag() {
        let stream = ProbeStream {
            width: Some(1920),
            height: Some(1080),
            tags: Some(ProbeStreamTags {
                rotate: Some("-90".to_owned()),
            }),
            side_data_list: Vec::new(),
        };

        let video = normalize_source_dimensions(&stream).expect("normalized dimensions");

        assert_eq!(
            video,
            ProbedSourceVideo {
                width: 1080,
                height: 1920,
            }
        );
    }
}
