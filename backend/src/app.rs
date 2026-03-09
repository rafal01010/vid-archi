use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::middleware;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::Level;

use crate::application::{UploadService, VideoQueryService};
use crate::http::handlers;
use crate::infrastructure::config::AppConfig;
use crate::observability::{
    correlation_id_middleware, log_request_failure, log_request_finish, log_request_start,
    parse_header_value, CORRELATION_ID_HEADER,
};

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
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| {
                    let correlation_id = request
                        .headers()
                        .get(CORRELATION_ID_HEADER)
                        .and_then(parse_header_value)
                        .unwrap_or_else(|| "missing".to_owned());

                    tracing::span!(
                        Level::INFO,
                        "http_request",
                        method = %request.method(),
                        uri = %request.uri(),
                        correlation_id = %correlation_id
                    )
                })
                .on_request(log_request_start)
                .on_response(log_request_finish)
                .on_failure(log_request_failure),
        )
        .layer(middleware::from_fn(correlation_id_middleware))
        .with_state(state)
}
