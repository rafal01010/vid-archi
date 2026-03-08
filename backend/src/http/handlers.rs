use axum::extract::{Path, State};
use axum::{Json, http::StatusCode};
use uuid::Uuid;

use crate::app::AppState;
use crate::http::dto::{
    CompleteUploadRequest, CompleteUploadResponse, CreateVideoUploadRequest,
    CreateVideoUploadResponse, HealthCheckResponse, SignUploadPartsRequest,
    SignUploadPartsResponse,
};
use crate::http::error::AppResult;

pub async fn health_check() -> Json<HealthCheckResponse> {
    Json(HealthCheckResponse { status: "ok" })
}

pub async fn create_video_upload(
    State(state): State<AppState>,
    Json(request): Json<CreateVideoUploadRequest>,
) -> AppResult<(StatusCode, Json<CreateVideoUploadResponse>)> {
    let response = state
        .upload_service
        .create_video_upload(request.into())
        .await?;

    Ok((StatusCode::CREATED, Json(response.into())))
}

pub async fn sign_upload_parts(
    State(state): State<AppState>,
    Path(video_id): Path<Uuid>,
    Json(request): Json<SignUploadPartsRequest>,
) -> AppResult<Json<SignUploadPartsResponse>> {
    let response = state
        .upload_service
        .sign_upload_parts(video_id, request.into())
        .await?;

    Ok(Json(response.into()))
}

pub async fn complete_video_upload(
    State(state): State<AppState>,
    Path(video_id): Path<Uuid>,
    Json(request): Json<CompleteUploadRequest>,
) -> AppResult<Json<CompleteUploadResponse>> {
    let response = state
        .upload_service
        .complete_upload(video_id, request.into())
        .await?;

    Ok(Json(CompleteUploadResponse {
        video_id: response.video_id,
        public_id: response.public_id,
        status: response.status,
        processing_job_id: response.processing_job_id,
        playback_path: response.playback_path,
    }))
}
