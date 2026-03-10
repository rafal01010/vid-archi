use std::sync::Arc;

use crate::http::error::{AppError, AppResult};
use crate::infrastructure::config::AppConfig;
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::{DeletedVideoRecord, VideoRepository};

#[derive(Clone)]
pub struct VideoDeleteService {
    config: Arc<AppConfig>,
    repository: VideoRepository,
    object_storage: ObjectStorage,
}

impl VideoDeleteService {
    pub fn new(
        config: AppConfig,
        repository: VideoRepository,
        object_storage: ObjectStorage,
    ) -> Self {
        Self {
            config: Arc::new(config),
            repository,
            object_storage,
        }
    }

    pub async fn delete_video(
        &self,
        public_id: &str,
        command: DeleteVideoCommand,
    ) -> AppResult<DeleteVideoResult> {
        let normalized_public_id = normalize_public_id(public_id)?;
        let normalized_delete_code = normalize_delete_code(command.delete_code)?;
        let delete_context = self
            .repository
            .find_video_delete_context(&normalized_public_id)
            .await?
            .ok_or_else(|| AppError::not_found("video not found"))?;

        if !self
            .repository
            .verify_delete_code(delete_context.id, &normalized_delete_code)
            .await?
        {
            return Err(AppError::forbidden("delete code is incorrect"));
        }

        reject_delete_for_active_processing(&delete_context.status)?;

        if let Some(active_upload_id) = delete_context.active_upload_id.as_deref() {
            self.object_storage
                .abort_multipart_upload_if_exists(&delete_context.source_s3_key, active_upload_id)
                .await;
        }

        let deleted_upload_object_count = self
            .object_storage
            .delete_video_assets_in_upload_bucket(delete_context.id)
            .await?;
        let deleted_processed_object_count = self
            .object_storage
            .delete_video_assets_in_processed_bucket(delete_context.id)
            .await?;
        let deleted_video = self.repository.delete_video(delete_context.id).await?;

        tracing::info!(
            video_id = %delete_context.id,
            public_id = %deleted_video.public_id,
            status = %delete_context.status,
            environment = %self.config.app_env,
            deleted_upload_object_count,
            deleted_processed_object_count,
            "deleted video and storage artifacts"
        );

        Ok(map_deleted_video(
            deleted_video,
            deleted_upload_object_count,
            deleted_processed_object_count,
        ))
    }
}

#[derive(Debug)]
pub struct DeleteVideoCommand {
    pub delete_code: String,
}

#[derive(Debug)]
pub struct DeleteVideoResult {
    pub public_id: String,
    pub deleted_upload_object_count: u64,
    pub deleted_processed_object_count: u64,
}

fn normalize_public_id(public_id: &str) -> AppResult<String> {
    let trimmed_public_id = public_id.trim();

    if trimmed_public_id.is_empty() {
        return Err(AppError::bad_request("public id is required"));
    }

    Ok(trimmed_public_id.to_owned())
}

fn normalize_delete_code(delete_code: String) -> AppResult<String> {
    let trimmed_delete_code = delete_code.trim();

    if trimmed_delete_code.len() < 6 {
        return Err(AppError::bad_request(
            "delete code must be at least 6 characters long",
        ));
    }

    if trimmed_delete_code.len() > 128 {
        return Err(AppError::bad_request(
            "delete code must be 128 characters or fewer",
        ));
    }

    Ok(trimmed_delete_code.to_owned())
}

fn reject_delete_for_active_processing(status: &str) -> AppResult<()> {
    if matches!(status, "PROCESSING_BASELINE") {
        return Err(AppError::conflict(
            "video is still preparing its first playable stream; try deleting it again after playback becomes available",
        ));
    }

    Ok(())
}

fn map_deleted_video(
    deleted_video: DeletedVideoRecord,
    deleted_upload_object_count: u64,
    deleted_processed_object_count: u64,
) -> DeleteVideoResult {
    DeleteVideoResult {
        public_id: deleted_video.public_id,
        deleted_upload_object_count,
        deleted_processed_object_count,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_delete_code, normalize_public_id, reject_delete_for_active_processing};

    #[test]
    fn normalize_public_id_rejects_blank_value() {
        let result = normalize_public_id("   ");

        assert!(result.is_err());
    }

    #[test]
    fn normalize_delete_code_rejects_short_value() {
        let result = normalize_delete_code("12345".to_owned());

        assert!(result.is_err());
    }

    #[test]
    fn normalize_delete_code_trims_whitespace() {
        let result = normalize_delete_code("  delete-me  ".to_owned()).expect("delete code");

        assert_eq!(result, "delete-me");
    }

    #[test]
    fn reject_delete_for_active_processing_blocks_processing_states() {
        let result = reject_delete_for_active_processing("PROCESSING_BASELINE");

        assert!(result.is_err());
    }

    #[test]
    fn reject_delete_for_active_processing_allows_ready_state() {
        let result = reject_delete_for_active_processing("READY");

        assert!(result.is_ok());
    }

    #[test]
    fn reject_delete_for_active_processing_allows_processing_full_state() {
        let result = reject_delete_for_active_processing("PROCESSING_FULL");

        assert!(result.is_ok());
    }
}
