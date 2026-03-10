use std::path::Path;
use std::time::Duration;

use aws_config::BehaviorVersion;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{Delete, ObjectIdentifier};
use aws_sdk_s3::Client;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::error::{AppError, AppResult};
use crate::infrastructure::config::TranscoderConfig;

#[derive(Clone)]
pub struct ObjectStorage {
    client: Client,
    aws_region: String,
    upload_bucket: String,
    processed_bucket: String,
    enable_aws_cli_s3_fallback: bool,
}

const PROCESSED_ARTIFACT_UPLOAD_TIMEOUT: Duration = Duration::from_secs(90);
const PROCESSED_ARTIFACT_UPLOAD_ATTEMPTS: u32 = 3;
const DELETE_PREFIX_TIMEOUT: Duration = Duration::from_secs(90);

impl ObjectStorage {
    pub async fn new(config: &TranscoderConfig) -> AppResult<Self> {
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
            aws_region: config.aws_region.clone(),
            upload_bucket: config.upload_bucket.clone(),
            processed_bucket: config.processed_bucket.clone(),
            enable_aws_cli_s3_fallback: config.enable_aws_cli_s3_fallback,
        })
    }

    pub async fn download_source_object(
        &self,
        object_key: &str,
        destination: &Path,
    ) -> AppResult<()> {
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let response = self
            .client
            .get_object()
            .bucket(&self.upload_bucket)
            .key(object_key)
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to download source object",
                    format!(
                        "bucket={} key={} error={error}",
                        self.upload_bucket, object_key
                    ),
                )
            })?;

        let mut file = tokio::fs::File::create(destination).await?;
        let mut body = response.body.into_async_read();
        tokio::io::copy(&mut body, &mut file).await?;
        file.flush().await?;

        Ok(())
    }

    pub async fn upload_processing_directory(
        &self,
        object_prefix: &str,
        local_directory: &Path,
    ) -> AppResult<()> {
        let mut files = collect_files(local_directory)?;
        files.sort_by_key(|path| upload_priority(path));
        tracing::info!(
            bucket = %self.processed_bucket,
            object_prefix,
            file_count = files.len(),
            local_directory = %local_directory.display(),
            "starting processed-artifact directory upload"
        );

        for local_path in files {
            let relative_path = local_path.strip_prefix(local_directory).map_err(|error| {
                AppError::internal_with_context(
                    "failed to compute processing artifact relative path",
                    format!(
                        "directory={} file={} error={error}",
                        local_directory.display(),
                        local_path.display()
                    ),
                )
            })?;
            let object_key = format!(
                "{}/{}",
                object_prefix.trim_end_matches('/'),
                relative_path.to_string_lossy().replace('\\', "/")
            );

            self.upload_processing_file(&object_key, &local_path)
                .await?;
        }

        Ok(())
    }

    pub async fn upload_processing_file(
        &self,
        object_key: &str,
        local_path: &Path,
    ) -> AppResult<()> {
        let file_size_bytes = tokio::fs::metadata(local_path)
            .await
            .map(|metadata| metadata.len())
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to stat processing artifact before upload",
                    format!("file={} error={error}", local_path.display()),
                )
            })?;
        let file_bytes = tokio::fs::read(local_path).await.map_err(|error| {
            AppError::internal_with_context(
                "failed to read processing artifact before upload",
                format!("file={} error={error}", local_path.display()),
            )
        })?;

        let mut last_error = None;

        for attempt in 1..=PROCESSED_ARTIFACT_UPLOAD_ATTEMPTS {
            tracing::info!(
                attempt,
                max_attempts = PROCESSED_ARTIFACT_UPLOAD_ATTEMPTS,
                bucket = %self.processed_bucket,
                key = object_key,
                file = %local_path.display(),
                file_size_bytes,
                "uploading processed artifact"
            );

            let body = ByteStream::from(file_bytes.clone());

            let upload = self
                .client
                .put_object()
                .bucket(&self.processed_bucket)
                .key(object_key)
                .content_type(content_type_for_path(local_path))
                .content_length(file_size_bytes as i64)
                .body(body)
                .send();

            match tokio::time::timeout(PROCESSED_ARTIFACT_UPLOAD_TIMEOUT, upload).await {
                Ok(Ok(_)) => {
                    tracing::info!(
                        attempt,
                        bucket = %self.processed_bucket,
                        key = object_key,
                        file = %local_path.display(),
                        file_size_bytes,
                        "uploaded processed artifact"
                    );
                    return Ok(());
                }
                Ok(Err(error)) => {
                    let error_message = format!("{error:?}");
                    if self
                        .try_upload_processing_file_with_aws_cli(
                            object_key,
                            local_path,
                            file_size_bytes,
                            attempt,
                            Some(error_message.as_str()),
                        )
                        .await?
                    {
                        return Ok(());
                    }
                    last_error = Some(format!(
                        "bucket={} key={} file={} attempt={} error={}",
                        self.processed_bucket,
                        object_key,
                        local_path.display(),
                        attempt,
                        error_message
                    ));
                    if attempt < PROCESSED_ARTIFACT_UPLOAD_ATTEMPTS {
                        tracing::warn!(
                            attempt,
                            max_attempts = PROCESSED_ARTIFACT_UPLOAD_ATTEMPTS,
                            bucket = %self.processed_bucket,
                            key = object_key,
                            file = %local_path.display(),
                            error = %error_message,
                            "processed artifact upload failed; retrying"
                        );
                    }
                }
                Err(_) => {
                    if self
                        .try_upload_processing_file_with_aws_cli(
                            object_key,
                            local_path,
                            file_size_bytes,
                            attempt,
                            None,
                        )
                        .await?
                    {
                        return Ok(());
                    }
                    last_error = Some(format!(
                        "bucket={} key={} file={} attempt={} error=timed out after {}s",
                        self.processed_bucket,
                        object_key,
                        local_path.display(),
                        attempt,
                        PROCESSED_ARTIFACT_UPLOAD_TIMEOUT.as_secs()
                    ));
                    if attempt < PROCESSED_ARTIFACT_UPLOAD_ATTEMPTS {
                        tracing::warn!(
                            attempt,
                            max_attempts = PROCESSED_ARTIFACT_UPLOAD_ATTEMPTS,
                            bucket = %self.processed_bucket,
                            key = object_key,
                            file = %local_path.display(),
                            timeout_seconds = PROCESSED_ARTIFACT_UPLOAD_TIMEOUT.as_secs(),
                            "processed artifact upload timed out; retrying"
                        );
                    }
                }
            }
        }

        Err(AppError::internal_with_context(
            "failed to upload processing artifact",
            last_error.unwrap_or_else(|| {
                format!(
                    "bucket={} key={} file={} error=upload attempts exhausted without additional context",
                    self.processed_bucket,
                    object_key,
                    local_path.display()
                )
            }),
        ))
    }

    pub async fn delete_upload_prefix(&self, prefix: &str) -> AppResult<u64> {
        match self.delete_prefix_with_sdk(&self.upload_bucket, prefix).await {
            Ok(deleted_count) => Ok(deleted_count),
            Err(error) => {
                if self
                    .try_delete_prefix_with_aws_cli(&self.upload_bucket, prefix, Some(error.as_str()))
                    .await?
                {
                    return Ok(0);
                }

                Err(AppError::internal_with_context(
                    "failed to delete upload-bucket prefix",
                    format!("bucket={} prefix={} error={error}", self.upload_bucket, prefix),
                ))
            }
        }
    }
}

impl ObjectStorage {
    async fn try_upload_processing_file_with_aws_cli(
        &self,
        object_key: &str,
        local_path: &Path,
        file_size_bytes: u64,
        attempt: u32,
        sdk_error: Option<&str>,
    ) -> AppResult<bool> {
        if !self.enable_aws_cli_s3_fallback {
            return Ok(false);
        }

        let s3_uri = format!("s3://{}/{}", self.processed_bucket, object_key);
        let mut command = Command::new("aws");
        command.args([
            "s3",
            "cp",
            local_path.to_string_lossy().as_ref(),
            s3_uri.as_str(),
            "--region",
            self.aws_region.as_str(),
            "--no-progress",
            "--only-show-errors",
            "--content-type",
            content_type_for_path(local_path),
        ]);

        tracing::warn!(
            attempt,
            bucket = %self.processed_bucket,
            key = object_key,
            file = %local_path.display(),
            file_size_bytes,
            sdk_error = sdk_error.unwrap_or("timed out"),
            "falling back to aws cli for processed artifact upload"
        );

        let output = tokio::time::timeout(PROCESSED_ARTIFACT_UPLOAD_TIMEOUT, command.output())
            .await
            .map_err(|_| {
                AppError::internal_with_context(
                    "aws cli upload fallback timed out",
                    format!(
                        "bucket={} key={} file={} timeout_seconds={}",
                        self.processed_bucket,
                        object_key,
                        local_path.display(),
                        PROCESSED_ARTIFACT_UPLOAD_TIMEOUT.as_secs()
                    ),
                )
            })?
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to spawn aws cli upload fallback",
                    format!(
                        "bucket={} key={} file={} error={error}",
                        self.processed_bucket,
                        object_key,
                        local_path.display()
                    ),
                )
            })?;

        if output.status.success() {
            tracing::info!(
                attempt,
                bucket = %self.processed_bucket,
                key = object_key,
                file = %local_path.display(),
                file_size_bytes,
                "uploaded processed artifact with aws cli fallback"
            );
            return Ok(true);
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            attempt,
            bucket = %self.processed_bucket,
            key = object_key,
            file = %local_path.display(),
            file_size_bytes,
            error = %stderr.trim(),
            "aws cli upload fallback failed"
        );

        Ok(false)
    }

    async fn delete_prefix_with_sdk(&self, bucket: &str, prefix: &str) -> Result<u64, String> {
        let mut deleted_object_count = 0u64;
        let mut continuation_token = None;

        loop {
            let response = self
                .client
                .list_objects_v2()
                .bucket(bucket)
                .prefix(prefix)
                .set_continuation_token(continuation_token.clone())
                .send()
                .await
                .map_err(|error| format!("list_objects_v2 failed: {error}"))?;

            let object_identifiers = response
                .contents()
                .iter()
                .filter_map(|object| object.key())
                .map(|key| {
                    ObjectIdentifier::builder()
                        .key(key)
                        .build()
                        .map_err(|error| format!("failed to build delete object identifier: {error}"))
                })
                .collect::<Result<Vec<_>, _>>()?;

            if !object_identifiers.is_empty() {
                let delete_response = self
                    .client
                    .delete_objects()
                    .bucket(bucket)
                    .delete(
                        Delete::builder()
                            .set_objects(Some(object_identifiers.clone()))
                            .build()
                            .map_err(|error| format!("failed to build delete request: {error}"))?,
                    )
                    .send()
                    .await
                    .map_err(|error| format!("delete_objects failed: {error}"))?;

                if !delete_response.errors().is_empty() {
                    let failed_keys = delete_response
                        .errors()
                        .iter()
                        .filter_map(|error| error.key())
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(format!("delete_objects reported failed keys: {failed_keys}"));
                }

                deleted_object_count += delete_response.deleted().len() as u64;
            }

            if !response.is_truncated().unwrap_or(false) {
                break;
            }

            continuation_token = response.next_continuation_token().map(str::to_owned);

            if continuation_token.is_none() {
                break;
            }
        }

        Ok(deleted_object_count)
    }

    async fn try_delete_prefix_with_aws_cli(
        &self,
        bucket: &str,
        prefix: &str,
        sdk_error: Option<&str>,
    ) -> AppResult<bool> {
        if !self.enable_aws_cli_s3_fallback {
            return Ok(false);
        }

        let s3_uri = format!("s3://{bucket}/{}", prefix.trim_start_matches('/'));
        let mut command = Command::new("aws");
        command.args([
            "s3",
            "rm",
            s3_uri.as_str(),
            "--recursive",
            "--region",
            self.aws_region.as_str(),
            "--only-show-errors",
        ]);

        tracing::warn!(
            bucket,
            prefix,
            sdk_error = sdk_error.unwrap_or("unknown"),
            "falling back to aws cli for prefix deletion"
        );

        let output = tokio::time::timeout(DELETE_PREFIX_TIMEOUT, command.output())
            .await
            .map_err(|_| {
                AppError::internal_with_context(
                    "aws cli delete fallback timed out",
                    format!(
                        "bucket={} prefix={} timeout_seconds={}",
                        bucket,
                        prefix,
                        DELETE_PREFIX_TIMEOUT.as_secs()
                    ),
                )
            })?
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to spawn aws cli delete fallback",
                    format!("bucket={} prefix={} error={error}", bucket, prefix),
                )
            })?;

        if output.status.success() {
            tracing::info!(bucket, prefix, "deleted upload prefix with aws cli fallback");
            return Ok(true);
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(bucket, prefix, error = %stderr.trim(), "aws cli delete fallback failed");

        Ok(false)
    }
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("m3u8") => "application/vnd.apple.mpegurl",
        Some("ts") => "video/mp2t",
        _ => "application/octet-stream",
    }
}

fn upload_priority(path: &Path) -> (u8, String) {
    let priority = match path.extension().and_then(|value| value.to_str()) {
        Some("ts") => 0,
        Some("m3u8") => 1,
        _ => 2,
    };

    (priority, path.to_string_lossy().into_owned())
}

fn collect_files(directory: &Path) -> AppResult<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    collect_files_recursive(directory, &mut files)?;
    Ok(files)
}

fn collect_files_recursive(directory: &Path, files: &mut Vec<std::path::PathBuf>) -> AppResult<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_files_recursive(&path, files)?;
            continue;
        }

        if path.is_file() {
            files.push(path);
        }
    }

    Ok(())
}
