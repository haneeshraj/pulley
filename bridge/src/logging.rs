//! Local, rotating logs: one file per day in `<config>/logs`, the last 7 kept. Scene
//! geometry is never logged; at debug level only sizes and counts are.

use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};

pub fn init(logs: &Path, debug: bool, to_stderr: bool) -> Option<WorkerGuard> {
    let level = if debug { tracing::Level::DEBUG } else { tracing::Level::INFO };
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("pulley-bridge")
        .filename_suffix("log")
        .max_log_files(7)
        .build(logs)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let builder = tracing_subscriber::fmt().with_max_level(level).with_target(false).with_ansi(false);
    let result = if to_stderr {
        builder.with_writer(std::io::stderr).try_init()
    } else {
        builder.with_writer(writer).try_init()
    };
    result.ok()?;
    Some(guard)
}
