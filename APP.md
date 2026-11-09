# Agenthub functionality

Agenthub is a local Tauri gateway. Each registered application receives an
isolated loopback port and bearer token and uses Agenthub as an
OpenAI-compatible API. Agenthub selects a configured provider, adapts the
request where necessary, and records operational metadata.

## Applications

- Create, edit, enable, disable, and delete application endpoints.
- Allocate one port and cryptographically random 320-bit key per application.
- Regenerate keys when client credentials need to be revoked.
- Pin an application to a provider or follow the current default provider.
- Bind listeners only to `127.0.0.1` or `::1`.

## Providers

- Native configuration presets for OpenAI, Azure OpenAI, Anthropic, Google
  Gemini, and Amazon Bedrock.
- Presets for OpenRouter, DeepSeek, and local Ollama (`http://localhost:11434/v1`).
- Generic OpenAI-compatible endpoint support, including loopback services.
- Provider credentials are write-only in the UI and stored in the OS keyring.
- After entering the API key, available models can be loaded from each provider
  and selected as the default; manual model or deployment IDs remain supported.
- The first provider becomes the default. Deleting the default promotes the
  next provider and affected applications fall back safely.

Anthropic and Gemini currently translate non-streaming OpenAI chat requests.
Other listed providers use OpenAI-compatible pass-through. Provider capability
errors are returned explicitly instead of sending an invalid upstream request.

## OpenAI-compatible API

The local API recognizes:

- `GET /v1/models`
- `POST /v1/chat/completions`
- `POST /v1/completions`
- `POST /v1/responses`
- `POST /v1/embeddings`
- `POST /v1/images/generations`
- `POST /v1/images/edits`
- `POST /v1/images/variations`
- `POST /v1/audio/speech`
- `POST /v1/audio/transcriptions`
- `POST /v1/audio/translations`

JSON, multipart, binary, and server-sent-event bodies pass through without
being converted for OpenAI-compatible providers. Streaming Anthropic and
Gemini translation is not yet supported. Image, audio, and embedding
availability depends on the selected upstream provider and model.

## Statistics

Request metadata is stored in SQLite and includes:

- Application, provider, model, endpoint, and modality
- HTTP status and categorized error
- Input and output tokens when reported by the provider
- Total duration, request bytes, response bytes, and timestamp

The overview displays request count, success rate, input/output tokens,
average latency, P95 latency, 30-day traffic, and recent requests.
Authentication failures are recorded without storing the supplied credential.

## Privacy and storage

Conversation logging is disabled by default and can only be enabled in
`config.toml`. When enabled, request and response bodies are written to rotated
JSONL files with configurable retention. Bodies are never written to SQLite.
Streaming bodies are captured only when conversation logging is enabled.
Non-UTF-8 bodies, such as generated audio, are represented as base64 records.

- `config.toml`: advanced server, privacy, and retention settings
- `agenthub.db`: provider metadata, applications, and request statistics
- `conversations/*.jsonl`: optional conversation logs
- OS keyring: provider API credentials

Configuration changes require an application restart.
