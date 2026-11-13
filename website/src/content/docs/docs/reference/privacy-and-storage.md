---
title: Privacy and storage
description: What Agenthub stores and where it is kept.
---

| Location | Contents |
| --- | --- |
| OS keyring | Provider API credentials |
| `agenthub.db` | Provider metadata, applications, and request statistics |
| `config.toml` | Advanced server, privacy, and retention settings |
| `conversations/*.jsonl` | Optional request and response bodies |

Request and response bodies are never written to SQLite. Conversation logging must be explicitly enabled and writes rotating JSONL files that can contain sensitive information.

Streaming bodies are captured only when conversation logging is enabled. Non-UTF-8 bodies, such as generated audio, are represented as base64 records.

Provider secrets must never appear in SQLite, TOML, logs, frontend state, or command responses.
