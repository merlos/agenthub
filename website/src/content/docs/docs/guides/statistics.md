---
title: Statistics
description: Interpret dashboard and provider usage metrics.
---

The overview shows request count, success rate, input and output tokens, average latency, P95 latency, 30-day traffic, and recent requests.

Each provider has a dedicated statistics page containing:

- Current billing-period and retained-history summaries
- Input, output, and total tokens
- Requests and average tokens per request
- Average output tokens per second
- Daily consumption for 7–30 visible days
- Monthly consumption for the latest 12 calendar months

Billing start day is saved per provider from 1–31. Shorter months use their last day, and all boundaries use UTC.

Statistics include only traffic sent through Agenthub. Pruned metadata cannot be recovered, and providers that omit usage report zero tokens.

Average output tokens per second is the arithmetic mean of each request’s output tokens divided by total request duration, excluding zero-duration requests. It is not generation-only speed.
