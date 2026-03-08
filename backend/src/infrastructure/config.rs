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
        let upload_bucket = read_bucket_name()?;
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

fn read_required_env(key: &str) -> AppResult<String> {
    env::var(key)
        .map_err(|_| AppError::internal(format!("missing required environment variable {key}")))
}

fn read_bucket_name() -> AppResult<String> {
    let bucket = env::var("UPLOAD_BUCKET")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("LOCAL_UPLOAD_BUCKET")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .ok_or_else(|| AppError::internal("missing upload bucket configuration"))?;

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
