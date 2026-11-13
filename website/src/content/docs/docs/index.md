---
title: Agenthub documentation
description: Learn how to install, configure, secure, and use Agenthub.
template: splash
hero:
  tagline: A local OpenAI-compatible gateway that gives every desktop AI tool an isolated, authenticated endpoint.
  actions:
    - text: Install Agenthub
      link: /agenthub/docs/getting-started/installation/
      icon: right-arrow
    - text: View on GitHub
      link: https://github.com/merlos/agenthub
      variant: minimal
      icon: external
---

import { Card, CardGrid } from '@astrojs/starlight/components';

<CardGrid>
  <Card title="Isolated applications" icon="seti:config">
    Give each application its own loopback port and generated bearer key.
  </Card>
  <Card title="Central providers" icon="seti:database">
    Configure provider credentials once and keep them in the operating system keyring.
  </Card>
  <Card title="Explicit model policy" icon="seti:lock">
    Permit only a default model, an exact allowlist, or the provider’s discovered catalog.
  </Card>
  <Card title="Operational visibility" icon="seti:log">
    Inspect request status, latency, token consumption, and provider usage without storing bodies in SQLite.
  </Card>
</CardGrid>

## How requests flow

1. A desktop application sends an OpenAI-compatible request to its assigned local port.
2. Agenthub authenticates the application using its generated bearer key.
3. The application’s explicit provider, or the current default provider, is selected.
4. Agenthub validates model access and forwards or translates the request.
5. Operational metadata is recorded. Request and response bodies are logged only when conversation logging is explicitly enabled.

Start with [installation](./getting-started/installation/), then add your [first provider](./getting-started/first-provider/).
