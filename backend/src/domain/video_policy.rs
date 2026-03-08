use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::http::error::{AppError, AppResult};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPolicy {
    pub video_policy_version: u32,
    pub max_upload_size_bytes: i64,
    pub max_upload_size_human: String,
    pub allowed_input_mime_types: Vec<String>,
    pub allowed_input_extensions: Vec<String>,
    pub baseline_rendition_profile: BaselineRenditionProfile,
}

impl VideoPolicy {
    pub async fn load(path: &Path) -> AppResult<Self> {
        let contents = tokio::fs::read_to_string(path)
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to read the video policy file",
                    format!("path={} error={error}", path.display()),
                )
            })?;

        serde_json::from_str(&contents).map_err(|error| {
            AppError::internal_with_context(
                "failed to parse the video policy file",
                format!("path={} error={error}", path.display()),
            )
        })
    }

    pub fn validate_upload(
        &self,
        filename: &str,
        content_type: &str,
        size_bytes: i64,
    ) -> AppResult<()> {
        if size_bytes <= 0 {
            return Err(AppError::bad_request("sizeBytes must be greater than zero"));
        }

        if size_bytes > self.max_upload_size_bytes {
            return Err(AppError::bad_request(format!(
                "sizeBytes exceeds the maximum allowed upload size of {}",
                self.max_upload_size_human
            )));
        }

        if !self
            .allowed_input_mime_types
            .iter()
            .any(|allowed_type| allowed_type == content_type)
        {
            return Err(AppError::bad_request(format!(
                "contentType is not allowed; expected one of {:?}",
                self.allowed_input_mime_types
            )));
        }

        let extension = PathBuf::from(filename)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{}", value.to_ascii_lowercase()))
            .ok_or_else(|| AppError::bad_request("filename must include a supported extension"))?;

        if !self
            .allowed_input_extensions
            .iter()
            .any(|allowed_extension| allowed_extension == &extension)
        {
            return Err(AppError::bad_request(format!(
                "filename extension is not allowed; expected one of {:?}",
                self.allowed_input_extensions
            )));
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineRenditionProfile {
    pub name: String,
    pub stream_protocol: String,
    pub segment_container: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub width: u32,
    pub height: u32,
    pub max_frame_rate: u32,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub segment_duration_seconds: u32,
    pub master_playlist_file_name: String,
    pub variant_playlist_file_name: String,
}

#[cfg(test)]
mod tests {
    use super::{BaselineRenditionProfile, VideoPolicy};

    fn sample_policy() -> VideoPolicy {
        VideoPolicy {
            video_policy_version: 1,
            max_upload_size_bytes: 1_073_741_824,
            max_upload_size_human: "1GB".to_owned(),
            allowed_input_mime_types: vec![
                "video/mp4".to_owned(),
                "video/quicktime".to_owned(),
                "video/webm".to_owned(),
            ],
            allowed_input_extensions: vec![
                ".mp4".to_owned(),
                ".mov".to_owned(),
                ".webm".to_owned(),
            ],
            baseline_rendition_profile: BaselineRenditionProfile {
                name: "360p".to_owned(),
                stream_protocol: "hls".to_owned(),
                segment_container: "mpegts".to_owned(),
                video_codec: "h264".to_owned(),
                audio_codec: "aac".to_owned(),
                width: 640,
                height: 360,
                max_frame_rate: 30,
                video_bitrate_kbps: 800,
                audio_bitrate_kbps: 128,
                segment_duration_seconds: 4,
                master_playlist_file_name: "master.m3u8".to_owned(),
                variant_playlist_file_name: "360p.m3u8".to_owned(),
            },
        }
    }

    #[test]
    fn accepts_supported_uploads() {
        let policy = sample_policy();

        let result = policy.validate_upload("demo.mp4", "video/mp4", 1024);

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_unsupported_content_type() {
        let policy = sample_policy();

        let result = policy.validate_upload("demo.mp4", "video/avi", 1024);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_unsupported_extension() {
        let policy = sample_policy();

        let result = policy.validate_upload("demo.avi", "video/mp4", 1024);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_oversized_uploads() {
        let policy = sample_policy();

        let result = policy.validate_upload("demo.mp4", "video/mp4", 1_073_741_825);

        assert!(result.is_err());
    }
}
