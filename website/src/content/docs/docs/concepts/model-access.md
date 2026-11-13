---
title: Model access
description: Control which models each application can list and call.
---

Every application connection uses one of three policies:

## Default model only

Only the effective provider’s configured default model is listed and permitted. This is the default for new connections.

## Selected models

Load the provider catalog and choose exact model IDs. An empty selection blocks every model request. Changing providers in the form clears the selection.

## All provider models

Agenthub discovers the effective provider’s complete model list on each request. Discovery failures return `502 model_discovery_failed`; Agenthub does not silently broaden or narrow access.

Every inference endpoint rejects disallowed models with `403 model_not_allowed` before forwarding. Multipart requests must include exactly one nonempty `model` field.
