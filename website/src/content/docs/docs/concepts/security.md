---
title: Security model
description: How Agenthub protects local endpoints and provider credentials.
---

## Local listeners

Application endpoints bind only to loopback addresses. Remote exposure is outside Agenthub’s supported scope. Each application receives its own generated 320-bit bearer key.

## Provider credentials

Provider secrets are stored in the operating system keyring. They are never stored in SQLite, TOML configuration, conversation logs, frontend state, or command responses. Existing keys are write-only in the UI.

## Stored data

SQLite contains provider metadata, application configuration, and request statistics—but never request or response bodies. Optional conversation logs are separate rotated JSONL files and are disabled by default.

:::danger
Conversation logs can contain sensitive prompts and responses. Enable them only when you understand and accept that risk.
:::
