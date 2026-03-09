use std::path::Path;

use aws_config::BehaviorVersion;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult};
use crate::infrastructure::config::TranscoderConfig;

#[derive(Clone)]
pub struct ObjectStorage {
    client: Client,
    upload_bucket: String,
    processed_bucket: String,
}

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
            upload_bucket: config.upload_bucket.clone(),
            processed_bucket: config.processed_bucket.clone(),
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
        for local_path in collect_files(local_directory)? {
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
        let body = ByteStream::from_path(local_path.to_path_buf())
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to open processing artifact for upload",
                    format!("file={} error={error}", local_path.display()),
                )
            })?;

        self.client
            .put_object()
            .bucket(&self.processed_bucket)
            .key(object_key)
            .content_type(content_type_for_path(local_path))
            .body(body)
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to upload processing artifact",
                    format!(
                        "bucket={} key={} file={} error={error}",
                        self.processed_bucket,
                        object_key,
                        local_path.display()
                    ),
                )
            })?;

        Ok(())
    }
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("m3u8") => "application/vnd.apple.mpegurl",
        Some("ts") => "video/mp2t",
        _ => "application/octet-stream",
    }
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
