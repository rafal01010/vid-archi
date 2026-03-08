mod application;
mod domain;
mod error;
mod infrastructure;
mod media;

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use dotenvy::dotenv;
use tracing_subscriber::EnvFilter;

use crate::application::{TranscoderProcessOutcome, TranscoderRuntime};
use crate::domain::video_policy::VideoPolicy;
use crate::infrastructure::config::TranscoderConfig;
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::TranscoderRepository;
use crate::media::MediaProcessor;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    init_tracing();

    let config = TranscoderConfig::from_env()?;
    let poll_interval = Duration::from_secs(config.poll_interval_seconds);
    let policy = Arc::new(VideoPolicy::load(&config.video_policy_file).await?);
    let repository = TranscoderRepository::connect(&config.database_url).await?;
    let object_storage = ObjectStorage::new(&config).await?;
    let media_processor = MediaProcessor::new(&config);
    let runtime =
        TranscoderRuntime::new(config, policy, repository, object_storage, media_processor);

    loop {
        match runtime.process_next_job().await {
            Ok(TranscoderProcessOutcome::Processed {
                transcoding_job_id,
                video_id,
                rendition,
            }) => {
                tracing::info!(%transcoding_job_id, %video_id, %rendition, "transcoder completed rendition job");
            }
            Ok(TranscoderProcessOutcome::Idle) => {
                tokio::time::sleep(poll_interval).await;
            }
            Err(error) => {
                tracing::error!(error = %error, "transcoder loop failed");
                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .json()
        .with_current_span(false)
        .with_span_list(false)
        .init();
}
