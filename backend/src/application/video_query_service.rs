use chrono::{DateTime, Utc};

use crate::http::error::{AppError, AppResult};
use crate::infrastructure::postgres::{VideoDetailRecord, VideoRepository, VideoSummaryRecord};

#[derive(Clone)]
pub struct VideoQueryService {
    repository: VideoRepository,
    max_recent_video_limit: usize,
}

impl VideoQueryService {
    pub fn new(repository: VideoRepository) -> Self {
        Self {
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

        Ok(map_video_details(video))
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

fn map_video_details(video: VideoDetailRecord) -> VideoDetailsResult {
    VideoDetailsResult {
        public_id: video.public_id.clone(),
        title: video.title,
        original_filename: video.original_filename,
        status: video.status,
        is_streamable: video.is_streamable,
        created_at: video.created_at,
        updated_at: video.updated_at,
        playback_path: format!("/v/{}", video.public_id),
        manifest_url: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_page_number, normalize_recent_video_limit};

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
}
