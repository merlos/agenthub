import type { AgenthubApi } from "./api";
import type { Application, DashboardStats, Provider, ProviderStats, RequestStat, TokenBucket } from "./types";

const copy = <T>(value: T): T => structuredClone(value);
const isoDay = (date: Date) => date.toISOString().slice(0, 10);
const day = (offset: number) => { const value = new Date(); value.setUTCHours(12, 0, 0, 0); value.setUTCDate(value.getUTCDate() + offset); return isoDay(value); };

let providers: Provider[] = [
  { id: "demo-openai", name: "OpenAI", kind: "openai", baseUrl: "https://api.openai.com/v1", defaultModel: "gpt-5-mini", isDefault: true, hasSecret: true },
  { id: "demo-anthropic", name: "Anthropic", kind: "anthropic", baseUrl: "https://api.anthropic.com/v1", defaultModel: "claude-sonnet-4-5", isDefault: false, hasSecret: true },
  { id: "demo-ollama", name: "Ollama", kind: "ollama", baseUrl: "http://localhost:11434/v1", defaultModel: "qwen3:8b", isDefault: false, hasSecret: false },
];

let applications: Application[] = [
  { id: "demo-opencode", name: "OpenCode", port: 43100, apiKey: "ah_demo_opencode_not_a_real_key", providerId: null, enabled: true, modelAccess: "defaultOnly", allowedModels: [] },
  { id: "demo-cursor", name: "Cursor", port: 43101, apiKey: "ah_demo_cursor_not_a_real_key", providerId: "demo-anthropic", enabled: true, modelAccess: "selected", allowedModels: ["claude-sonnet-4-5", "claude-haiku-4-5"] },
  { id: "demo-continue", name: "Continue", port: 43102, apiKey: "ah_demo_continue_not_a_real_key", providerId: "demo-ollama", enabled: false, modelAccess: "all", allowedModels: [] },
];

const requestTemplates = [
  ["OpenCode", "OpenAI", "gpt-5-mini", "/chat/completions", "text", 200, 1840, 622, 1280],
  ["Cursor", "Anthropic", "claude-sonnet-4-5", "/chat/completions", "text", 200, 3210, 945, 2140],
  ["OpenCode", "OpenAI", "text-embedding-3-small", "/embeddings", "embedding", 200, 768, 0, 186],
  ["Cursor", "Anthropic", "claude-haiku-4-5", "/chat/completions", "text", 429, 920, 0, 412],
  ["OpenCode", "OpenAI", "gpt-5-mini", "/responses", "text", 200, 1442, 511, 970],
] as const;

const recent: RequestStat[] = requestTemplates.map((item, index) => ({
  id: index + 1, application: item[0], provider: item[1], model: item[2], endpoint: item[3], modality: item[4], status: item[5],
  tokensIn: item[6], tokensOut: item[7], accumulatedTokensIn: 184_320 - index * 8_100, accumulatedTokensOut: 72_840 - index * 3_240,
  durationMs: item[8], requestBytes: 2_400 + index * 310, responseBytes: 4_800 + index * 720,
  errorType: item[5] >= 400 ? "rate_limit" : null, createdAt: `${day(-index)}T${String(14 - index).padStart(2, "0")}:24:00Z`,
}));

const dailyTraffic = Array.from({ length: 30 }, (_, index) => ({ day: day(index - 29), requests: 42 + ((index * 37) % 96) }));
const summary = (requests: number, tokensIn: number, tokensOut: number, speed: number) => ({ requests, tokensIn, tokensOut, totalTokens: tokensIn + tokensOut, averageTokens: Math.round((tokensIn + tokensOut) / requests), averageTokensPerSecond: speed });
const tokenBucket = (date: string, index: number): TokenBucket => ({ date, ...summary(30 + index % 17, 18_000 + (index * 791) % 16_000, 7_000 + (index * 433) % 9_000, 31 + index % 13) });
const providerNames: Record<string, string> = { "demo-openai": "OpenAI", "demo-anthropic": "Anthropic", "demo-ollama": "Ollama" };
const billingDays: Record<string, number> = { "demo-openai": 1, "demo-anthropic": 15, "demo-ollama": 1 };
const billingDate = (year: number, month: number, requestedDay: number) => new Date(Date.UTC(year, month, Math.min(requestedDay, new Date(Date.UTC(year, month + 1, 0)).getUTCDate())));

const buildProviderStats = (id: string): ProviderStats => {
  const billingStartDay = billingDays[id] ?? 1;
  const today = new Date();
  let start = billingDate(today.getUTCFullYear(), today.getUTCMonth(), billingStartDay);
  if (today < start) start = billingDate(today.getUTCFullYear(), today.getUTCMonth() - 1, billingStartDay);
  const end = billingDate(start.getUTCFullYear(), start.getUTCMonth() + 1, billingStartDay); end.setUTCDate(end.getUTCDate() - 1);
  return {
    providerId: id, providerName: providerNames[id] ?? "Demo provider", billingStartDay, periodStart: isoDay(start), periodEnd: isoDay(end),
    current: summary(1_284, 1_842_650, 728_420, 38.7), historical: summary(8_946, 12_482_940, 5_106_380, 36.2),
    daily: Array.from({ length: 30 }, (_, index) => tokenBucket(day(index - 29), index)),
    monthly: Array.from({ length: 12 }, (_, index) => { const date = new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth() - 11 + index, 1)); return tokenBucket(isoDay(date).slice(0, 7), index * 11); }),
  };
};

const dashboard: DashboardStats = {
  requests: 8_946, successRate: 98.7, tokensIn: 12_482_940, tokensOut: 5_106_380, averageDurationMs: 1_240, p95DurationMs: 3_860,
  providerTokens: [
    { provider: "OpenAI", tokensIn: 7_240_000, tokensOut: 2_980_000, averageTokensPerSecond: 42.4 },
    { provider: "Anthropic", tokensIn: 4_610_000, tokensOut: 1_940_000, averageTokensPerSecond: 34.8 },
    { provider: "Ollama", tokensIn: 632_940, tokensOut: 186_380, averageTokensPerSecond: 18.2 },
  ], daily: dailyTraffic, recent,
};

export const demoApi: AgenthubApi = {
  async providers() { return copy(providers); },
  async saveProvider(provider) {
    const saved: Provider = { ...provider, id: provider.id || `demo-provider-${providers.length + 1}`, hasSecret: Boolean(provider.apiKey) || providers.find(item => item.id === provider.id)?.hasSecret || false };
    delete (saved as Provider & { apiKey?: string }).apiKey;
    providers = [...providers.filter(item => item.id !== saved.id), saved]; providerNames[saved.id] = saved.name; return copy(saved);
  },
  async providerModels(provider) { return provider.kind === "anthropic" ? ["claude-sonnet-4-5", "claude-haiku-4-5"] : provider.kind === "ollama" ? ["qwen3:8b", "gemma3:12b"] : ["gpt-5-mini", "gpt-5", "text-embedding-3-small"]; },
  async deleteProvider(id) { providers = providers.filter(provider => provider.id !== id); },
  async setDefaultProvider(id) { providers = providers.map(provider => ({ ...provider, isDefault: provider.id === id })); },
  async applications() { return copy(applications); },
  async saveApplication(application) {
    const existing = applications.find(item => item.id === application.id);
    const saved: Application = { id: application.id || `demo-app-${applications.length + 1}`, name: application.name, port: application.port ?? existing?.port ?? 43100 + applications.length, apiKey: application.apiKey ?? existing?.apiKey ?? "ah_demo_not_a_real_key", providerId: application.providerId ?? null, enabled: application.enabled ?? true, modelAccess: application.modelAccess ?? "defaultOnly", allowedModels: application.allowedModels ?? [] };
    applications = [...applications.filter(item => item.id !== saved.id), saved]; return copy(saved);
  },
  async deleteApplication(id) { applications = applications.filter(application => application.id !== id); },
  async regenerateKey(id) { const application = applications.find(item => item.id === id)!; return this.saveApplication({ ...application, apiKey: "ah_demo_regenerated_not_a_real_key" }); },
  async stats() { return copy(dashboard); },
  async providerUsage() { return providers.map((provider, index) => ({ providerId: provider.id, totalTokens: 8_200_000 - index * 2_430_000, monthTokens: 1_940_000 - index * 510_000, periodStart: day(-29), periodEnd: day(0) })); },
  async providerStats(id) { return buildProviderStats(id); },
  async setProviderBillingStartDay(id, value) { billingDays[id] = value; },
  async settings() { return { logConversations: false, configPath: "~/.config/agenthub-demo/config.toml", conversationLogPath: "~/.config/agenthub-demo/conversations", conversationRetentionDays: 30, requestMetadataDays: 90 }; },
};
