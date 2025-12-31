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

        // Format the log message
        let level_str = match level {
            Level::ERROR => "ERROR",
            Level::WARN => "WARN",
            Level::INFO => "INFO",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "TRACE",
        };

        // Get all fields to reconstruct the message
        let mut field_visitor = FieldVisitor::default();
        event.record(&mut field_visitor);

        // Get the message field specifically
        let mut visitor = LogVisitor::default();
        event.record(&mut visitor);

        // Extract the actual message content
        // When using tracing::info!("message: {}", value), the format string is the "message" field
        // and the values are separate fields. We need to reconstruct the formatted message.
        let raw_message = if !visitor.message.is_empty() {
            // If we have a message field, try to format it with other fields
            // For simple cases like "Triggering search for query: '{}'", the message field
            // contains the format string, and we need to substitute values
            let msg_template = visitor.message.trim_matches('"').trim();

            // Check if the message template has placeholders and we have fields to substitute
            if msg_template.contains("{}") && !field_visitor.fields.is_empty() {
                // Try to format the message with the fields
                // This is a simplified approach - for complex formatting, we'd need a proper formatter
                let mut formatted = msg_template.to_string();
                for field_str in &field_visitor.fields {
                    // Extract value from field (format: "name=value")
                    if let Some((name, value)) = field_str.split_once('=') {
                        if name != "message" {
                            // Extract value, removing quotes if present
                            let clean_value = value.trim_matches('"').trim();
                            // Replace first {} with the value
                            if let Some(pos) = formatted.find("{}") {
                                formatted.replace_range(pos..pos + 2, clean_value);
                            }
                        }
                    }
                }
                formatted
            } else {
                msg_template.to_string()
            }
        } else if !field_visitor.fields.is_empty() {
            // No message field, use all fields (excluding message itself)
            field_visitor
                .fields
                .iter()
                .filter(|f| !f.is_empty() && !f.starts_with("message="))
                .map(|f| {
                    // Format as "name: value" for readability
                    if let Some((name, value)) = f.split_once('=') {
                        format!("{}: {}", name, value.trim_matches('"').trim())
                    } else {
                        f.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            // Fallback: use event metadata name
            event.metadata().name().to_string()
        };

        // Clean up the message: remove quotes, trim, and remove any existing level prefixes
        // The raw_message should NOT contain [INFO] or any level prefix - if it does, something is wrong
        let mut message_content = raw_message.trim().to_string();

        // Remove surrounding quotes if present (defensive)
        if message_content.starts_with('"')
            && message_content.ends_with('"')
            && message_content.len() > 1
        {
            message_content = message_content[1..message_content.len() - 1].to_string();
        }

        // Remove any existing level prefixes - this should not be necessary if our extraction is correct
        // but we do it defensively to handle any edge cases
        // Remove all level prefixes until none remain
        let prefixes_with_space = ["[INFO] ", "[WARN] ", "[ERROR] ", "[DEBUG] ", "[TRACE] "];
        let prefixes_without_space = ["[INFO]", "[WARN]", "[ERROR]", "[DEBUG]", "[TRACE]"];

        loop {
            let mut changed = false;

            // Check prefixes with space first
            for prefix in &prefixes_with_space {
                if message_content.starts_with(prefix) {
                    message_content = message_content[prefix.len()..].trim().to_string();
                    changed = true;
                    break;
                }
            }

            // If no change, check prefixes without space
            if !changed {
                for prefix in &prefixes_without_space {
                    if message_content.starts_with(prefix) {
                        message_content = message_content[prefix.len()..].trim().to_string();
                        changed = true;
                        break;
                    }
                }
            }

            // If no change after checking all prefixes, we're done
            if !changed {
                break;
            }
        }

        // Final trim
        message_content = message_content.trim().to_string();

        // Format the final log message with our level prefix
        // At this point, message_content should NOT contain any [LEVEL] prefix
        let log_message = format!("[{}] {}", level_str, message_content);

        // Store in shared state
        // We need to use blocking or ensure this completes
        // Since we're in an async context, use spawn but make sure it's not dropped
        let log_msg = log_message.clone();

        // Store the log message synchronously using a blocking approach
        // Since we're in an async context but need to avoid spawning, we'll use a channel
        // or we can use tokio::spawn but ensure it completes
        // Actually, let's use a blocking approach with tokio::task::spawn_blocking
        // But first, let's try using a simpler approach - just spawn and don't await
        // The key is that the log layer should capture ALL events, including from spawned tasks
        let logs_clone = Arc::clone(&self.logs);
        tokio::spawn(async move {
            let mut logs = logs_clone.lock().await;
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
        // Only capture the "message" field - tracing stores the format string here
        // When using tracing::info!("text"), the message field contains just "text"
        // When using tracing::info!("text: {}", val), the message field contains "text: {}"
        if field.name() == "message" {
            let formatted = format!("{:?}", value);
            // Remove quotes if the value is a string (tracing adds quotes for Debug)
            // The message should be a simple string like "🔥 Fetching trending events..."
            // NOT something like "[INFO] 🔥 Fetching trending events..."
            if formatted.starts_with('"') && formatted.ends_with('"') {
                let unquoted = &formatted[1..formatted.len() - 1];
                // Check if it already has a prefix (this would be a bug)
                if unquoted.starts_with("[INFO]") || unquoted.starts_with("[WARN]") {
                    // This shouldn't happen - strip it
                    self.message = unquoted
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
                        .trim()
                        .to_string();
                } else {
                    self.message = unquoted.to_string();
                }
            } else {
                self.message = formatted;
            }
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        // Only capture the "message" field - tracing stores the format string here
        // This is the preferred method - no Debug formatting needed
        if field.name() == "message" {
            // Check if value already has a prefix (shouldn't happen)
            if value.starts_with("[INFO]") || value.starts_with("[WARN]") {
                // Strip it
                self.message = value
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
                    .trim()
                    .to_string();
            } else {
                self.message = value.to_string();
            }
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
