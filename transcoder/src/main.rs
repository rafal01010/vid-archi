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

use crate::application::{TranscoderProcessOutcome, TranscoderRuntime};
use crate::domain::video_policy::VideoPolicy;
use crate::infrastructure::config::TranscoderConfig;
use crate::infrastructure::message_queue::{
    ReceivedTranscoderMessageKind, TranscoderQueueConsumer, TranscoderQueuePublisher,
};
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::TranscoderRepository;
use crate::media::MediaProcessor;
use crate::observability::init_tracing;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    init_tracing("transcoder");

    let config = TranscoderConfig::from_env()?;
    let poll_interval = Duration::from_secs(config.poll_interval_seconds);
    let policy = Arc::new(VideoPolicy::load(&config.video_policy_file).await?);
    let repository = TranscoderRepository::connect(&config.database_url).await?;
    let object_storage = ObjectStorage::new(&config).await?;
    let queue_consumer = TranscoderQueueConsumer::new(&config).await?;
    let queue_publisher = TranscoderQueuePublisher::new(&config).await?;
    let media_processor = MediaProcessor::new(&config);
    let runtime =
        TranscoderRuntime::new(config, policy, repository, object_storage, media_processor);

    loop {
        let Some(message) = queue_consumer.receive().await? else {
            continue;
        };

        let payload = match message.kind {
            ReceivedTranscoderMessageKind::Job(payload) => payload,
            ReceivedTranscoderMessageKind::Invalid { reason } => {
                queue_consumer.delete(&message.receipt_handle).await?;
                tracing::warn!(reason = %reason, "transcoder deleted invalid queue message");
                continue;
            }
        };

        match runtime.process_job(&payload).await {
            Ok(TranscoderProcessOutcome::Processed {
                transcoding_job_id,
                video_id,
                rendition,
                segment_index,
                follow_up_jobs,
            }) => {
                queue_consumer.delete(&message.receipt_handle).await?;
                for follow_up_job in follow_up_jobs {
                    queue_publisher.enqueue(follow_up_job).await?;
                }
                tracing::info!(%transcoding_job_id, %video_id, %rendition, %segment_index, "transcoder completed segment job");
            }
            Ok(TranscoderProcessOutcome::RetryQueued {
                transcoding_job_id,
                video_id,
                rendition,
                segment_index,
                next_attempt,
                retry_job_message,
            }) => {
                queue_consumer.delete(&message.receipt_handle).await?;
                queue_publisher.enqueue(retry_job_message).await?;
                tracing::warn!(
                    %transcoding_job_id,
                    %video_id,
                    %rendition,
                    %segment_index,
                    %next_attempt,
                    "transcoder failure scheduled a retry"
                );
            }
            Ok(TranscoderProcessOutcome::FailedTerminal {
                transcoding_job_id,
                video_id,
                rendition,
                segment_index,
            }) => {
                queue_consumer.delete(&message.receipt_handle).await?;
                tracing::error!(
                    %transcoding_job_id,
                    %video_id,
                    %rendition,
                    %segment_index,
                    "transcoder failure reached the terminal attempt limit"
                );
            }
            Ok(TranscoderProcessOutcome::Ignored { transcoding_job_id }) => {
                queue_consumer.delete(&message.receipt_handle).await?;
                tracing::info!(%transcoding_job_id, "transcoder ignored stale or duplicate queue message");
            }
            Ok(TranscoderProcessOutcome::WrongRendition {
                transcoding_job_id,
                queue_rendition,
                worker_rendition,
                reroute_job_message,
            }) => {
                queue_publisher.enqueue(reroute_job_message).await?;
                queue_consumer.delete(&message.receipt_handle).await?;
                tracing::info!(
                    %transcoding_job_id,
                    %queue_rendition,
                    %worker_rendition,
                    "transcoder rerouted queue message to the correct rendition queue"
                );
                tokio::time::sleep(poll_interval).await;
            }
            Err(error) => {
                tracing::error!(error = %error, "transcoder loop failed");
                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}
