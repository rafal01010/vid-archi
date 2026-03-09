use std::env;
use std::path::PathBuf;

use crate::http::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub app_env: String,
    pub api_port: u16,
    pub database_url: String,
    pub aws_region: String,
    pub upload_bucket: String,
    pub processed_bucket: String,
    pub processed_asset_base_url: Option<String>,
    pub video_policy_file: PathBuf,
    pub s3_endpoint_url: Option<String>,
    pub s3_force_path_style: bool,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub multipart_part_size_bytes: i64,
    pub upload_session_ttl_seconds: i64,
    pub presign_ttl_seconds: u64,
}

impl AppConfig {
    pub fn from_env() -> AppResult<Self> {
        let app_env = env::var("APP_ENV").unwrap_or_else(|_| "local".to_owned());
        let api_port = read_env_with_default("API_PORT", 8080u16)?;
        let database_url = read_required_env("DATABASE_URL")?;
        let aws_region = env::var("AWS_REGION").unwrap_or_else(|_| "ap-northeast-1".to_owned());
        let upload_bucket =
            read_bucket_name("UPLOAD_BUCKET", "LOCAL_UPLOAD_BUCKET", "upload bucket")?;
        let processed_bucket = read_bucket_name(
            "PROCESSED_BUCKET",
            "LOCAL_PROCESSED_BUCKET",
            "processed bucket",
        )?;
        let processed_asset_base_url = env::var("PROCESSED_ASSET_BASE_URL")
            .ok()
            .or_else(|| env::var("CDN_BASE_URL").ok())
            .filter(|value| !value.trim().is_empty());
        let video_policy_file = env::var("VIDEO_POLICY_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("config/video_policy.json"));
        let s3_endpoint_url = env::var("LOCAL_S3_ENDPOINT")
            .ok()
            .or_else(|| env::var("AWS_S3_ENDPOINT").ok())
            .filter(|value| !value.trim().is_empty());
        let s3_force_path_style = s3_endpoint_url.is_some();
        let s3_access_key_id = env::var("MINIO_ROOT_USER")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let s3_secret_access_key = env::var("MINIO_ROOT_PASSWORD")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let multipart_part_size_bytes =
            read_env_with_default("UPLOAD_PART_SIZE_BYTES", 8 * 1024 * 1024i64)?;
        let upload_session_ttl_seconds =
            read_env_with_default("UPLOAD_SESSION_TTL_SECONDS", 24 * 60 * 60i64)?;
        let presign_ttl_seconds = read_env_with_default("UPLOAD_PRESIGN_TTL_SECONDS", 15 * 60u64)?;

        Ok(Self {
            app_env,
            api_port,
            database_url,
            aws_region,
            upload_bucket,
            processed_bucket,
            processed_asset_base_url,
            video_policy_file,
            s3_endpoint_url,
            s3_force_path_style,
            s3_access_key_id,
            s3_secret_access_key,
            multipart_part_size_bytes,
            upload_session_ttl_seconds,
            presign_ttl_seconds,
        })
    }
}

impl AppConfig {
    pub fn manifest_url_for_key(&self, manifest_key: &str) -> Option<String> {
        self.asset_url_for_key(manifest_key)
    }

    pub fn asset_url_for_key(&self, asset_key: &str) -> Option<String> {
        let normalized_key = asset_key.trim_start_matches('/');

        if let Some(base_url) = &self.processed_asset_base_url {
            return Some(format!(
                "{}/{}",
                base_url.trim_end_matches('/'),
                normalized_key
            ));
        }

        self.s3_endpoint_url.as_ref().map(|endpoint_url| {
            format!(
                "{}/{}/{}",
                endpoint_url.trim_end_matches('/'),
                self.processed_bucket,
                normalized_key
            )
        })
    }
}

fn read_required_env(key: &str) -> AppResult<String> {
    env::var(key)
        .map_err(|_| AppError::internal(format!("missing required environment variable {key}")))
}

fn read_bucket_name(primary_key: &str, local_key: &str, label: &str) -> AppResult<String> {
    let bucket = env::var(primary_key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var(local_key)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .ok_or_else(|| AppError::internal(format!("missing {label} configuration")))?;

    Ok(bucket)
}

fn read_env_with_default<T>(key: &str, default_value: T) -> AppResult<T>
where
    T: std::str::FromStr,
{
    match env::var(key) {
        Ok(value) => value.parse::<T>().map_err(|_| {
            AppError::internal(format!(
                "failed to parse environment variable {key} into the expected type"
            ))
        }),
        Err(_) => Ok(default_value),
    }
}
