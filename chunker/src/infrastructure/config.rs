use std::env;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct ChunkerConfig {
    pub database_url: String,
    pub aws_region: String,
    pub upload_bucket: String,
    pub processed_bucket: String,
    pub sqs_chunker_queue_url: String,
    pub sqs_transcoder_360p_queue_url: String,
    pub sqs_transcoder_480p_queue_url: String,
    pub sqs_transcoder_720p_queue_url: String,
    pub sqs_transcoder_1080p_queue_url: String,
    pub sqs_transcoder_1440p_queue_url: String,
    pub sqs_transcoder_2160p_queue_url: String,
    pub video_policy_file: PathBuf,
    pub s3_endpoint_url: Option<String>,
    pub s3_force_path_style: bool,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub chunker_id: String,
    pub poll_interval_seconds: u64,
    pub sqs_wait_time_seconds: i32,
    pub sqs_visibility_timeout_seconds: i32,
    pub max_processing_attempts: i32,
    pub chunker_temp_dir: PathBuf,
    pub ffmpeg_binary: String,
    pub ffprobe_binary: String,
}

impl ChunkerConfig {
    pub fn from_env() -> AppResult<Self> {
        let database_url = env::var("STG_DATABASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env::var("DATABASE_URL")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .ok_or_else(|| AppError::config("missing DATABASE_URL or STG_DATABASE_URL"))?;
        let aws_region = env::var("AWS_REGION").unwrap_or_else(|_| "ap-northeast-1".to_owned());
        let upload_bucket =
            read_bucket_name("UPLOAD_BUCKET", "LOCAL_UPLOAD_BUCKET", "upload bucket")?;
        let processed_bucket =
            read_bucket_name("PROCESSED_BUCKET", "LOCAL_PROCESSED_BUCKET", "processed bucket")?;
        let sqs_chunker_queue_url = read_required_env("SQS_CHUNKER_QUEUE_URL")?;
        let sqs_transcoder_360p_queue_url = read_required_env("SQS_TRANSCODER_360P_QUEUE_URL")?;
        let sqs_transcoder_480p_queue_url = read_required_env("SQS_TRANSCODER_480P_QUEUE_URL")?;
        let sqs_transcoder_720p_queue_url = read_required_env("SQS_TRANSCODER_720P_QUEUE_URL")?;
        let sqs_transcoder_1080p_queue_url = read_required_env("SQS_TRANSCODER_1080P_QUEUE_URL")?;
        let sqs_transcoder_1440p_queue_url = read_required_env("SQS_TRANSCODER_1440P_QUEUE_URL")?;
        let sqs_transcoder_2160p_queue_url = read_required_env("SQS_TRANSCODER_2160P_QUEUE_URL")?;
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
        let chunker_id = env::var("CHUNKER_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                let hostname = env::var("HOSTNAME").unwrap_or_else(|_| "local".to_owned());
                format!("chunker-{hostname}-{}", std::process::id())
            });
        let poll_interval_seconds = read_env_with_default("CHUNKER_POLL_INTERVAL_SECONDS", 5u64)?;
        let sqs_wait_time_seconds = read_env_with_default("CHUNKER_SQS_WAIT_TIME_SECONDS", 20i32)?;
        let sqs_visibility_timeout_seconds =
            read_env_with_default("CHUNKER_SQS_VISIBILITY_TIMEOUT_SECONDS", 300i32)?;
        let max_processing_attempts =
            read_env_with_default("CHUNKER_MAX_PROCESSING_ATTEMPTS", 3i32)?;
        let chunker_temp_dir = env::var("CHUNKER_TEMP_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp/vid-archi-chunker"));
        let ffmpeg_binary = env::var("FFMPEG_BIN").unwrap_or_else(|_| "ffmpeg".to_owned());
        let ffprobe_binary = env::var("FFPROBE_BIN").unwrap_or_else(|_| "ffprobe".to_owned());

        Ok(Self {
            database_url,
            aws_region,
            upload_bucket,
            processed_bucket,
            sqs_chunker_queue_url,
            sqs_transcoder_360p_queue_url,
            sqs_transcoder_480p_queue_url,
            sqs_transcoder_720p_queue_url,
            sqs_transcoder_1080p_queue_url,
            sqs_transcoder_1440p_queue_url,
            sqs_transcoder_2160p_queue_url,
            video_policy_file,
            s3_endpoint_url,
            s3_force_path_style,
            s3_access_key_id,
            s3_secret_access_key,
            chunker_id,
            poll_interval_seconds,
            sqs_wait_time_seconds,
            sqs_visibility_timeout_seconds,
            max_processing_attempts,
            chunker_temp_dir,
            ffmpeg_binary,
            ffprobe_binary,
        })
    }
}

fn read_required_env(key: &str) -> AppResult<String> {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::config(format!("missing {key}")))
}

fn read_bucket_name(primary_key: &str, local_key: &str, label: &str) -> AppResult<String> {
    env::var(primary_key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var(local_key)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .ok_or_else(|| AppError::config(format!("missing {label} configuration")))
}

fn read_env_with_default<T>(key: &str, default_value: T) -> AppResult<T>
where
    T: std::str::FromStr,
{
    match env::var(key) {
        Ok(value) => value.parse::<T>().map_err(|_| {
            AppError::config(format!(
                "failed to parse environment variable {key} into the expected type"
            ))
        }),
        Err(_) => Ok(default_value),
    }
}
