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
    pub kind: ReceivedTranscoderMessageKind,
    pub receipt_handle: String,
}

#[derive(Debug, Clone)]
pub enum ReceivedTranscoderMessageKind {
    Job(TranscoderJobMessage),
    Invalid { reason: String },
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
    queue_urls: RenditionQueueUrls,
}

#[derive(Clone)]
struct RenditionQueueUrls {
    queue_360p: String,
    queue_480p: String,
    queue_720p: String,
    queue_1080p: String,
    queue_1440p: String,
    queue_2160p: String,
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
        let kind = match serde_json::from_str::<TranscoderJobMessage>(body) {
            Ok(payload) => ReceivedTranscoderMessageKind::Job(payload),
            Err(error) => ReceivedTranscoderMessageKind::Invalid {
                reason: format!("body={body} error={error}"),
            },
        };
        let receipt_handle = message
            .receipt_handle()
            .ok_or_else(|| {
                AppError::internal("received transcoder queue message without a receipt handle")
            })?
            .to_owned();

        Ok(Some(ReceivedTranscoderJobMessage {
            kind,
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
            queue_urls: RenditionQueueUrls {
                queue_360p: config.sqs_transcoder_360p_queue_url.clone(),
                queue_480p: config.sqs_transcoder_480p_queue_url.clone(),
                queue_720p: config.sqs_transcoder_720p_queue_url.clone(),
                queue_1080p: config.sqs_transcoder_1080p_queue_url.clone(),
                queue_1440p: config.sqs_transcoder_1440p_queue_url.clone(),
                queue_2160p: config.sqs_transcoder_2160p_queue_url.clone(),
            },
        })
    }

    pub async fn enqueue(&self, message: TranscoderJobMessage) -> AppResult<()> {
        let body = serde_json::to_string(&message).map_err(|error| {
            AppError::internal_with_context(
                "failed to serialize transcoder queue message",
                error.to_string(),
            )
        })?;

        let queue_url = self
            .queue_urls
            .for_rendition(&message.rendition)
            .ok_or_else(|| {
                AppError::config(format!(
                    "no transcoder queue configured for rendition {}",
                    message.rendition
                ))
            })?;

        self.client
            .send_message()
            .queue_url(queue_url)
            .message_body(body)
            .send()
            .await
            .map_err(|error| {
                AppError::internal_with_context(
                    "failed to send transcoder queue message",
                    format!("queue_url={} error={error:?}", queue_url),
                )
            })?;

        Ok(())
    }
}

impl RenditionQueueUrls {
    fn for_rendition(&self, rendition: &str) -> Option<&str> {
        match rendition {
            "360p" => Some(&self.queue_360p),
            "480p" => Some(&self.queue_480p),
            "720p" => Some(&self.queue_720p),
            "1080p" => Some(&self.queue_1080p),
            "1440p" => Some(&self.queue_1440p),
            "2160p" => Some(&self.queue_2160p),
            _ => None,
        }
        .map(String::as_str)
    }
}
