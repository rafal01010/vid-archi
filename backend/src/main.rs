mod app;
mod application;
mod domain;
mod http;
mod infrastructure;
mod observability;

use std::error::Error;
use std::net::SocketAddr;

use dotenvy::dotenv;

use crate::app::{build_router, AppState};
use crate::application::{UploadService, VideoDeleteService, VideoQueryService};
use crate::domain::video_policy::VideoPolicy;
use crate::infrastructure::config::AppConfig;
use crate::infrastructure::message_queue::ChunkerQueuePublisher;
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::VideoRepository;
use crate::observability::init_tracing;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    init_tracing("backend");

    let config = AppConfig::from_env()?;
    let policy = VideoPolicy::load(&config.video_policy_file).await?;
    let repository = VideoRepository::connect(&config.database_url).await?;
    let object_storage = ObjectStorage::new(&config).await?;
    let queue_publisher = ChunkerQueuePublisher::new(&config).await?;
    let upload_service = UploadService::new(
        config.clone(),
        policy.clone(),
        repository.clone(),
        object_storage.clone(),
        queue_publisher,
    );
    let video_delete_service =
        VideoDeleteService::new(config.clone(), repository.clone(), object_storage);
    let video_query_service = VideoQueryService::new(config.clone(), policy, repository);

    let state = AppState::new(
        config.clone(),
        upload_service,
        video_delete_service,
        video_query_service,
    );
    let router = build_router(state);
    let address = SocketAddr::from(([0, 0, 0, 0], config.api_port));

    tracing::info!(%address, "starting upload API");

    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
