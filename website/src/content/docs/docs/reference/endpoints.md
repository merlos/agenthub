---
title: API endpoints
description: OpenAI-compatible routes recognized by Agenthub.
---

Agenthub recognizes these routes on every application’s `/v1` base URL:

| Method | Endpoint |
| --- | --- |
| `GET` | `/v1/models` |
| `POST` | `/v1/chat/completions` |
| `POST` | `/v1/completions` |
| `POST` | `/v1/responses` |
| `POST` | `/v1/embeddings` |
| `POST` | `/v1/images/generations` |
| `POST` | `/v1/images/edits` |
| `POST` | `/v1/images/variations` |
| `POST` | `/v1/audio/speech` |
| `POST` | `/v1/audio/transcriptions` |
| `POST` | `/v1/audio/translations` |

JSON and multipart requests are inspected for model access. Multipart payloads and binary or server-sent-event responses pass through without conversion for OpenAI-compatible providers.

Endpoint availability ultimately depends on the selected upstream provider and model. Catalog membership does not guarantee that a model supports a particular endpoint or that your upstream account has access.
