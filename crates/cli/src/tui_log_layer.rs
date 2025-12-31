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
        let target = event.metadata().target();
        let module_path = event.metadata().module_path().unwrap_or("");

        // Format the log message
        let level_str = match level {
            Level::ERROR => "ERROR",
            Level::WARN => "WARN",
            Level::INFO => "INFO",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "TRACE",
        };

        // Try to get the formatted message from the event
        // First, get all fields to see what we have
        let mut field_visitor = FieldVisitor::default();
        event.record(&mut field_visitor);

        // Also try to get message specifically
        let mut visitor = LogVisitor::default();
        event.record(&mut visitor);

        // Extract the actual message content
        // When using tracing::info!("message: {}", value), the format string is the "message" field
        // and the values are separate fields. We need to reconstruct the formatted message.
        let raw_message = if !visitor.message.is_empty() {
            // If we have a message field, try to format it with other fields
            // For simple cases like "Triggering search for query: '{}'", the message field
            // contains the format string, and we need to substitute values
            let msg_template = visitor.message;

            // Check if the message template has placeholders and we have fields to substitute
            if msg_template.contains("{}") && !field_visitor.fields.is_empty() {
                // Try to format the message with the fields
                // This is a simplified approach - for complex formatting, we'd need a proper formatter
                let mut formatted = msg_template;
                for field in &field_visitor.fields {
                    // Extract value from field (format: "name=value")
                    if let Some((name, value)) = field.split_once('=') {
                        if name != "message" {
                            // Replace first {} with the value
                            if let Some(pos) = formatted.find("{}") {
                                formatted.replace_range(pos..pos + 2, value);
                            }
                        }
                    }
                }
                formatted
            } else {
                msg_template
            }
        } else if !field_visitor.fields.is_empty() {
            // No message field, use all fields
            field_visitor
                .fields
                .iter()
                .filter(|f| !f.is_empty() && !f.starts_with("message="))
                .map(|f| f.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            // Fallback to target/module path
            if !target.is_empty() {
                format!("{}: {}", target, module_path)
            } else {
                module_path.to_string()
            }
        };

        // Remove any existing [LEVEL] prefix from the message (handles double prefixes)
        let message_content = raw_message
            .trim_start_matches("[INFO] ")
            .trim_start_matches("[WARN] ")
            .trim_start_matches("[ERROR] ")
            .trim_start_matches("[DEBUG] ")
            .trim_start_matches("[TRACE] ")
            .trim_start_matches("[INFO]")
            .trim_start_matches("[WARN]")
            .trim_start_matches("[ERROR]")
            .trim_start_matches("[DEBUG]")
            .trim_start_matches("[TRACE]")
            .trim();

        let log_message = format!("[{}] {}", level_str, message_content);

        // Store in shared state
        // We need to use blocking or ensure this completes
        // Since we're in an async context, use spawn but make sure it's not dropped
        let logs = Arc::clone(&self.logs);
        let log_msg = log_message.clone();

        // Try to push synchronously if possible, otherwise spawn
        // For now, use spawn but ensure it's awaited somewhere
        tokio::spawn(async move {
            let mut logs = logs.lock().await;
            logs.push(log_msg);
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
