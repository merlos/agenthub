use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: Server,
    pub privacy: Privacy,
    pub retention: Retention,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Server {
    pub bind: String,
    pub port_start: u16,
    pub port_end: u16,
    pub request_timeout_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Privacy {
    pub log_conversations: bool,
    pub conversation_log_directory: Option<PathBuf>,
    pub conversation_retention_days: u64,
    pub conversation_max_file_mb: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Retention {
    /// Zero retains request metadata indefinitely.
    pub request_metadata_days: u64,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".into(),
            port_start: 43100,
            port_end: 43999,
            request_timeout_seconds: 1800,
        }
    }
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            log_conversations: false,
            conversation_log_directory: None,
            conversation_retention_days: 30,
            conversation_max_file_mb: 10,
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("agenthub")
}
pub fn load(path: &Path) -> anyhow::Result<Config> {
    if !path.exists() {
        let config = Config::default();
        fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
        fs::write(path, toml::to_string_pretty(&config)?)?;
        return Ok(config);
    }
    let config: Config = toml::from_str(&fs::read_to_string(path)?)?;
    validate(&config)?;
    Ok(config)
}

fn validate(config: &Config) -> anyhow::Result<()> {
    if config.server.bind != "127.0.0.1" && config.server.bind != "::1" {
        anyhow::bail!("server.bind must be a loopback address (127.0.0.1 or ::1)");
    }
    if config.server.port_start > config.server.port_end {
        anyhow::bail!("server.port_start must not exceed server.port_end");
    }
    if config.server.request_timeout_seconds == 0 {
        anyhow::bail!("server.request_timeout_seconds must be greater than zero");
    }
    if config.privacy.conversation_max_file_mb == 0 {
        anyhow::bail!("privacy.conversation_max_file_mb must be greater than zero");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creates_and_loads_config() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        let a = load(&p).unwrap();
        assert!(!a.privacy.log_conversations);
        assert!(p.exists());
        let b = load(&p).unwrap();
        assert_eq!(b.server.port_start, 43100);
        assert_eq!(b.server.request_timeout_seconds, 1800);
    }

    #[test]
    fn rejects_unsafe_or_invalid_server_settings() {
        let mut config = Config::default();
        config.server.bind = "0.0.0.0".into();
        assert!(validate(&config).is_err());
        config.server.bind = "127.0.0.1".into();
        config.server.port_start = 2;
        config.server.port_end = 1;
        assert!(validate(&config).is_err());
        config.server.port_start = 1;
        config.server.request_timeout_seconds = 0;
        assert!(validate(&config).is_err());
        config.server.request_timeout_seconds = 1;
        config.privacy.conversation_max_file_mb = 0;
        assert!(validate(&config).is_err());
        assert!(config_dir().ends_with("agenthub"));
    }
}
