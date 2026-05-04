use crate::{Auralog, LogLevel};
use serde_json::{Map, Value};
use std::sync::Arc;
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

pub struct AuralogLayer {
    client: Arc<Auralog>,
}

impl AuralogLayer {
    pub fn new(client: Arc<Auralog>) -> Self {
        Self { client }
    }
}

impl<S> Layer<S> for AuralogLayer
where
    S: Subscriber,
    for<'lookup> S: LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut visitor = JsonVisitor::default();
        attrs.record(&mut visitor);
        span.extensions_mut().insert(SpanFields(visitor.fields));
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut visitor = JsonVisitor::default();
        values.record(&mut visitor);
        let mut extensions = span.extensions_mut();
        let fields = extensions.get_mut::<SpanFields>();
        if let Some(fields) = fields {
            fields.0.extend(visitor.fields);
        } else {
            extensions.insert(SpanFields(visitor.fields));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);

        let message = visitor
            .fields
            .remove("message")
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| metadata.name().to_string());

        visitor.fields.insert(
            "source".to_string(),
            Value::String("rust_tracing".to_string()),
        );
        visitor.fields.insert(
            "target".to_string(),
            Value::String(metadata.target().to_string()),
        );
        if let Some(module_path) = metadata.module_path() {
            visitor.fields.insert(
                "module_path".to_string(),
                Value::String(module_path.to_string()),
            );
        }
        if let Some(file) = metadata.file() {
            visitor
                .fields
                .insert("file".to_string(), Value::String(file.to_string()));
        }
        if let Some(line) = metadata.line() {
            visitor
                .fields
                .insert("line".to_string(), Value::Number(line.into()));
        }
        if let Some(scope) = ctx.event_scope(event) {
            let spans: Vec<Value> = scope
                .from_root()
                .map(|span| {
                    let metadata = span.metadata();
                    let fields = span
                        .extensions()
                        .get::<SpanFields>()
                        .map(|fields| Value::Object(fields.0.clone()))
                        .unwrap_or(Value::Null);
                    serde_json::json!({
                        "name": metadata.name(),
                        "target": metadata.target(),
                        "module_path": metadata.module_path(),
                        "file": metadata.file(),
                        "line": metadata.line(),
                        "fields": fields
                    })
                })
                .collect();
            if !spans.is_empty() {
                visitor
                    .fields
                    .insert("spans".to_string(), Value::Array(spans));
            }
        }

        self.client.log(
            level_from_tracing(*metadata.level()),
            message,
            Value::Object(visitor.fields),
            None,
        );
    }
}

fn level_from_tracing(level: tracing_core::Level) -> LogLevel {
    match level {
        tracing_core::Level::ERROR => LogLevel::Error,
        tracing_core::Level::WARN => LogLevel::Warn,
        tracing_core::Level::INFO => LogLevel::Info,
        tracing_core::Level::DEBUG | tracing_core::Level::TRACE => LogLevel::Debug,
    }
}

#[derive(Default)]
struct JsonVisitor {
    fields: Map<String, Value>,
}

#[derive(Clone)]
struct SpanFields(Map<String, Value>);

impl tracing_core::field::Visit for JsonVisitor {
    fn record_i64(&mut self, field: &tracing_core::field::Field, value: i64) {
        self.fields.insert(field.name().to_string(), value.into());
    }

    fn record_u64(&mut self, field: &tracing_core::field::Field, value: u64) {
        self.fields.insert(field.name().to_string(), value.into());
    }

    fn record_bool(&mut self, field: &tracing_core::field::Field, value: bool) {
        self.fields.insert(field.name().to_string(), value.into());
    }

    fn record_str(&mut self, field: &tracing_core::field::Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), Value::String(value.to_string()));
    }

    fn record_debug(&mut self, field: &tracing_core::field::Field, value: &dyn std::fmt::Debug) {
        self.fields.insert(
            field.name().to_string(),
            Value::String(format!("{value:?}")),
        );
    }
}
