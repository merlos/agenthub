---
title: Client configuration
description: Point an OpenAI-compatible client at Agenthub.
---

Configure the client with the base URL and key shown on its Agenthub application card. For clients that read OpenAI environment variables:

```bash
export OPENAI_BASE_URL="http://localhost:43100/v1"
export OPENAI_API_KEY="ah_..."
```

Every application has a different port and key. Do not reuse a connection for unrelated tools if you want independent revocation and usage identity.

## Model selection

The model sent by the client must comply with that application’s [model-access policy](../concepts/model-access/). A request without a JSON `model` uses the effective provider’s default only when that model is allowed.

Use `GET /v1/models` against the application endpoint to inspect the models visible to that client.
