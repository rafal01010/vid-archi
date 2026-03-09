use std::sync::Arc;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::application::{UploadService, VideoQueryService};
use crate::http::handlers;
use crate::infrastructure::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub upload_service: UploadService,
    pub video_query_service: VideoQueryService,
}

impl AppState {
    pub fn new(
        config: AppConfig,
        upload_service: UploadService,
        video_query_service: VideoQueryService,
    ) -> Self {
        Self {
            config: Arc::new(config),
            upload_service,
            video_query_service,
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", axum::routing::get(handlers::health_check))
        .route(
            "/api/videos",
            axum::routing::get(handlers::list_recent_videos).post(handlers::create_video_upload),
        )
        .route(
            "/api/videos/{public_id}",
            axum::routing::get(handlers::get_video_details),
        )
        .route(
            "/api/videos/{public_id}/playback",
            axum::routing::get(handlers::get_video_playback),
        )
        .route(
            "/api/videos/{video_id}/parts/sign",
            axum::routing::post(handlers::sign_upload_parts),
        )
        .route(
            "/api/videos/{video_id}/complete",
            axum::routing::post(handlers::complete_video_upload),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
