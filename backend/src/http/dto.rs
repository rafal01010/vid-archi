use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::{
    CompleteUploadCommand, CompletedUploadPartInput, CreateVideoUploadCommand,
    CreateVideoUploadResult, SignUploadPartsCommand, SignUploadPartsResult,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVideoUploadRequest {
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub title: Option<String>,
}

impl From<CreateVideoUploadRequest> for CreateVideoUploadCommand {
    fn from(value: CreateVideoUploadRequest) -> Self {
        Self {
            filename: value.filename,
            content_type: value.content_type,
            size_bytes: value.size_bytes,
            title: value.title,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckResponse {
    pub status: &'static str,
}
