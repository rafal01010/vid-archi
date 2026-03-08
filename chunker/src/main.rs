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

use crate::application::{ChunkerProcessOutcome, ChunkerRuntime};
use crate::domain::video_policy::VideoPolicy;
use crate::infrastructure::config::ChunkerConfig;
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::ChunkerRepository;
use crate::media::SourceProbe;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    init_tracing();

    let config = ChunkerConfig::from_env()?;
    let poll_interval = Duration::from_secs(config.poll_interval_seconds);
    let policy = Arc::new(VideoPolicy::load(&config.video_policy_file).await?);
    let repository = ChunkerRepository::connect(&config.database_url).await?;
    let object_storage = ObjectStorage::new(&config).await?;
    let source_probe = SourceProbe::new(&config);
    let runtime = ChunkerRuntime::new(config, policy, repository, object_storage, source_probe);

    loop {
        match runtime.process_next_job().await {
            Ok(ChunkerProcessOutcome::Dispatched { job_id, video_id }) => {
                tracing::info!(%job_id, %video_id, "chunker dispatched baseline transcoder job");
            }
            Ok(ChunkerProcessOutcome::Idle) => {
                tokio::time::sleep(poll_interval).await;
            }
            Err(error) => {
                tracing::error!(error = %error, "chunker loop failed");
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
