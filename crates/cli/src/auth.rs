//! Authentication configuration module
//!
//! Handles loading and saving API credentials from ~/.config/polymarket-tui/auth.json

use {
    serde::{Deserialize, Serialize},
    std::path::PathBuf,
};

/// Authentication credentials for Polymarket API
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// CLOB API key
    pub api_key: String,
    /// CLOB API secret (base64 encoded, 44 chars)
    pub secret: String,
    /// CLOB API passphrase
    pub passphrase: String,
    /// Polygon wallet address (funder address)
    pub address: String,
    /// Optional username/display name
    #[serde(default)]
    pub username: Option<String>,
}

#[allow(dead_code)]
impl AuthConfig {
    /// Get the config directory path
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("polymarket-tui")
    }

    /// Get the auth config file path
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("auth.json")
    }

    /// Load auth config from file
    pub fn load() -> Option<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return None;
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => Some(config),
                Err(e) => {
                    eprintln!("Failed to parse auth config: {}", e);
                    None
                },
            },
            Err(e) => {
                eprintln!("Failed to read auth config: {}", e);
                None
            },
        }
    }

    /// Save auth config to file
    pub fn save(&self) -> Result<(), String> {
        let dir = Self::config_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let path = Self::config_path();
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize auth config: {}", e))?;

        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write auth config: {}", e))?;

        Ok(())
    }

    /// Delete auth config file (logout)
    pub fn delete() -> Result<(), String> {
        let path = Self::config_path();
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete auth config: {}", e))?;
        }
        Ok(())
    }

    /// Validate the credentials format
    pub fn validate(&self) -> Result<(), String> {
        // Check API key is not empty
        if self.api_key.trim().is_empty() {
            return Err("API key is required".to_string());
        }

        // Check secret is base64 and ~44 chars
        if self.secret.trim().is_empty() {
            return Err("Secret is required".to_string());
        }
        if self.secret.len() != 44 {
            return Err(format!(
                "Secret should be 44 characters (got {})",
                self.secret.len()
            ));
        }

        // Check passphrase is not empty
        if self.passphrase.trim().is_empty() {
            return Err("Passphrase is required".to_string());
        }

        // Check address is valid format (0x + 40 hex chars)
        let address = self.address.trim();
        if !address.starts_with("0x") || address.len() != 42 {
            return Err("Address should be 0x followed by 40 hex characters".to_string());
        }

        Ok(())
    }

    /// Get a shortened display version of the address
    pub fn short_address(&self) -> String {
        if self.address.len() >= 10 {
            format!(
                "{}...{}",
                &self.address[..6],
                &self.address[self.address.len() - 4..]
            )
        } else {
            self.address.clone()
        }
    }

    /// Get display name (username or short address)
    pub fn display_name(&self) -> String {
        self.username
            .clone()
            .unwrap_or_else(|| self.short_address())
    }
}
