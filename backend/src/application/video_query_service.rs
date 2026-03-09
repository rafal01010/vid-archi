use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::domain::video_policy::VideoPolicy;
use crate::http::error::{AppError, AppResult};
use crate::infrastructure::config::AppConfig;
use crate::infrastructure::postgres::{
    ReadyRenditionRecord, VideoDetailRecord, VideoRepository, VideoSummaryRecord,
};

#[derive(Clone)]
pub struct VideoQueryService {
    config: Arc<AppConfig>,
    policy: Arc<VideoPolicy>,
    repository: VideoRepository,
    max_recent_video_limit: usize,
}

impl VideoQueryService {
    pub fn new(config: AppConfig, policy: VideoPolicy, repository: VideoRepository) -> Self {
        Self {
            config: Arc::new(config),
            policy: Arc::new(policy),
            repository,
            max_recent_video_limit: 100,
        }
    }

    pub async fn list_recent_videos(
        &self,
        requested_page: Option<usize>,
        requested_limit: Option<usize>,
        requested_page_size: Option<usize>,
    ) -> AppResult<ListRecentVideosResult> {
        let page = normalize_page_number(requested_page)?;
        let page_size = normalize_recent_video_limit(
            requested_page_size.or(requested_limit),
            self.max_recent_video_limit,
        )?;
        let offset = (page - 1) * page_size;
        let total_count = self.repository.count_videos().await?;
        let videos = self
            .repository
            .list_recent_videos(page_size as i64, offset as i64)
            .await?;
        let total_pages = if total_count == 0 {
            1
        } else {
            ((total_count + page_size as i64 - 1) / page_size as i64) as usize
        };

        Ok(ListRecentVideosResult {
            videos: videos.into_iter().map(map_video_summary).collect(),
            page,
            page_size,
            total_count,
            total_pages,
            has_previous_page: page > 1,
            has_next_page: page < total_pages,
        })
    }

    pub async fn get_video_details(&self, public_id: &str) -> AppResult<VideoDetailsResult> {
        let normalized_public_id = public_id.trim();

        if normalized_public_id.is_empty() {
            return Err(AppError::bad_request("public id is required"));
        }

        let video = self
            .repository
            .find_video_by_public_id(normalized_public_id)
            .await?
            .ok_or_else(|| AppError::not_found("video not found"))?;

        Ok(map_video_details(&self.config, video))
    }

    pub async fn get_video_playback(&self, public_id: &str) -> AppResult<VideoPlaybackResult> {
        let normalized_public_id = public_id.trim();

        if normalized_public_id.is_empty() {
            return Err(AppError::bad_request("public id is required"));
        }

        let video = self
            .repository
            .find_video_by_public_id(normalized_public_id)
            .await?
            .ok_or_else(|| AppError::not_found("video not found"))?;
        let ready_renditions = self
            .repository
            .list_ready_renditions(video.id)
            .await?;

        Ok(map_video_playback(&self.config, &self.policy, video, ready_renditions))
    }
}

#[derive(Debug)]
pub struct ListRecentVideosResult {
    pub videos: Vec<VideoSummary>,
    pub page: usize,
    pub page_size: usize,
    pub total_count: i64,
    pub total_pages: usize,
    pub has_previous_page: bool,
    pub has_next_page: bool,
}

#[derive(Debug)]
pub struct VideoSummary {
    pub public_id: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub status: String,
    pub is_streamable: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub playback_path: String,
}

#[derive(Debug)]
pub struct VideoDetailsResult {
    pub public_id: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub status: String,
    pub is_streamable: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub playback_path: String,
    pub manifest_url: Option<String>,
}

#[derive(Debug)]
pub struct VideoPlaybackResult {
    pub public_id: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub status: String,
    pub is_streamable: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub playback_path: String,
    pub manifest_url: Option<String>,
    pub default_quality: String,
    pub poll_interval_ms: u64,
    pub available_qualities: Vec<PlaybackQualityResult>,
}

#[derive(Debug)]
pub struct PlaybackQualityResult {
    pub name: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub codec: String,
    pub container: String,
    pub playlist_url: String,
}

fn normalize_recent_video_limit(
    requested_limit: Option<usize>,
    max_limit: usize,
) -> AppResult<usize> {
    let limit = requested_limit.unwrap_or(10);

    if limit == 0 {
        return Err(AppError::bad_request("limit must be greater than zero"));
    }

    Ok(limit.min(max_limit))
}

fn normalize_page_number(requested_page: Option<usize>) -> AppResult<usize> {
    let page = requested_page.unwrap_or(1);

    if page == 0 {
        return Err(AppError::bad_request("page must be greater than zero"));
    }

    Ok(page)
}

fn map_video_summary(video: VideoSummaryRecord) -> VideoSummary {
    VideoSummary {
        public_id: video.public_id.clone(),
        title: video.title,
        original_filename: video.original_filename,
        status: video.status,
        is_streamable: video.is_streamable,
        created_at: video.created_at,
        updated_at: video.updated_at,
        playback_path: format!("/v/{}", video.public_id),
    }
}

fn map_video_details(config: &AppConfig, video: VideoDetailRecord) -> VideoDetailsResult {
    VideoDetailsResult {
        public_id: video.public_id.clone(),
        title: video.title,
        original_filename: video.original_filename,
        status: video.status,
        is_streamable: video.is_streamable,
        created_at: video.created_at,
        updated_at: video.updated_at,
        playback_path: format!("/v/{}", video.public_id),
        manifest_url: video
            .manifest_s3_key
            .as_deref()
            .and_then(|manifest_key| config.manifest_url_for_key(manifest_key)),
    }
}

fn map_video_playback(
    config: &AppConfig,
    policy: &VideoPolicy,
    video: VideoDetailRecord,
    ready_renditions: Vec<ReadyRenditionRecord>,
) -> VideoPlaybackResult {
    let available_qualities = policy
        .adaptive_rendition_ladder
        .iter()
        .filter_map(|profile| {
            ready_renditions
                .iter()
                .find(|rendition| rendition.rendition == profile.name)
                .and_then(|rendition| {
                    config
                        .asset_url_for_key(&rendition.playlist_key)
                        .map(|playlist_url| PlaybackQualityResult {
                            name: profile.name.clone(),
                            label: profile.name.clone(),
                            width: rendition.output_width.unwrap_or(profile.width as i32) as u32,
                            height: rendition.output_height.unwrap_or(profile.height as i32) as u32,
                            codec: rendition.codec.clone(),
                            container: rendition.container.clone(),
                            playlist_url,
                        })
                })
        })
        .collect();

    VideoPlaybackResult {
        public_id: video.public_id.clone(),
        title: video.title,
        original_filename: video.original_filename,
        status: video.status,
        is_streamable: video.is_streamable,
        created_at: video.created_at,
        updated_at: video.updated_at,
        playback_path: format!("/v/{}", video.public_id),
        manifest_url: video
            .manifest_s3_key
            .as_deref()
            .and_then(|manifest_key| config.manifest_url_for_key(manifest_key)),
        default_quality: "auto".to_owned(),
        poll_interval_ms: 5000,
        available_qualities,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::{
        map_video_playback, normalize_page_number, normalize_recent_video_limit, ReadyRenditionRecord,
        VideoDetailRecord,
    };
    use crate::domain::video_policy::VideoPolicy;
    use crate::infrastructure::config::AppConfig;

    #[test]
    fn normalize_recent_video_limit_uses_default_when_unspecified() {
        let limit = normalize_recent_video_limit(None, 100).expect("normalized limit");

        assert_eq!(limit, 10);
    }

    #[test]
    fn normalize_recent_video_limit_caps_at_maximum() {
        let limit = normalize_recent_video_limit(Some(500), 100).expect("normalized limit");

        assert_eq!(limit, 100);
    }

    #[test]
    fn normalize_recent_video_limit_rejects_zero() {
        let result = normalize_recent_video_limit(Some(0), 100);

        assert!(result.is_err());
    }

    #[test]
    fn normalize_page_number_uses_first_page_when_unspecified() {
        let page = normalize_page_number(None).expect("normalized page");

        assert_eq!(page, 1);
    }

    #[test]
    fn normalize_page_number_rejects_zero() {
        let result = normalize_page_number(Some(0));

        assert!(result.is_err());
    }

    #[test]
    fn map_video_playback_orders_ready_qualities_by_policy_ladder() {
        let config = AppConfig {
            app_env: "test".to_owned(),
            api_port: 8080,
            database_url: "postgres://localhost/test".to_owned(),
            aws_region: "ap-northeast-1".to_owned(),
            upload_bucket: "upload-bucket".to_owned(),
            processed_bucket: "processed-bucket".to_owned(),
            processed_asset_base_url: Some("https://cdn.example.com".to_owned()),
            video_policy_file: "config/video_policy.json".into(),
            s3_endpoint_url: None,
            s3_force_path_style: false,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            multipart_part_size_bytes: 8 * 1024 * 1024,
            upload_session_ttl_seconds: 3600,
            presign_ttl_seconds: 900,
        };
        let policy = VideoPolicy {
            video_policy_version: 1,
            max_upload_size_bytes: 1_073_741_824,
            max_upload_size_human: "1GB".to_owned(),
            allowed_input_mime_types: Vec::new(),
            allowed_input_extensions: Vec::new(),
            baseline_rendition_profile: crate::domain::video_policy::BaselineRenditionProfile {
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
            adaptive_rendition_ladder: vec![
                crate::domain::video_policy::AdaptiveRenditionProfile {
                    name: "360p".to_owned(),
                    width: 640,
                    height: 360,
                    video_bitrate_kbps: 800,
                    audio_bitrate_kbps: 128,
                    variant_playlist_file_name: "360p.m3u8".to_owned(),
                },
                crate::domain::video_policy::AdaptiveRenditionProfile {
                    name: "720p".to_owned(),
                    width: 1280,
                    height: 720,
                    video_bitrate_kbps: 2800,
                    audio_bitrate_kbps: 128,
                    variant_playlist_file_name: "720p.m3u8".to_owned(),
                },
            ],
        };
        let playback = map_video_playback(
            &config,
            &policy,
            VideoDetailRecord {
                id: Uuid::new_v4(),
                public_id: "demo-video".to_owned(),
                title: Some("Demo".to_owned()),
                original_filename: "demo.mp4".to_owned(),
                status: "READY".to_owned(),
                is_streamable: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                manifest_s3_key: Some("videos/demo/hls/master.m3u8".to_owned()),
            },
            vec![
                ReadyRenditionRecord {
                    rendition: "720p".to_owned(),
                    codec: "h264".to_owned(),
                    container: "mpegts".to_owned(),
                    playlist_key: "videos/demo/hls/720p/720p.m3u8".to_owned(),
                    output_width: Some(960),
                    output_height: Some(540),
                },
                ReadyRenditionRecord {
                    rendition: "360p".to_owned(),
                    codec: "h264".to_owned(),
                    container: "mpegts".to_owned(),
                    playlist_key: "videos/demo/hls/360p/360p.m3u8".to_owned(),
                    output_width: Some(640),
                    output_height: Some(360),
                },
            ],
        );

        assert_eq!(playback.default_quality, "auto");
        assert_eq!(playback.available_qualities.len(), 2);
        assert_eq!(playback.available_qualities[0].name, "360p");
        assert_eq!(playback.available_qualities[0].width, 640);
        assert_eq!(playback.available_qualities[1].name, "720p");
        assert_eq!(playback.available_qualities[1].width, 960);
    }
}
