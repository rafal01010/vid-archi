use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::domain::public_id::PublicIdGenerator;
use crate::domain::video_policy::VideoPolicy;
use crate::http::error::{AppError, AppResult};
use crate::infrastructure::config::AppConfig;
use crate::infrastructure::object_storage::{
    CompletedUploadPart, ObjectStorage, PresignedUploadPart,
};
use crate::infrastructure::postgres::{
    CreateUploadSessionRecord, CreateVideoRecord, FinalizedUploadRecord, UploadSessionContext,
    VideoRepository,
};

#[derive(Clone)]
pub struct UploadService {
    config: Arc<AppConfig>,
    policy: Arc<VideoPolicy>,
    repository: VideoRepository,
    object_storage: ObjectStorage,
    public_id_generator: PublicIdGenerator,
}

impl UploadService {
    pub fn new(
        config: AppConfig,
        policy: VideoPolicy,
        repository: VideoRepository,
        object_storage: ObjectStorage,
    ) -> Self {
        Self {
            config: Arc::new(config),
            policy: Arc::new(policy),
            repository,
            object_storage,
            public_id_generator: PublicIdGenerator::default(),
        }
    }

    pub async fn create_video_upload(
        &self,
        command: CreateVideoUploadCommand,
    ) -> AppResult<CreateVideoUploadResult> {
        let normalized_filename = sanitize_filename(&command.filename)?;
        let normalized_content_type = command.content_type.trim().to_ascii_lowercase();
        let normalized_title = normalize_optional_text(command.title);

        self.policy.validate_upload(
            &normalized_filename,
            &normalized_content_type,
            command.size_bytes,
        )?;

        let video_id = Uuid::new_v4();
        let public_id = self
            .resolve_public_id(video_id, normalized_title.as_deref(), &normalized_filename)
            .await?;
        let upload_session_id = Uuid::new_v4();
        let source_object_key = format!("videos/{video_id}/source/original");
        let upload_expires_at = Utc::now() + Duration::seconds(self.config.upload_session_ttl_seconds);

        let s3_upload_id = match self
            .object_storage
            .create_multipart_upload(
                &source_object_key,
                &normalized_content_type,
                &video_id,
                &public_id,
                &normalized_filename,
            )
            .await
        {
            Ok(upload_id) => upload_id,
            Err(error) => {
                tracing::error!(%video_id, %public_id, error = %error, "failed to initialize multipart upload");
                return Err(error);
            }
        };

        let video_record = CreateVideoRecord {
            id: video_id,
            public_id: public_id.clone(),
            title: normalized_title.clone(),
            original_filename: normalized_filename.clone(),
            content_type: normalized_content_type.clone(),
            size_bytes: command.size_bytes,
            source_s3_key: source_object_key.clone(),
        };

        let upload_session_record = CreateUploadSessionRecord {
            id: upload_session_id,
            video_id,
            s3_upload_id: s3_upload_id.clone(),
            part_size_bytes: self.config.multipart_part_size_bytes,
            expires_at: upload_expires_at,
        };

        if let Err(error) = self
            .repository
            .create_video_and_upload_session(video_record, upload_session_record)
            .await
        {
            tracing::warn!(
                %video_id,
                %public_id,
                %upload_session_id,
                error = %error,
                "rolling back multipart upload after database failure"
            );

            if let Err(abort_error) = self
                .object_storage
                .abort_multipart_upload(&source_object_key, &s3_upload_id)
                .await
            {
                tracing::error!(
                    %video_id,
                    %public_id,
                    %upload_session_id,
                    error = %abort_error,
                    "failed to abort multipart upload during rollback"
                );
            }

            return Err(error);
        }

        Ok(CreateVideoUploadResult {
            video_id,
            public_id: public_id.clone(),
            upload_session_id,
            s3_upload_id,
            source_object_key,
            part_size_bytes: self.config.multipart_part_size_bytes,
            upload_expires_at,
            sign_parts_endpoint: format!("/api/videos/{video_id}/parts/sign"),
            complete_upload_endpoint: format!("/api/videos/{video_id}/complete"),
            playback_path: format!("/v/{public_id}"),
        })
    }

    pub async fn sign_upload_parts(
        &self,
        video_id: Uuid,
        command: SignUploadPartsCommand,
    ) -> AppResult<SignUploadPartsResult> {
        let upload_session = self
            .get_active_upload_session(video_id, command.upload_session_id)
            .await?;
        let part_numbers = normalize_part_numbers(command.part_numbers)?;
        validate_part_window(&upload_session, &part_numbers)?;

        self.repository.mark_video_uploading(video_id).await?;

        let signed_parts = self
            .object_storage
            .sign_upload_parts(
                &upload_session.source_s3_key,
                &upload_session.s3_upload_id,
                &part_numbers,
            )
            .await?;

        Ok(SignUploadPartsResult {
            video_id,
            upload_session_id: upload_session.upload_session_id,
            expires_at: upload_session.expires_at,
            parts: signed_parts,
        })
    }

    pub async fn complete_upload(
        &self,
        video_id: Uuid,
        command: CompleteUploadCommand,
    ) -> AppResult<CompleteUploadResult> {
        let upload_session = self
            .get_upload_session(video_id, command.upload_session_id)
            .await?;

        if upload_session.upload_session_status == "COMPLETED" {
            let finalized_upload = self
                .repository
                .get_finalized_upload(video_id)
                .await?
                .ok_or_else(|| AppError::internal("missing finalized upload after completed session"))?;

            return Ok(map_finalized_upload(finalized_upload));
        }

        if upload_session.upload_session_status != "OPEN" {
            return Err(AppError::conflict(
                "upload session is not open and cannot be completed",
            ));
        }

        if upload_session.expires_at < Utc::now() {
            return Err(AppError::conflict("upload session has expired"));
        }

        let completed_parts = normalize_completed_parts(command.parts)?;
        validate_part_window(
            &upload_session,
            &completed_parts
                .iter()
                .map(|part| part.part_number)
                .collect::<Vec<_>>(),
        )?;

        self.object_storage
            .complete_multipart_upload(
                &upload_session.source_s3_key,
                &upload_session.s3_upload_id,
                &completed_parts,
            )
            .await?;

        let finalized_upload = self
            .repository
            .finalize_completed_upload(video_id, command.upload_session_id)
            .await?;

        Ok(map_finalized_upload(finalized_upload))
    }

    async fn resolve_public_id(
        &self,
        video_id: Uuid,
        title: Option<&str>,
        filename: &str,
    ) -> AppResult<String> {
        let candidate = self
            .public_id_generator
            .generate(video_id, title, filename)
            .map_err(AppError::bad_request)?;

        if self.repository.public_id_exists(&candidate).await? {
            return Err(AppError::conflict(
                "generated public share id already exists; regenerate the upload session",
            ));
        }

        Ok(candidate)
    }

    async fn get_upload_session(
        &self,
        video_id: Uuid,
        upload_session_id: Uuid,
    ) -> AppResult<UploadSessionContext> {
        self.repository
            .find_upload_session(video_id, upload_session_id)
            .await?
            .ok_or_else(|| AppError::not_found("upload session not found"))
    }

    async fn get_active_upload_session(
        &self,
        video_id: Uuid,
        upload_session_id: Uuid,
    ) -> AppResult<UploadSessionContext> {
        let upload_session = self.get_upload_session(video_id, upload_session_id).await?;

        if upload_session.upload_session_status != "OPEN" {
            return Err(AppError::conflict(
                "upload session is not open and cannot sign new parts",
            ));
        }

        if upload_session.expires_at < Utc::now() {
            return Err(AppError::conflict("upload session has expired"));
        }

        Ok(upload_session)
    }
}

#[derive(Debug)]
pub struct CreateVideoUploadCommand {
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub title: Option<String>,
}

#[derive(Debug)]
pub struct SignUploadPartsCommand {
    pub upload_session_id: Uuid,
    pub part_numbers: Vec<i32>,
}

#[derive(Debug)]
pub struct CompleteUploadCommand {
    pub upload_session_id: Uuid,
    pub parts: Vec<CompletedUploadPartInput>,
}

#[derive(Debug)]
pub struct CompletedUploadPartInput {
    pub part_number: i32,
    pub etag: String,
}

#[derive(Debug)]
pub struct CreateVideoUploadResult {
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

#[derive(Debug)]
pub struct SignUploadPartsResult {
    pub video_id: Uuid,
    pub upload_session_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub parts: Vec<PresignedUploadPart>,
}

#[derive(Debug)]
pub struct CompleteUploadResult {
    pub video_id: Uuid,
    pub public_id: String,
    pub status: String,
    pub processing_job_id: Uuid,
    pub playback_path: String,
}

fn sanitize_filename(filename: &str) -> AppResult<String> {
    let trimmed_filename = filename.trim();

    if trimmed_filename.is_empty() {
        return Err(AppError::bad_request("filename is required"));
    }

    let sanitized_filename = std::path::Path::new(trimmed_filename)
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("filename must contain a valid file name"))?;

    Ok(sanitized_filename.to_owned())
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn normalize_part_numbers(part_numbers: Vec<i32>) -> AppResult<Vec<i32>> {
    if part_numbers.is_empty() {
        return Err(AppError::bad_request("at least one part number is required"));
    }

    let mut normalized_part_numbers = part_numbers;
    normalized_part_numbers.sort_unstable();
    normalized_part_numbers.dedup();

    if normalized_part_numbers.iter().any(|part_number| *part_number <= 0) {
        return Err(AppError::bad_request("part numbers must be positive integers"));
    }

    Ok(normalized_part_numbers)
}

fn normalize_completed_parts(
    parts: Vec<CompletedUploadPartInput>,
) -> AppResult<Vec<CompletedUploadPart>> {
    if parts.is_empty() {
        return Err(AppError::bad_request("at least one completed part is required"));
    }

    let mut normalized_parts = parts
        .into_iter()
        .map(|part| {
            let normalized_etag = part.etag.trim();

            if part.part_number <= 0 {
                return Err(AppError::bad_request(
                    "part numbers must be positive integers",
                ));
            }

            if normalized_etag.is_empty() {
                return Err(AppError::bad_request("etag is required for every part"));
            }

            Ok(CompletedUploadPart {
                part_number: part.part_number,
                etag: normalized_etag.to_owned(),
            })
        })
        .collect::<AppResult<Vec<_>>>()?;

    normalized_parts.sort_by_key(|part| part.part_number);

    if normalized_parts
        .windows(2)
        .any(|window| window[0].part_number == window[1].part_number)
    {
        return Err(AppError::bad_request("duplicate part numbers are not allowed"));
    }

    Ok(normalized_parts)
}

fn validate_part_window(
    upload_session: &UploadSessionContext,
    part_numbers: &[i32],
) -> AppResult<()> {
    let max_part_count = ((upload_session.size_bytes + upload_session.part_size_bytes - 1)
        / upload_session.part_size_bytes) as i32;

    if part_numbers.iter().any(|part_number| *part_number > max_part_count) {
        return Err(AppError::bad_request(
            "requested part number exceeds the expected multipart window",
        ));
    }

    Ok(())
}

fn map_finalized_upload(finalized_upload: FinalizedUploadRecord) -> CompleteUploadResult {
    CompleteUploadResult {
        video_id: finalized_upload.video_id,
        public_id: finalized_upload.public_id.clone(),
        status: finalized_upload.video_status,
        processing_job_id: finalized_upload.processing_job_id,
        playback_path: format!("/v/{}", finalized_upload.public_id),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::{
        CompletedUploadPartInput, normalize_completed_parts, normalize_optional_text,
        normalize_part_numbers, sanitize_filename, validate_part_window,
    };
    use crate::infrastructure::postgres::UploadSessionContext;

    #[test]
    fn sanitize_filename_strips_path_components() {
        let sanitized_filename =
            sanitize_filename("C:\\\\fakepath\\\\launch-demo.mp4").expect("sanitized filename");

        assert_eq!(sanitized_filename, "launch-demo.mp4");
    }

    #[test]
    fn normalize_optional_text_trims_whitespace() {
        let normalized = normalize_optional_text(Some("  demo title  ".to_owned()));

        assert_eq!(normalized.as_deref(), Some("demo title"));
    }

    #[test]
    fn normalize_part_numbers_sorts_and_deduplicates() {
        let normalized_part_numbers =
            normalize_part_numbers(vec![3, 1, 2, 2]).expect("normalized part numbers");

        assert_eq!(normalized_part_numbers, vec![1, 2, 3]);
    }

    #[test]
    fn normalize_completed_parts_rejects_duplicate_parts() {
        let result = normalize_completed_parts(vec![
            CompletedUploadPartInput {
                part_number: 1,
                etag: "etag-a".to_owned(),
            },
            CompletedUploadPartInput {
                part_number: 1,
                etag: "etag-b".to_owned(),
            },
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn validate_part_window_rejects_parts_beyond_expected_range() {
        let upload_session = UploadSessionContext {
            video_id: Uuid::new_v4(),
            public_id: "demo".to_owned(),
            video_status: "INITIATED".to_owned(),
            size_bytes: 8_388_608,
            source_s3_key: "videos/demo/source/original".to_owned(),
            upload_session_id: Uuid::new_v4(),
            s3_upload_id: "upload-id".to_owned(),
            part_size_bytes: 8_388_608,
            expires_at: Utc::now(),
            upload_session_status: "OPEN".to_owned(),
        };

        let result = validate_part_window(&upload_session, &[2]);

        assert!(result.is_err());
    }
}
