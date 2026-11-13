---
title: Architecture
description: Understand Agenthub's local gateway and request flow.
---

Agenthub is a Tauri desktop application with a React interface and a Rust gateway. Each application connection owns an authenticated loopback listener. The Rust service selects a provider, enforces model policy, adapts requests when necessary, and records operational metadata.

## Request lifecycle

1. A client calls its assigned `127.0.0.1` or `::1` listener.
2. The listener validates the generated bearer key.
3. Agenthub resolves the application’s pinned provider or the current default.
4. Model access is checked before forwarding.
5. Compatible requests pass through; supported Anthropic or Gemini requests are translated.
6. Status, latency, token usage, byte counts, and request identity are persisted in SQLite.

The default provider request timeout is 30 minutes and can be changed with `server.request_timeout_seconds`.
