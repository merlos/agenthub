use crate::config::Privacy;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct ConversationLogger {
    enabled: bool,
    directory: PathBuf,
    retention_days: u64,
    max_file_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConversationEntry<'a> {
    timestamp: DateTime<Utc>,
    application_id: &'a str,
    provider_id: &'a str,
    endpoint: &'a str,
    request: EncodedBody<'a>,
    response: EncodedBody<'a>,
}

#[derive(Serialize)]
#[serde(tag = "encoding", content = "data", rename_all = "lowercase")]
enum EncodedBody<'a> {
    Utf8(&'a str),
    Base64(String),
}

impl ConversationLogger {
    pub fn new(privacy: &Privacy, config_directory: &Path) -> Self {
        Self {
            enabled: privacy.log_conversations,
            directory: privacy
                .conversation_log_directory
                .clone()
                .unwrap_or_else(|| config_directory.join("conversations")),
            retention_days: privacy.conversation_retention_days,
            max_file_bytes: privacy.conversation_max_file_mb * 1024 * 1024,
        }
    }

    pub fn log(
        &self,
        application_id: &str,
        provider_id: &str,
        endpoint: &str,
        request: &[u8],
        response: &[u8],
    ) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        fs::create_dir_all(&self.directory)?;
        self.prune()?;
        let now = Utc::now();
        let entry = ConversationEntry {
            timestamp: now,
            application_id,
            provider_id,
            endpoint,
            request: encode_body(request),
            response: encode_body(response),
        };
        let mut encoded = serde_json::to_vec(&entry)?;
        encoded.push(b'\n');
        let path = self.output_path(now)?;
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(&encoded)?;
        Ok(())
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    fn output_path(&self, now: DateTime<Utc>) -> anyhow::Result<PathBuf> {
        let base = now.format("conversations-%Y-%m-%d").to_string();
        for index in 0..10_000 {
            let suffix = if index == 0 {
                String::new()
            } else {
                format!("-{index}")
            };
            let path = self.directory.join(format!("{base}{suffix}.jsonl"));
            let size = fs::metadata(&path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if size < self.max_file_bytes {
                return Ok(path);
            }
        }
        anyhow::bail!("conversation log rotation limit reached")
    }

    fn prune(&self) -> anyhow::Result<()> {
        if self.retention_days == 0 || !self.directory.exists() {
            return Ok(());
        }
        let cutoff = std::time::SystemTime::now()
            - std::time::Duration::from_secs(
                Duration::days(self.retention_days as i64).num_seconds() as u64,
            );
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("jsonl")
                && entry.metadata()?.modified()? < cutoff
            {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}

fn encode_body(body: &[u8]) -> EncodedBody<'_> {
    match std::str::from_utf8(body) {
        Ok(text) => EncodedBody::Utf8(text),
        Err(_) => EncodedBody::Base64(base64::engine::general_purpose::STANDARD.encode(body)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_logger_writes_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let logger = ConversationLogger::new(&Privacy::default(), directory.path());
        logger
            .log("a", "p", "/v1/responses", b"request", b"response")
            .unwrap();
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn enabled_logger_writes_jsonl() {
        let directory = tempfile::tempdir().unwrap();
        let privacy = Privacy {
            log_conversations: true,
            conversation_log_directory: Some(directory.path().to_path_buf()),
            conversation_retention_days: 30,
            conversation_max_file_mb: 1,
        };
        let logger = ConversationLogger::new(&privacy, directory.path());
        logger
            .log("a", "p", "/v1/responses", b"request", b"response")
            .unwrap();
        let path = fs::read_dir(directory.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let contents = fs::read_to_string(path).unwrap();
        assert!(contents.contains("\"applicationId\":\"a\""));
        assert!(contents.contains("\"request\":{\"encoding\":\"utf8\",\"data\":\"request\"}"));
        assert!(matches!(encode_body(&[0xff]), EncodedBody::Base64(_)));
        assert!(logger.enabled());
    }

    #[test]
    fn rotates_and_prunes_conversation_files() {
        let directory = tempfile::tempdir().unwrap();
        let privacy = Privacy {
            log_conversations: true,
            conversation_log_directory: Some(directory.path().to_path_buf()),
            conversation_retention_days: 1,
            conversation_max_file_mb: 1,
        };
        let mut logger = ConversationLogger::new(&privacy, directory.path());
        let now = Utc::now();
        let first = logger.output_path(now).unwrap();
        fs::write(&first, vec![0; logger.max_file_bytes as usize]).unwrap();
        assert_ne!(logger.output_path(now).unwrap(), first);

        let old = directory.path().join("old.jsonl");
        fs::write(&old, "old").unwrap();
        let old_time = filetime::FileTime::from_unix_time(1, 0);
        filetime::set_file_mtime(&old, old_time).unwrap();
        fs::write(directory.path().join("keep.txt"), "keep").unwrap();
        logger.prune().unwrap();
        assert!(!old.exists());

        logger.retention_days = 0;
        logger.prune().unwrap();
        logger.max_file_bytes = 0;
        assert!(logger.output_path(now).is_err());
    }
}
