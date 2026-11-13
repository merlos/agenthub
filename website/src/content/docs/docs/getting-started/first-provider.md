---
title: Add your first provider
description: Configure a cloud or local model provider.
---

1. Open **Providers** and select **Add provider**.
2. Choose a preset or **Compatible** for another OpenAI-compatible API.
3. Confirm the base URL and enter a display name.
4. Enter the API key when the provider requires one.
5. Optionally use **Load models** to discover model IDs.
6. Choose the default model and save.

The first provider becomes the default automatically. Provider API keys are write-only in the interface and stored in your operating system keyring.

## Supported presets

Agenthub includes presets for OpenAI, Azure OpenAI, Anthropic, Google Gemini, Amazon Bedrock, OpenRouter, DeepSeek, Ollama, and generic OpenAI-compatible services.

:::caution
Anthropic and Gemini currently translate non-streaming OpenAI chat requests. Streaming translation is not supported. Other listed providers use OpenAI-compatible pass-through.
:::

Next, [connect an application](./connect-an-application/).
