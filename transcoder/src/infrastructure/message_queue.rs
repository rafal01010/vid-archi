use aws_config::BehaviorVersion;
use aws_sdk_sqs::config::Region;
use aws_sdk_sqs::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::infrastructure::config::TranscoderConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscoderJobMessage {
    pub transcoding_job_id: Uuid,
    pub processing_job_id: Uuid,
    pub video_id: Uuid,
    pub rendition: String,
    pub correlation_id: String,
    pub attempt: i32,
}

#[derive(Debug, Clone)]
pub struct ReceivedTranscoderJobMessage {
    pub payload: TranscoderJobMessage,
    pub receipt_handle: String,
}

#[derive(Clone)]
pub struct TranscoderQueueConsumer {
    client: Client,
    queue_url: String,
    wait_time_seconds: i32,
    visibility_timeout_seconds: i32,
}

#[derive(Clone)]
pub struct TranscoderQueuePublisher {
    client: Client,
    queue_url: String,
}

impl TranscoderQueueConsumer {
    pub async fn new(config: &TranscoderConfig) -> AppResult<Self> {
        let shared_config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(config.aws_region.clone()))
            .load()
            .await;

        Ok(Self {
            client: Client::new(&shared_config),
            queue_url: config.sqs_transcoder_queue_url.clone(),
            wait_time_seconds: config.sqs_wait_time_seconds,
            visibility_timeout_seconds: config.sqs_visibility_timeout_seconds,
        })
    }

    pub async fn receive(&self) -> AppResult<Option<ReceivedTranscoderJobMessage>> {
        let response = self
            .client
            .receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(1)
            .wait_time_seconds(self.wait_time_seconds)
            .visibility_timeout(self.visibility_timeout_seconds)
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to receive transcoder queue message",
                    format!("queue_url={} error={error:?}", self.queue_url),
                )
            })?;

        let Some(message) = response.messages().first() else {
            return Ok(None);
        };

        let body = message.body().ok_or_else(|| {
            AppError::internal("received transcoder queue message without a body")
        })?;
        let payload = serde_json::from_str::<TranscoderJobMessage>(body).map_err(|error| {
            AppError::internal_with_context(
                "failed to parse transcoder queue message",
                format!("body={body} error={error}"),
            )
        })?;
        let receipt_handle = message
            .receipt_handle()
            .ok_or_else(|| {
                AppError::internal("received transcoder queue message without a receipt handle")
            })?
            .to_owned();

        Ok(Some(ReceivedTranscoderJobMessage {
            payload,
            receipt_handle,
        }))
    }

    pub async fn delete(&self, receipt_handle: &str) -> AppResult<()> {
        self.client
            .delete_message()
            .queue_url(&self.queue_url)
            .receipt_handle(receipt_handle)
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to delete transcoder queue message",
                    format!("queue_url={} error={error:?}", self.queue_url),
                )
            })?;

        Ok(())
    }

    pub async fn release(&self, receipt_handle: &str) -> AppResult<()> {
        self.client
            .change_message_visibility()
            .queue_url(&self.queue_url)
            .receipt_handle(receipt_handle)
            .visibility_timeout(0)
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to release transcoder queue message",
                    format!("queue_url={} error={error:?}", self.queue_url),
                )
            })?;

        Ok(())
    }
}

impl TranscoderQueuePublisher {
    pub async fn new(config: &TranscoderConfig) -> AppResult<Self> {
        let shared_config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(config.aws_region.clone()))
            .load()
            .await;

        Ok(Self {
            client: Client::new(&shared_config),
            queue_url: config.sqs_transcoder_queue_url.clone(),
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
