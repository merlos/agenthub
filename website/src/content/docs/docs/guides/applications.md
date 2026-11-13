---
title: Applications
description: Manage authenticated endpoints for desktop tools.
---

Applications represent the clients that send requests through Agenthub. You can create, edit, enable, disable, and delete their endpoints independently.

Each application stores:

- A display name
- A dedicated loopback port
- A generated bearer key
- A pinned provider or default-provider selection
- A model-access policy and optional model allowlist

Disabling an application stops its listener while retaining its configuration. Deleting it removes the endpoint. Regenerating the key revokes every client still using the old value.

Provider requests allow up to 30 minutes by default, accommodating slower local models. Adjust the timeout in [configuration](../reference/configuration/).
