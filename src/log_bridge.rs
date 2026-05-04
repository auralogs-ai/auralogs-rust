use crate::{Auralog, LogLevel};
use serde_json::json;
use std::sync::Arc;

pub struct AuralogLogLogger {
    client: Arc<Auralog>,
    level_filter: log::LevelFilter,
}

impl AuralogLogLogger {
    pub fn new(client: Arc<Auralog>, level_filter: log::LevelFilter) -> Self {
        Self {
            client,
            level_filter,
        }
    }
}

impl log::Log for AuralogLogLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= self.level_filter
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let level = match record.level() {
            log::Level::Error => LogLevel::Error,
            log::Level::Warn => LogLevel::Warn,
            log::Level::Info => LogLevel::Info,
            log::Level::Debug => LogLevel::Debug,
            log::Level::Trace => LogLevel::Debug,
        };
        self.client.log(
            level,
            record.args().to_string(),
            json!({
                "source": "rust_log",
                "rust_log_level": record.level().as_str(),
                "target": record.target(),
                "module_path": record.module_path(),
                "file": record.file(),
                "line": record.line()
            }),
            None,
        );
    }

    fn flush(&self) {
        self.client.flush();
    }
}

pub fn install_log_logger(
    client: Arc<Auralog>,
    level_filter: log::LevelFilter,
) -> std::result::Result<(), log::SetLoggerError> {
    log::set_boxed_logger(Box::new(AuralogLogLogger::new(client, level_filter)))?;
    log::set_max_level(level_filter);
    Ok(())
}
