# Agenthub engineering guide

## Purpose

Agenthub is a local AI gateway. Desktop tools connect to a per-application
OpenAI-compatible endpoint; agenthub authenticates the tool, selects a
provider, translates when needed, and records operational metadata.

## Architecture

- `src/`: React/TypeScript presentation layer. `api.ts` is the only Tauri IPC
  boundary. Components do not access storage or provider APIs directly.
- `src-tauri/src/lib.rs`: Tauri composition root and IPC commands.
- `src-tauri/src/proxy.rs`: localhost listeners, authentication, endpoint
  capabilities, streaming, provider adaptation, and metrics recording.
- `src-tauri/src/db.rs`: SQLite repository for providers, applications, and
  request statistics, including versioned schema migrations.
- `src-tauri/src/config.rs`: advanced file-based settings.
- `src-tauri/src/conversation_log.rs`: opt-in rotating JSONL conversation logs.
- OS keyring: provider secrets. Secrets must never be stored in SQLite, TOML,
  logs, frontend state, or command responses.

## Data and request flow

1. An application sends an OpenAI-compatible request to its assigned port.
2. The listener validates its generated bearer key.
3. Its explicit provider or the default provider is selected.
4. The adapter forwards or translates the request.
5. Metadata is persisted in SQLite. Bodies are written to JSONL only when
   `privacy.log_conversations` is explicitly enabled.

## Conventions

- Keep modules narrow and avoid provider-specific logic outside `proxy.rs`.
- Serialize IPC fields as `camelCase`; Rust internals use `snake_case`.
- Return actionable errors without credentials, prompts, or responses.
- Bind to loopback by default. Any change to this default requires a security
  review and explicit user documentation.
- Use parameterized SQL only. Schema changes require a migration strategy.
- Never store request or response bodies in SQLite.
- Keep loopback binding validation in place; remote exposure is out of scope.
- Keep UI controls keyboard accessible and preserve visible focus states.
- Update `APP.md` whenever behavior, support, or limitations change.
- Add unit tests for every branch of parsing, adaptation, persistence, and
  business logic. CI must enforce 100% coverage on testable project code.

## Checks

```bash
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```
