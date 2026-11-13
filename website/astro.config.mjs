import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://merlos.github.io",
  base: "/agenthub",
  integrations: [
    starlight({
      title: "Agenthub",
      description: "A local, OpenAI-compatible gateway for desktop AI tools.",
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/merlos/agenthub" }],
      customCss: ["./src/styles/starlight.css"],
      sidebar: [
        { label: "Overview", items: [{ label: "Introduction", slug: "docs" }] },
        {
          label: "Getting started",
          items: [
            { label: "Installation", slug: "docs/getting-started/installation" },
            { label: "First provider", slug: "docs/getting-started/first-provider" },
            { label: "Connect an application", slug: "docs/getting-started/connect-an-application" },
            { label: "Client configuration", slug: "docs/getting-started/client-configuration" }
          ]
        },
        {
          label: "Concepts",
          items: [
            { label: "Architecture", slug: "docs/concepts/architecture" },
            { label: "Security", slug: "docs/concepts/security" },
            { label: "Provider selection", slug: "docs/concepts/provider-selection" },
            { label: "Model access", slug: "docs/concepts/model-access" }
          ]
        },
        {
          label: "Guides",
          items: [
            { label: "Applications", slug: "docs/guides/applications" },
            { label: "Providers", slug: "docs/guides/providers" },
            { label: "Statistics", slug: "docs/guides/statistics" },
            { label: "Request logs", slug: "docs/guides/request-logs" }
          ]
        },
        {
          label: "Reference",
          items: [
            { label: "API endpoints", slug: "docs/reference/endpoints" },
            { label: "Configuration", slug: "docs/reference/configuration" },
            { label: "Privacy and storage", slug: "docs/reference/privacy-and-storage" },
            { label: "Limitations", slug: "docs/reference/limitations" }
          ]
        }
      ]
    })
  ]
});
