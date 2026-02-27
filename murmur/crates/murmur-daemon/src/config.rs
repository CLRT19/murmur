use murmur_providers::ProviderConfig;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level daemon configuration.
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub voice: VoiceConfig,
    #[serde(default)]
    pub context: ContextConfig,
}

#[derive(Debug, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_socket_path")]
    pub socket_path: String,
    #[serde(default = "default_cache_size")]
    pub cache_size: usize,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Deserialize)]
pub struct VoiceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_voice_engine")]
    pub engine: String,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_language")]
    pub language: String,
}

#[derive(Debug, Deserialize)]
pub struct ContextConfig {
    #[serde(default = "default_history_lines")]
    pub history_lines: usize,
    #[serde(default = "default_true")]
    pub git_enabled: bool,
    #[serde(default = "default_true")]
    pub project_detection: bool,
}

fn default_socket_path() -> String {
    "/tmp/murmur.sock".to_string()
}

fn default_cache_size() -> usize {
    1000
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_voice_engine() -> String {
    "whisper".to_string()
}

fn default_hotkey() -> String {
    "ctrl+shift+v".to_string()
}

fn default_language() -> String {
    "en".to_string()
}

fn default_history_lines() -> usize {
    500
}

fn default_true() -> bool {
    true
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            cache_size: default_cache_size(),
            log_level: default_log_level(),
        }
    }
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: default_voice_engine(),
            hotkey: default_hotkey(),
            language: default_language(),
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            history_lines: default_history_lines(),
            git_enabled: default_true(),
            project_detection: default_true(),
        }
    }
}

impl Config {
    /// Load config from the default path (~/.config/murmur/config.toml).
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Load config from a specific path.
    pub fn load_from(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(format!("{home}/.config/murmur/config.toml"))
    }

    pub fn pid_path() -> PathBuf {
        PathBuf::from("/tmp/murmur.pid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = Config::default();
        assert_eq!(config.daemon.socket_path, "/tmp/murmur.sock");
        assert_eq!(config.daemon.cache_size, 1000);
        assert!(!config.voice.enabled);
    }

    #[test]
    fn parse_toml_config() {
        let toml_str = r#"
[daemon]
socket_path = "/tmp/test.sock"
cache_size = 500
log_level = "debug"

[providers.anthropic]
api_key = "sk-test"
model = "claude-haiku-4-5-20251001"

[voice]
enabled = true
engine = "apple"

[context]
history_lines = 100
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.daemon.socket_path, "/tmp/test.sock");
        assert_eq!(config.daemon.cache_size, 500);
        assert!(config.providers.contains_key("anthropic"));
        assert!(config.voice.enabled);
        assert_eq!(config.context.history_lines, 100);
    }
}
