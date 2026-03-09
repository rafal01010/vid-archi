mod application;
mod domain;
mod error;
mod infrastructure;
mod media;
mod observability;

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use dotenvy::dotenv;

use crate::application::{ChunkerProcessOutcome, ChunkerRuntime};
use crate::domain::video_policy::VideoPolicy;
use crate::infrastructure::config::ChunkerConfig;
use crate::infrastructure::message_queue::{
    ChunkerJobMessage, ChunkerQueueConsumer, ChunkerQueuePublisher, ReceivedChunkerMessageKind,
    TranscoderQueuePublisher,
};
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::ChunkerRepository;
use crate::media::SourceProbe;
use crate::observability::init_tracing;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    init_tracing("chunker");

    let config = ChunkerConfig::from_env()?;
    let poll_interval = Duration::from_secs(config.poll_interval_seconds);
    let policy = Arc::new(VideoPolicy::load(&config.video_policy_file).await?);
    let repository = ChunkerRepository::connect(&config.database_url).await?;
    let object_storage = ObjectStorage::new(&config).await?;
    let queue_consumer = ChunkerQueueConsumer::new(&config).await?;
    let chunker_queue_publisher = ChunkerQueuePublisher::new(&config).await?;
    let transcoder_queue_publisher = TranscoderQueuePublisher::new(&config).await?;
    let source_probe = SourceProbe::new(&config);
    let runtime = ChunkerRuntime::new(config, policy, repository, object_storage, source_probe);

    loop {
        let Some(message) = queue_consumer.receive().await? else {
            continue;
        };

        let payload = match message.kind {
            ReceivedChunkerMessageKind::Job(payload) => payload,
            ReceivedChunkerMessageKind::Invalid { reason } => {
                queue_consumer.delete(&message.receipt_handle).await?;
                tracing::warn!(reason = %reason, "chunker deleted invalid queue message");
                continue;
            }
        };

        match runtime.process_job(payload.processing_job_id).await {
            Ok(ChunkerProcessOutcome::Dispatched {
                job_id,
                video_id,
                transcoder_message,
            }) => {
                transcoder_queue_publisher
                    .enqueue(transcoder_message)
                    .await?;
                queue_consumer.delete(&message.receipt_handle).await?;
                tracing::info!(%job_id, %video_id, "chunker dispatched baseline transcoder job");
            }
            Ok(ChunkerProcessOutcome::RetryQueued {
                job_id,
                video_id,
                retry_job_id,
                next_attempt,
            }) => {
                queue_consumer.delete(&message.receipt_handle).await?;
                tracing::warn!(%job_id, %video_id, %next_attempt, "chunker failure scheduled a retry");
                let retry_message = ChunkerJobMessage {
                    processing_job_id: retry_job_id,
                    video_id,
                    correlation_id: payload.correlation_id.clone(),
                    attempt: next_attempt,
                };
                chunker_queue_publisher.enqueue(retry_message).await?;
            }
            Ok(ChunkerProcessOutcome::FailedTerminal { job_id, video_id }) => {
                queue_consumer.delete(&message.receipt_handle).await?;
                tracing::error!(%job_id, %video_id, "chunker failure reached the terminal attempt limit");
            }
            Ok(ChunkerProcessOutcome::Ignored { job_id }) => {
                queue_consumer.delete(&message.receipt_handle).await?;
                tracing::info!(%job_id, "chunker ignored stale or duplicate queue message");
            }
            Err(error) => {
                tracing::error!(error = %error, "chunker loop failed");
                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}
