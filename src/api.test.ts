import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

describe("api", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));
  it("maps every operation to its Tauri command", async () => {
    const { api, selectApi, tauriApi } = await import("./api");
    const provider = { id: "p", name: "P", kind: "openai" as const, baseUrl: "u", defaultModel: "m", isDefault: true };
    const app = { id: "a", name: "A", port: 1, apiKey: "k", providerId: null, enabled: true };
    await api.providers(); await api.saveProvider(provider); await api.providerModels(provider); await api.deleteProvider("p"); await api.setDefaultProvider("p");
    await api.applications(); await api.saveApplication(app); await api.deleteApplication("a"); await api.regenerateKey("a");
    await api.stats(); await api.settings(); await api.providerUsage(); await api.providerStats("p"); await api.setProviderBillingStartDay("p", 15);
    expect(invoke.mock.calls.map(c => c[0])).toEqual(["list_providers","save_provider","list_provider_models","delete_provider","set_default_provider","list_applications","save_application","delete_application","regenerate_application_key","dashboard_stats","get_settings","list_provider_usage","provider_stats","set_provider_billing_start_day"]);
    expect(invoke).toHaveBeenCalledWith("provider_stats", { id: "p" });
    expect(invoke).toHaveBeenCalledWith("set_provider_billing_start_day", { id: "p", day: 15 });
    expect(invoke).toHaveBeenCalledWith("delete_application", { id: "a" });
    expect(selectApi(false)).toBe(tauriApi);
    expect(selectApi(true)).not.toBe(tauriApi);
  });
});
