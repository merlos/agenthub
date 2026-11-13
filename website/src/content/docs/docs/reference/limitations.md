---
title: Limitations
description: Current capability boundaries and expected behavior.
---

- Agenthub supports local loopback access only. Remote exposure is out of scope.
- Anthropic and Gemini translation is limited to non-streaming OpenAI chat requests.
- Other supported providers rely on their OpenAI-compatible APIs.
- Image, audio, and embedding support depends on the upstream provider and selected model.
- A provider listing a model does not guarantee endpoint support or account access.
- Providers that omit usage still appear in statistics with zero token counts.
- Metadata removed by retention cannot be reconstructed; retained totals are not necessarily lifetime totals.
- Providers without a working model-list endpoint cannot use **All provider models** and should use default-only or selected-model policy.
- Configuration file changes require an application restart.

Capability errors are returned explicitly rather than sending a known-invalid upstream request.
