use std::path::Path;

use aws_config::BehaviorVersion;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult};
use crate::infrastructure::config::ChunkerConfig;

#[derive(Clone)]
pub struct ObjectStorage {
    client: Client,
    upload_bucket: String,
    processed_bucket: String,
}

impl ObjectStorage {
    pub async fn new(config: &ChunkerConfig) -> AppResult<Self> {
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

    pub async fn upload_source_segment(&self, object_key: &str, local_path: &Path) -> AppResult<()> {
        self.upload_file(&self.upload_bucket, object_key, local_path, "video/mp2t")
            .await
    }

    pub async fn upload_source_playlist(&self, object_key: &str, local_path: &Path) -> AppResult<()> {
        self.upload_file(
            &self.upload_bucket,
            object_key,
            local_path,
            "application/vnd.apple.mpegurl",
        )
        .await
    }

    pub async fn upload_processed_file(
        &self,
        object_key: &str,
        local_path: &Path,
        content_type: &str,
    ) -> AppResult<()> {
        self.upload_file(&self.processed_bucket, object_key, local_path, content_type)
            .await
    }

    async fn upload_file(
        &self,
        bucket: &str,
        object_key: &str,
        local_path: &Path,
        content_type: &str,
    ) -> AppResult<()> {
        let file_bytes = tokio::fs::read(local_path).await.map_err(|error| {
            AppError::internal_with_context(
                "failed to read generated artifact before upload",
                format!("file={} error={error}", local_path.display()),
            )
        })?;
        let file_size_bytes = file_bytes.len() as i64;

        self.client
            .put_object()
            .bucket(bucket)
            .key(object_key)
            .content_type(content_type)
            .content_length(file_size_bytes)
            .body(ByteStream::from(file_bytes))
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to upload generated artifact",
                    format!(
                        "bucket={} key={} file={} error={error}",
                        bucket,
                        object_key,
                        local_path.display()
                    ),
                )
            })?;

        Ok(())
    }
}
