use crate::{Auralog, LogLevel};
use serde_json::json;
use std::panic;
use std::sync::Arc;

pub(crate) fn install(client: Arc<Auralog>) {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("panic");
        let location = info.location().map(|location| {
            json!({
                "file": location.file(),
                "line": location.line(),
                "column": location.column()
            })
        });
        client.log_without_global_metadata(
            LogLevel::Fatal,
            payload.to_string(),
            json!({
                "source": "rust_panic",
                "location": location,
                "thread": std::thread::current().name()
            }),
            None,
        );
        client.flush();
        previous(info);
    }));
}
