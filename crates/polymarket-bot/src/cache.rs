use crate::error::{PolymarketError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Generic file-based cache for storing serializable data
#[derive(Clone)]
pub struct FileCache {
    cache_dir: PathBuf,
    default_ttl_seconds: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry<T> {
    data: T,
    cached_at: u64,
    ttl_seconds: Option<u64>,
}

impl FileCache {
    /// Create a new FileCache with the given cache directory
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        fs::create_dir_all(&cache_dir).map_err(|e| {
            PolymarketError::InvalidData(format!("Failed to create cache directory: {}", e))
        })?;

        Ok(Self {
            cache_dir,
            default_ttl_seconds: None,
        })
    }

    /// Set a default TTL for cached entries (in seconds)
    pub fn with_default_ttl(mut self, ttl_seconds: u64) -> Self {
        self.default_ttl_seconds = Some(ttl_seconds);
        self
    }

    /// Get cached data by key
    pub fn get<T>(&self, key: &str) -> Result<Option<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let cache_file = self.cache_file_path(key);

        if !cache_file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&cache_file).map_err(|e| {
            PolymarketError::InvalidData(format!("Failed to read cache file: {}", e))
        })?;

        let entry: CacheEntry<T> =
            serde_json::from_str(&content).map_err(PolymarketError::Serialization)?;

        // Check if entry has expired
        if let Some(ttl) = entry.ttl_seconds {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| PolymarketError::InvalidData(format!("System time error: {}", e)))?
                .as_secs();

            if now.saturating_sub(entry.cached_at) > ttl {
                // Cache expired, remove file and return None
                let _ = fs::remove_file(&cache_file);
                return Ok(None);
            }
        }

        Ok(Some(entry.data))
    }

    /// Store data in cache with the given key
    pub fn set<T>(&self, key: &str, data: T) -> Result<()>
    where
        T: Serialize,
    {
        let cache_file = self.cache_file_path(key);

        let cached_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| PolymarketError::InvalidData(format!("System time error: {}", e)))?
            .as_secs();

        let entry = CacheEntry {
            data,
            cached_at,
            ttl_seconds: self.default_ttl_seconds,
        };

        let json = serde_json::to_string_pretty(&entry).map_err(PolymarketError::Serialization)?;

        // Write to temp file first, then rename (atomic operation)
        let temp_file = cache_file.with_extension("tmp");
        fs::write(&temp_file, json).map_err(|e| {
            PolymarketError::InvalidData(format!("Failed to write cache file: {}", e))
        })?;

        fs::rename(&temp_file, &cache_file).map_err(|e| {
            PolymarketError::InvalidData(format!("Failed to rename cache file: {}", e))
        })?;

        Ok(())
    }

    /// Remove a cached entry
    pub fn remove(&self, key: &str) -> Result<()> {
        let cache_file = self.cache_file_path(key);
        if cache_file.exists() {
            fs::remove_file(&cache_file).map_err(|e| {
                PolymarketError::InvalidData(format!("Failed to remove cache file: {}", e))
            })?;
        }
        Ok(())
    }

    /// Clear all cached entries
    pub fn clear(&self) -> Result<()> {
        if self.cache_dir.exists() {
            for entry in fs::read_dir(&self.cache_dir).map_err(|e| {
                PolymarketError::InvalidData(format!("Failed to read cache directory: {}", e))
            })? {
                let entry = entry.map_err(|e| {
                    PolymarketError::InvalidData(format!("Failed to read directory entry: {}", e))
                })?;
                let path = entry.path();
                if path.is_file() && path.extension().map(|e| e == "json").unwrap_or(false) {
                    fs::remove_file(&path).map_err(|e| {
                        PolymarketError::InvalidData(format!("Failed to remove cache file: {}", e))
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Get the cache directory path
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    fn cache_file_path(&self, key: &str) -> PathBuf {
        // Sanitize key to be filesystem-safe
        let sanitized = key
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        self.cache_dir.join(format!("{}.json", sanitized))
    }
}

/// Helper function to get default cache directory
pub fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .map(|d| d.join("polymarket-bot"))
        .unwrap_or_else(|| PathBuf::from(".cache"))
}
