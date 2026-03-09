use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Span;
use tracing_subscriber::EnvFilter;

pub const CORRELATION_ID_HEADER: &str = "x-correlation-id";

#[derive(Clone, Debug)]
pub struct CorrelationId(String);

impl CorrelationId {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn init_tracing(service_name: &'static str) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .flatten_event(true)
        .with_target(true)
        .with_file(false)
        .with_line_number(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    tracing::info!(service = service_name, "tracing initialized");
}

pub async fn correlation_id_middleware(
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let correlation_id = request
        .headers()
        .get(CORRELATION_ID_HEADER)
        .and_then(parse_header_value)
        .unwrap_or_else(generate_correlation_id);
    let header_value = HeaderValue::from_str(&correlation_id)
        .unwrap_or_else(|_| HeaderValue::from_static("invalid-correlation-id"));

    request
        .headers_mut()
        .insert(CORRELATION_ID_HEADER, header_value.clone());
    request
        .extensions_mut()
        .insert(CorrelationId::new(correlation_id));

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CORRELATION_ID_HEADER, header_value);
    response
}

pub fn log_request_start<B>(request: &Request<B>, _span: &Span) {
    tracing::info!(
        method = %request.method(),
        uri = %request.uri(),
        "request started"
    );
}

pub fn log_request_finish<B>(
    response: &axum::http::Response<B>,
    latency: Duration,
    _span: &Span,
) {
    tracing::info!(
        status = response.status().as_u16(),
        elapsed_ms = latency.as_millis() as u64,
        "request finished"
    );
}

pub fn log_request_failure(
    failure_class: tower_http::classify::ServerErrorsFailureClass,
    latency: Duration,
    _span: &Span,
) {
    tracing::error!(
        failure_class = %failure_class,
        elapsed_ms = latency.as_millis() as u64,
        "request failed"
    );
}

pub fn generate_correlation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn parse_header_value(value: &HeaderValue) -> Option<String> {
    value
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
