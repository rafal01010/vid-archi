use std::time::Duration;

use aws_config::BehaviorVersion;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::Client;
use uuid::Uuid;

use crate::http::error::{AppError, AppResult};
use crate::infrastructure::config::AppConfig;

#[derive(Clone)]
pub struct ObjectStorage {
    client: Client,
    upload_bucket: String,
    presign_ttl_seconds: u64,
}

impl ObjectStorage {
    pub async fn new(config: &AppConfig) -> AppResult<Self> {
        let region = Region::new(config.aws_region.clone());

        let shared_config = if let (Some(access_key_id), Some(secret_access_key)) = (
            config.s3_access_key_id.clone(),
            config.s3_secret_access_key.clone(),
        ) {
            let credentials =
                Credentials::new(access_key_id, secret_access_key, None, None, "local-minio");

            aws_config::defaults(BehaviorVersion::latest())
                .region(region.clone())
                .credentials_provider(SharedCredentialsProvider::new(credentials))
                .load()
                .await
        } else {
            aws_config::defaults(BehaviorVersion::latest())
                .region(region.clone())
                .load()
                .await
        };

        let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&shared_config)
            .region(region)
            .force_path_style(config.s3_force_path_style);

        if let Some(endpoint_url) = &config.s3_endpoint_url {
            s3_config_builder = s3_config_builder.endpoint_url(endpoint_url);
        }

        Ok(Self {
            client: Client::from_conf(s3_config_builder.build()),
            upload_bucket: config.upload_bucket.clone(),
            presign_ttl_seconds: config.presign_ttl_seconds,
        })
    }

    pub async fn create_multipart_upload(
        &self,
        object_key: &str,
        content_type: &str,
        video_id: &Uuid,
        public_id: &str,
        original_filename: &str,
    ) -> AppResult<String> {
        let response = self
            .client
            .create_multipart_upload()
            .bucket(&self.upload_bucket)
            .key(object_key)
            .content_type(content_type)
            .metadata("video-id", video_id.to_string())
            .metadata("public-id", public_id)
            .metadata("original-filename", original_filename)
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to create multipart upload",
                    error.to_string(),
                )
            })?;

        response.upload_id().map(str::to_owned).ok_or_else(|| {
            AppError::internal("S3 multipart upload response did not include an upload id")
        })
    }

    pub async fn abort_multipart_upload(&self, object_key: &str, upload_id: &str) -> AppResult<()> {
        self.client
            .abort_multipart_upload()
            .bucket(&self.upload_bucket)
            .key(object_key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to abort multipart upload",
                    error.to_string(),
                )
            })?;

        Ok(())
    }

    pub async fn sign_upload_parts(
        &self,
        object_key: &str,
        upload_id: &str,
        part_numbers: &[i32],
    ) -> AppResult<Vec<PresignedUploadPart>> {
        let mut signed_parts = Vec::with_capacity(part_numbers.len());

        for part_number in part_numbers {
            let presigning_config =
                PresigningConfig::expires_in(Duration::from_secs(self.presign_ttl_seconds))
                    .map_err(|error| {
                        AppError::internal_with_context(
                            "failed to build S3 presigning configuration",
                            error.to_string(),
                        )
                    })?;

            let presigned_request = self
                .client
                .upload_part()
                .bucket(&self.upload_bucket)
                .key(object_key)
                .upload_id(upload_id)
                .part_number(*part_number)
                .presigned(presigning_config)
                .await
                .map_err(|error| {
                    AppError::internal_with_context(
                        "failed to presign multipart upload part",
                        error.to_string(),
                    )
                })?;

            signed_parts.push(PresignedUploadPart {
                part_number: *part_number,
                url: presigned_request.uri().to_string(),
            });
        }

        Ok(signed_parts)
    }

    pub async fn complete_multipart_upload(
        &self,
        object_key: &str,
        upload_id: &str,
        parts: &[CompletedUploadPart],
    ) -> AppResult<()> {
        let completed_parts = parts
            .iter()
            .map(|part| {
                CompletedPart::builder()
                    .part_number(part.part_number)
                    .e_tag(part.etag.clone())
                    .build()
            })
            .collect::<Vec<_>>();

        self.client
            .complete_multipart_upload()
            .bucket(&self.upload_bucket)
            .key(object_key)
            .upload_id(upload_id)
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .set_parts(Some(completed_parts))
                    .build(),
            )
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to complete multipart upload",
                    error.to_string(),
                )
            })?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PresignedUploadPart {
    pub part_number: i32,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct CompletedUploadPart {
    pub part_number: i32,
    pub etag: String,
}
