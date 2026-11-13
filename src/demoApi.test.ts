import { describe, expect, it } from "vitest";
import { demoApi } from "./demoApi";
import type { Provider } from "./types";

describe("demo api", () => {
  it("provides complete, isolated marketing data", async () => {
    const providers = await demoApi.providers();
    expect(providers.map(provider => provider.name)).toEqual(["OpenAI", "Anthropic", "Ollama"]);
    providers[0].name = "Changed copy";
    expect((await demoApi.providers())[0].name).toBe("OpenAI");

    const applications = await demoApi.applications();
    expect(applications).toHaveLength(3);
    applications[0].name = "Changed copy";
    expect((await demoApi.applications())[0].name).toBe("OpenCode");

    const dashboard = await demoApi.stats();
    expect(dashboard.daily).toHaveLength(30);
    expect(dashboard.recent).toHaveLength(5);
    dashboard.recent[0].application = "Changed copy";
    expect((await demoApi.stats()).recent[0].application).toBe("OpenCode");

    expect(await demoApi.providerUsage()).toHaveLength(3);
    expect(await demoApi.settings()).toMatchObject({ logConversations: false, requestMetadataDays: 90 });
  });

  it("supports provider interactions without external services", async () => {
    const input = { id: "", name: "Demo compatible", kind: "compatible" as const, baseUrl: "http://localhost:9000/v1", defaultModel: "demo-model", isDefault: false };
    const created = await demoApi.saveProvider({ ...input, apiKey: "fake" });
    expect(created).toMatchObject({ id: "demo-provider-4", hasSecret: true });
    expect(created).not.toHaveProperty("apiKey");
    expect(await demoApi.providerModels({ ...created })).toEqual(["gpt-5-mini", "gpt-5", "text-embedding-3-small"]);

    const updated = await demoApi.saveProvider({ ...created, name: "Updated demo" });
    expect(updated.hasSecret).toBe(true);
    expect((await demoApi.providers()).find(provider => provider.id === created.id)?.name).toBe("Updated demo");

    const blank: Omit<Provider, "hasSecret"> = { ...input, id: "blank-provider", name: "Blank" };
    expect((await demoApi.saveProvider(blank)).hasSecret).toBe(false);
    expect(await demoApi.providerModels({ ...blank, kind: "anthropic" })).toEqual(["claude-sonnet-4-5", "claude-haiku-4-5"]);
    expect(await demoApi.providerModels({ ...blank, kind: "ollama" })).toEqual(["qwen3:8b", "gemma3:12b"]);

    await demoApi.setDefaultProvider(created.id);
    expect((await demoApi.providers()).find(provider => provider.id === created.id)?.isDefault).toBe(true);
    await demoApi.deleteProvider("blank-provider");
    expect((await demoApi.providers()).some(provider => provider.id === "blank-provider")).toBe(false);
  });

  it("supports application interactions in memory", async () => {
    const created = await demoApi.saveApplication({ name: "Marketing app" });
    expect(created).toMatchObject({ id: "demo-app-4", port: 43103, enabled: true, providerId: null, modelAccess: "defaultOnly", allowedModels: [] });

    const updated = await demoApi.saveApplication({ ...created, name: "Edited app", enabled: false, providerId: "demo-openai", modelAccess: "selected", allowedModels: ["gpt-5-mini"] });
    expect(updated).toMatchObject({ name: "Edited app", enabled: false, providerId: "demo-openai", modelAccess: "selected" });

    const regenerated = await demoApi.regenerateKey(created.id);
    expect(regenerated.apiKey).toBe("ah_demo_regenerated_not_a_real_key");
    await demoApi.deleteApplication(created.id);
    expect((await demoApi.applications()).some(application => application.id === created.id)).toBe(false);
  });

  it("updates provider billing periods", async () => {
    const initial = await demoApi.providerStats("demo-openai");
    expect(initial.daily).toHaveLength(30);
    expect(initial.monthly).toHaveLength(12);
    await demoApi.setProviderBillingStartDay("demo-openai", 15);
    expect((await demoApi.providerStats("demo-openai")).billingStartDay).toBe(15);
    await demoApi.setProviderBillingStartDay("demo-openai", 31);
    expect((await demoApi.providerStats("demo-openai")).billingStartDay).toBe(31);
    expect((await demoApi.providerStats("unknown")).providerName).toBe("Demo provider");
  });
});
