use aws_config::BehaviorVersion;
use aws_sdk_sqs::config::Region;
use aws_sdk_sqs::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::http::error::{AppError, AppResult};
use crate::infrastructure::config::AppConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkerJobMessage {
    pub processing_job_id: Uuid,
    pub video_id: Uuid,
    pub correlation_id: String,
    pub attempt: i32,
}

#[derive(Clone)]
pub struct ChunkerQueuePublisher {
    client: Client,
    queue_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscoderJobMessage {
    pub transcoding_job_id: Uuid,
    pub processing_job_id: Uuid,
    pub video_id: Uuid,
    pub rendition: String,
    pub segment_index: i32,
    pub correlation_id: String,
    pub attempt: i32,
}

#[derive(Clone)]
pub struct BaselineTranscoderQueuePublisher {
    client: Client,
    queue_url: String,
}

impl ChunkerQueuePublisher {
    pub async fn new(config: &AppConfig) -> AppResult<Self> {
        let shared_config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(config.aws_region.clone()))
            .load()
            .await;

        Ok(Self {
            client: Client::new(&shared_config),
            queue_url: config.sqs_chunker_queue_url.clone(),
        })
    }

    pub async fn enqueue(&self, message: ChunkerJobMessage) -> AppResult<()> {
        let body = serde_json::to_string(&message).map_err(|error| {
            AppError::internal_with_context(
                "failed to serialize chunker queue message",
                error.to_string(),
            )
        })?;

        self.client
            .send_message()
            .queue_url(&self.queue_url)
            .message_body(body)
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to send chunker queue message",
                    format!("queue_url={} error={error:?}", self.queue_url),
                )
            })?;

        Ok(())
    }
}

impl BaselineTranscoderQueuePublisher {
    pub async fn new(config: &AppConfig) -> AppResult<Self> {
        let shared_config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(config.aws_region.clone()))
            .load()
            .await;

        Ok(Self {
            client: Client::new(&shared_config),
            queue_url: config.sqs_transcoder_360p_queue_url.clone(),
        })
    }

    pub async fn enqueue(&self, message: TranscoderJobMessage) -> AppResult<()> {
        let body = serde_json::to_string(&message).map_err(|error| {
            AppError::internal_with_context(
                "failed to serialize transcoder queue message",
                error.to_string(),
            )
        })?;

        self.client
            .send_message()
            .queue_url(&self.queue_url)
            .message_body(body)
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to send transcoder queue message",
                    format!("queue_url={} error={error:?}", self.queue_url),
                )
            })?;

        Ok(())
    }
}
