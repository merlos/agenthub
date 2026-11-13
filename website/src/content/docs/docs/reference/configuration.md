---
title: Configuration
description: Advanced server, privacy, and retention settings.
---

Agenthub creates `~/.config/agenthub/config.toml` on Linux. macOS and Windows use their platform configuration directory. Edit the file and restart Agenthub for changes to take effect.

```toml
[server]
bind = "127.0.0.1"
port_start = 43100
port_end = 43999
request_timeout_seconds = 1800

[privacy]
log_conversations = false
# conversation_log_directory = "/path/to/logs"
conversation_retention_days = 30
conversation_max_file_mb = 10

[retention]
# Zero retains request metadata indefinitely.
request_metadata_days = 0
```

## Server

The bind address must remain loopback. The port range controls automatic application endpoint allocation. Request timeout applies while waiting for upstream providers.

## Privacy

Conversation logging is disabled by default. Its directory, retention, and rotation size apply only after logging is enabled.

## Retention

Request metadata retention controls SQLite statistics. Set it to zero for unlimited retention.
