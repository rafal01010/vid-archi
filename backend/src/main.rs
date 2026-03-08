mod app;
mod application;
mod domain;
mod http;
mod infrastructure;

use std::error::Error;
use std::net::SocketAddr;

use dotenvy::dotenv;
use tracing_subscriber::EnvFilter;

use crate::app::{AppState, build_router};
use crate::application::UploadService;
use crate::domain::video_policy::VideoPolicy;
use crate::infrastructure::config::AppConfig;
use crate::infrastructure::object_storage::ObjectStorage;
use crate::infrastructure::postgres::VideoRepository;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    init_tracing();

    let config = AppConfig::from_env()?;
    let policy = VideoPolicy::load(&config.video_policy_file).await?;
    let repository = VideoRepository::connect(&config.database_url).await?;
    let object_storage = ObjectStorage::new(&config).await?;
    let upload_service = UploadService::new(config.clone(), policy.clone(), repository, object_storage);

    let state = AppState::new(config.clone(), upload_service);
    let router = build_router(state);
    let address = SocketAddr::from(([0, 0, 0, 0], config.api_port));

    tracing::info!(%address, "starting upload API");

    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router).await?;

    Ok(())
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
