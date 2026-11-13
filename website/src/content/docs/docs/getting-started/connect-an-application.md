---
title: Connect an application
description: Create an isolated endpoint for a desktop AI tool.
---

1. Open **Applications** and select **Add application**.
2. Give the connection a recognizable name, such as `OpenCode` or `Cursor`.
3. Use the default provider or pin the application to a specific provider.
4. Select a model-access policy.
5. Save the application.

Agenthub allocates a loopback port and creates a cryptographically random 320-bit bearer key. The application card displays the values needed by your client:

```text
Base URL: http://localhost:43100/v1
API key:  ah_...
```

You can stop an endpoint without deleting it. Regenerating its key immediately invalidates the previous client configuration.

See [client configuration](./client-configuration/) for the values most OpenAI-compatible tools expect.
