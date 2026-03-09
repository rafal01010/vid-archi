use tracing_subscriber::EnvFilter;

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
