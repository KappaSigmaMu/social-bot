use std::fs;
use std::io;
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const LOG_DIR: &str = "logs";

pub fn init(service: &str) -> WorkerGuard {
    fs::create_dir_all(LOG_DIR).expect("creating logs directory");

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let file_appender =
        RollingFileAppender::new(Rotation::DAILY, LOG_DIR, format!("{service}.log"));
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(io::stdout)
                .with_target(false),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_target(false),
        )
        .init();

    guard
}

pub fn log_dir() -> &'static Path {
    Path::new(LOG_DIR)
}
