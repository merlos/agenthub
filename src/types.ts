export type ProviderKind = "openai" | "anthropic" | "google" | "azure" | "aws" | "openrouter" | "deepseek" | "ollama" | "compatible";

export interface Provider {
  id: string;
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  defaultModel: string;
  isDefault: boolean;
  hasSecret: boolean;
}

export interface Application {
  id: string;
  name: string;
  port: number;
  apiKey: string;
  providerId: string | null;
  enabled: boolean;
}

export interface DashboardStats {
  requests: number;
  successRate: number;
  tokensIn: number;
  tokensOut: number;
  averageDurationMs: number;
  p95DurationMs: number;
  providerTokens: Array<{ provider: string; tokensIn: number; tokensOut: number }>;
  daily: Array<{ day: string; requests: number }>;
  recent: RequestStat[];
}

export interface RequestStat {
  id: number;
  application: string;
  provider: string;
  model: string;
  endpoint: string;
  modality: string;
  status: number;
  tokensIn: number;
  tokensOut: number;
  accumulatedTokensIn: number;
  accumulatedTokensOut: number;
  durationMs: number;
  requestBytes: number;
  responseBytes: number;
  errorType: string | null;
  createdAt: string;
}

export interface Settings {
  logConversations: boolean;
  configPath: string;
  conversationLogPath: string;
  conversationRetentionDays: number;
  requestMetadataDays: number;
}
