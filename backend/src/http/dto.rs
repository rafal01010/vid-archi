use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::{
    CompleteUploadCommand, CompletedUploadPartInput, CreateVideoUploadCommand,
    CreateVideoUploadResult, DeleteVideoCommand, DeleteVideoResult, ListRecentVideosResult,
    SignUploadPartsCommand, SignUploadPartsResult, VideoDetailsResult, VideoPlaybackResult,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVideoUploadRequest {
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub title: Option<String>,
    pub delete_code: String,
}

impl From<CreateVideoUploadRequest> for CreateVideoUploadCommand {
    fn from(value: CreateVideoUploadRequest) -> Self {
        Self {
            filename: value.filename,
            content_type: value.content_type,
            size_bytes: value.size_bytes,
            title: value.title,
            delete_code: value.delete_code,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVideoUploadResponse {
    pub video_id: Uuid,
    pub public_id: String,
    pub upload_session_id: Uuid,
    pub s3_upload_id: String,
    pub source_object_key: String,
    pub part_size_bytes: i64,
    pub upload_expires_at: DateTime<Utc>,
    pub sign_parts_endpoint: String,
    pub complete_upload_endpoint: String,
    pub playback_path: String,
}

impl From<CreateVideoUploadResult> for CreateVideoUploadResponse {
    fn from(value: CreateVideoUploadResult) -> Self {
        Self {
            video_id: value.video_id,
            public_id: value.public_id,
            upload_session_id: value.upload_session_id,
            s3_upload_id: value.s3_upload_id,
            source_object_key: value.source_object_key,
            part_size_bytes: value.part_size_bytes,
            upload_expires_at: value.upload_expires_at,
            sign_parts_endpoint: value.sign_parts_endpoint,
            complete_upload_endpoint: value.complete_upload_endpoint,
            playback_path: value.playback_path,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignUploadPartsRequest {
    pub upload_session_id: Uuid,
    pub part_numbers: Vec<i32>,
}

impl From<SignUploadPartsRequest> for SignUploadPartsCommand {
    fn from(value: SignUploadPartsRequest) -> Self {
        Self {
            upload_session_id: value.upload_session_id,
            part_numbers: value.part_numbers,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedPartResponse {
    pub part_number: i32,
    pub url: String,
    pub method: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignUploadPartsResponse {
    pub video_id: Uuid,
    pub upload_session_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub parts: Vec<SignedPartResponse>,
}

impl From<SignUploadPartsResult> for SignUploadPartsResponse {
    fn from(value: SignUploadPartsResult) -> Self {
        Self {
            video_id: value.video_id,
            upload_session_id: value.upload_session_id,
            expires_at: value.expires_at,
            parts: value
                .parts
                .into_iter()
                .map(|part| SignedPartResponse {
                    part_number: part.part_number,
                    url: part.url,
                    method: "PUT",
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteUploadRequest {
    pub upload_session_id: Uuid,
    pub parts: Vec<CompletedPartRequest>,
}

impl From<CompleteUploadRequest> for CompleteUploadCommand {
    fn from(value: CompleteUploadRequest) -> Self {
        Self {
            upload_session_id: value.upload_session_id,
            parts: value
                .parts
                .into_iter()
                .map(|part| CompletedUploadPartInput {
                    part_number: part.part_number,
                    etag: part.etag,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedPartRequest {
    pub part_number: i32,
    pub etag: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteUploadResponse {
    pub video_id: Uuid,
    pub public_id: String,
    pub status: String,
    pub processing_job_id: Uuid,
    pub playback_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteVideoRequest {
    pub delete_code: String,
}

impl From<DeleteVideoRequest> for DeleteVideoCommand {
    fn from(value: DeleteVideoRequest) -> Self {
        Self {
            delete_code: value.delete_code,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteVideoResponse {
    pub public_id: String,
    pub deleted_upload_object_count: u64,
    pub deleted_processed_object_count: u64,
}

impl From<DeleteVideoResult> for DeleteVideoResponse {
    fn from(value: DeleteVideoResult) -> Self {
        Self {
            public_id: value.public_id,
            deleted_upload_object_count: value.deleted_upload_object_count,
            deleted_processed_object_count: value.deleted_processed_object_count,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSummaryResponse {
    pub public_id: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub status: String,
    pub is_streamable: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub playback_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListRecentVideosResponse {
    pub videos: Vec<VideoSummaryResponse>,
    pub page: usize,
    pub page_size: usize,
    pub total_count: i64,
    pub total_pages: usize,
    pub has_previous_page: bool,
    pub has_next_page: bool,
}

impl From<ListRecentVideosResult> for ListRecentVideosResponse {
    fn from(value: ListRecentVideosResult) -> Self {
        Self {
            videos: value
                .videos
                .into_iter()
                .map(|video| VideoSummaryResponse {
                    public_id: video.public_id,
                    title: video.title,
                    original_filename: video.original_filename,
                    status: video.status,
                    is_streamable: video.is_streamable,
                    created_at: video.created_at,
                    updated_at: video.updated_at,
                    playback_path: video.playback_path,
                })
                .collect(),
            page: value.page,
            page_size: value.page_size,
            total_count: value.total_count,
            total_pages: value.total_pages,
            has_previous_page: value.has_previous_page,
            has_next_page: value.has_next_page,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoDetailsResponse {
    pub public_id: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub status: String,
    pub is_streamable: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub playback_path: String,
    pub manifest_url: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

impl From<VideoDetailsResult> for VideoDetailsResponse {
    fn from(value: VideoDetailsResult) -> Self {
        Self {
            public_id: value.public_id,
            title: value.title,
            original_filename: value.original_filename,
            status: value.status,
            is_streamable: value.is_streamable,
            created_at: value.created_at,
            updated_at: value.updated_at,
            playback_path: value.playback_path,
            manifest_url: value.manifest_url,
            error_code: value.error_code,
            error_message: value.error_message,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackQualityResponse {
    pub name: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub codec: String,
    pub container: String,
    pub playlist_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPlaybackResponse {
    pub public_id: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub status: String,
    pub is_streamable: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub baseline_ready_at: Option<DateTime<Utc>>,
    pub processing_completed_at: Option<DateTime<Utc>>,
    pub playback_path: String,
    pub manifest_url: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub default_quality: String,
    pub poll_interval_ms: u64,
    pub available_qualities: Vec<PlaybackQualityResponse>,
}

impl From<VideoPlaybackResult> for VideoPlaybackResponse {
    fn from(value: VideoPlaybackResult) -> Self {
        Self {
            public_id: value.public_id,
            title: value.title,
            original_filename: value.original_filename,
            status: value.status,
            is_streamable: value.is_streamable,
            created_at: value.created_at,
            updated_at: value.updated_at,
            baseline_ready_at: value.baseline_ready_at,
            processing_completed_at: value.processing_completed_at,
            playback_path: value.playback_path,
            manifest_url: value.manifest_url,
            error_code: value.error_code,
            error_message: value.error_message,
            default_quality: value.default_quality,
            poll_interval_ms: value.poll_interval_ms,
            available_qualities: value
                .available_qualities
                .into_iter()
                .map(|quality| PlaybackQualityResponse {
                    name: quality.name,
                    label: quality.label,
                    width: quality.width,
                    height: quality.height,
                    codec: quality.codec,
                    container: quality.container,
                    playlist_url: quality.playlist_url,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckResponse {
    pub status: &'static str,
}
