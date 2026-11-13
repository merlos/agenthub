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
- Provider requests allow up to 30 minutes by default; `server.request_timeout_seconds`
  in `config.toml` can override this for slower or faster environments.

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

Application connections have three model-access modes:

- **Default model only** (the default for new and migrated connections): only
  the effective provider's configured default model is listed and permitted.
- **Selected models**: load available models in the connection form and check
  the model IDs to allow. An empty selection blocks every model request.
- **All provider models**: discover the effective provider's complete model
  list on each request, including new models. Discovery failures return
  `502 model_discovery_failed`; no broader-access fallback is used. Providers
  without a working model-list endpoint must use default-only or selected access.

`GET /v1/models` returns only the enabled models. Every inference endpoint,
including streaming and multipart requests, rejects other models with
`403 model_not_allowed` before forwarding. JSON requests without a `model`
use the provider's default only if allowed; multipart requests must specify
exactly one nonempty `model` field. Invalid or uninspectable request bodies
return `400 invalid_request_error`.

Changing the provider in the form clears model selections. Connections using
the default provider follow changes to that provider; selected-model policies
continue to allow only their saved exact IDs, while all-model policies discover
the newly effective provider. Provider catalog membership does not guarantee
that a model supports a particular endpoint or that upstream access is granted.

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

JSON and multipart requests are inspected for model access; multipart payloads
and binary or server-sent-event responses pass through without conversion for
OpenAI-compatible providers. Streaming Anthropic and
Gemini translation is not yet supported. Image, audio, and embedding
availability depends on the selected upstream provider and model.

## Statistics

Request metadata is stored in SQLite and includes:

- Application, provider, model, endpoint, and modality
- HTTP status and categorized error
- Input and output tokens when reported by the provider
- Total duration, request bytes, response bytes, and timestamp

Streaming OpenAI-compatible chat requests ask the provider to include usage and
record usage events from the stream. Providers that do not report usage still
appear with zero token counts.

The overview displays request count, success rate, input/output tokens,
average latency, P95 latency, 30-day traffic, and recent requests. The Providers
page displays only total tokens across retained history and month tokens for
the current billing period. Each provider has a statistics icon linking to a
dedicated page with input, output, total tokens, average tokens per request,
and average output tokens per second for the billing period and retained history.
The billing start day is saved independently per provider (1–31, default 1);
shorter months use their last day. Changing it recalculates the billing period
without changing request history. Dates and billing boundaries use UTC.

Daily input/output consumption is shown for the latest days that fit the chart
(7–30 days), with exact counts and average output tokens/s on hover, keyboard
focus, or tap. Monthly input/output consumption covers the latest 12 calendar
months, including the partial current month. Dates without usage show zero.
Average tokens/request includes all recorded requests. Average output tokens/s
is the arithmetic mean of per-request output tokens divided by total request
duration, excluding zero-duration requests; it is not generation-only speed.
Statistics cover only traffic through agenthub. Metadata retention still applies:
previously pruned history cannot be recovered, and total tokens are not lifetime
totals when retention is limited.
Request dates use `yyyy-mm-dd` and durations use human-friendly units. The
request log auto-refreshes every 5 seconds by default. Use the Disable
auto-refresh button to pause updates, or Enable auto-refresh to resume.
Polling stops when leaving the page, and the text filter remains active during updates.
The request log can be filtered by its displayed metadata. Each log entry shows
input/output tokens for that request and the accumulated input/output totals.
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
