//! Custom tracing layer that captures logs for TUI display

use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

/// A tracing layer that captures log messages and stores them in shared state
pub struct TuiLogLayer {
    logs: Arc<TokioMutex<Vec<String>>>,
}

impl TuiLogLayer {
    pub fn new(logs: Arc<TokioMutex<Vec<String>>>) -> Self {
        Self { logs }
    }
}

impl<S> Layer<S> for TuiLogLayer
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = *event.metadata().level();
        let message = format!("{}", event.metadata().name());

        // Format the log message
        let level_str = match level {
            Level::ERROR => "ERROR",
            Level::WARN => "WARN",
            Level::INFO => "INFO",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "TRACE",
        };

        // Try to get the formatted message from the event
        let mut visitor = LogVisitor::default();
        event.record(&mut visitor);
        
        // Also try to get all fields
        let mut field_visitor = FieldVisitor::default();
        event.record(&mut field_visitor);
        
        // Extract the actual message content (remove any existing [LEVEL] prefix)
        let message_content = if !visitor.message.is_empty() {
            visitor.message.trim_start_matches("[INFO] ")
                .trim_start_matches("[WARN] ")
                .trim_start_matches("[ERROR] ")
                .trim_start_matches("[DEBUG] ")
                .trim_start_matches("[TRACE] ")
                .to_string()
        } else if !field_visitor.fields.is_empty() {
            field_visitor.fields.join(" ")
        } else {
            message.to_string()
        };
        
        let log_message = format!("[{}] {}", level_str, message_content);

        // Store in shared state (synchronously to ensure it's captured)
        // Use blocking spawn or direct access to avoid async issues
        let logs = Arc::clone(&self.logs);
        // Use tokio::spawn to avoid blocking, but ensure it completes
        tokio::spawn(async move {
            let mut logs = logs.lock().await;
            logs.push(log_message);
            // Keep only last 1000 logs
            if logs.len() > 1000 {
                logs.remove(0);
            }
        });
    }
}

#[derive(Default)]
struct LogVisitor {
    message: String,
}

impl tracing::field::Visit for LogVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }

    fn record_f64(&mut self, _field: &tracing::field::Field, _value: f64) {}
    fn record_i64(&mut self, _field: &tracing::field::Field, _value: i64) {}
    fn record_u64(&mut self, _field: &tracing::field::Field, _value: u64) {}
    fn record_bool(&mut self, _field: &tracing::field::Field, _value: bool) {}
    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

#[derive(Default)]
struct FieldVisitor {
    fields: Vec<String>,
}

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields.push(format!("{}={:?}", field.name(), value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.fields.push(format!("{}={}", field.name(), value));
    }
}
