//! Logging macros for conditional tracing

/// Macros for conditional logging based on tracing feature
#[cfg(feature = "tracing")]
macro_rules! log_info {
    ($($arg:tt)*) => { tracing::info!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_info {
    ($($arg:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! log_debug {
    ($($arg:tt)*) => { tracing::debug!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_debug {
    ($($arg:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! log_error {
    ($($arg:tt)*) => { tracing::error!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_error {
    ($($arg:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! log_warn {
    ($($arg:tt)*) => { tracing::warn!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_warn {
    ($($arg:tt)*) => {};
}

pub(crate) use {log_debug, log_error, log_info, log_warn};
