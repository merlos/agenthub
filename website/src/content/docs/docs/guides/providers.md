---
title: Providers
description: Configure credentials, endpoints, defaults, and models.
---

Agenthub supports OpenAI, Azure OpenAI, Anthropic, Google Gemini, Amazon Bedrock, OpenRouter, DeepSeek, Ollama, and generic OpenAI-compatible endpoints.

Provider configuration includes a display name, kind, base URL, default model, and optional secret. Use **Load models** after supplying credentials to discover available IDs, or enter a deployment or model ID manually.

The first provider becomes the default. Select **Make default** on another provider to change where unpinned applications send requests.

## Adaptation

Anthropic and Gemini support translated, non-streaming OpenAI chat requests. OpenAI-compatible providers use pass-through behavior. Capability errors are returned explicitly rather than forwarding an invalid upstream request.
