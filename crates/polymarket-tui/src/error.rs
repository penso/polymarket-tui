use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolymarketError {
    #[error("Mutex lock was poisoned: {0}")]
    PoisonedLock(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),
}

pub type Result<T> = std::result::Result<T, PolymarketError>;

/// Helper function to lock a Mutex and convert poison errors
pub fn lock_mutex<T>(mutex: &std::sync::Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|e| PolymarketError::PoisonedLock(format!("{}", e)))
}
