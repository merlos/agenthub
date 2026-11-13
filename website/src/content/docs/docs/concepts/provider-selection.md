---
title: Provider selection
description: Defaults, pinned providers, and fallback behavior.
---

The first configured provider becomes the default. A connection can either follow that default or remain pinned to one provider.

- **Default provider:** the application follows future changes to the global default.
- **Pinned provider:** requests continue to use that exact provider until the connection is edited.

Deleting the default promotes the next available provider. Applications that referenced the deleted provider fall back safely to the current default.

Model policy still applies after provider changes. Selected-model policies retain their saved exact IDs; all-model policies discover the newly effective provider’s catalog.
