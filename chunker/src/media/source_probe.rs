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
                .arg("stream=width,height")
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

        Ok(ProbedSourceVideo {
            width: stream.width.expect("checked width is present"),
            height: stream.height.expect("checked height is present"),
        })
    }
}

#[derive(Debug)]
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
}
