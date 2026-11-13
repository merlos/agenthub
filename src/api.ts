import { invoke } from "@tauri-apps/api/core";
import type { Application, DashboardStats, Provider, ProviderStats, ProviderUsage, Settings } from "./types";
import { demoApi } from "./demoApi";

export interface AgenthubApi {
  providers(): Promise<Provider[]>;
  saveProvider(provider: Omit<Provider, "hasSecret"> & { apiKey?: string }): Promise<Provider>;
  providerModels(provider: Omit<Provider, "hasSecret"> & { apiKey?: string }): Promise<string[]>;
  deleteProvider(id: string): Promise<void>;
  setDefaultProvider(id: string): Promise<void>;
  applications(): Promise<Application[]>;
  saveApplication(application: Partial<Application> & Pick<Application, "name">): Promise<Application>;
  deleteApplication(id: string): Promise<void>;
  regenerateKey(id: string): Promise<Application>;
  stats(): Promise<DashboardStats>;
  providerUsage(): Promise<ProviderUsage[]>;
  providerStats(id: string): Promise<ProviderStats>;
  setProviderBillingStartDay(id: string, day: number): Promise<void>;
  settings(): Promise<Settings>;
}

export const tauriApi: AgenthubApi = {
  providers: () => invoke<Provider[]>("list_providers"),
  saveProvider: (provider: Omit<Provider, "hasSecret"> & { apiKey?: string }) => invoke<Provider>("save_provider", { provider }),
  providerModels: (provider: Omit<Provider, "hasSecret"> & { apiKey?: string }) => invoke<string[]>("list_provider_models", { provider }),
  deleteProvider: (id: string) => invoke<void>("delete_provider", { id }),
  setDefaultProvider: (id: string) => invoke<void>("set_default_provider", { id }),
  applications: () => invoke<Application[]>("list_applications"),
  saveApplication: (application: Partial<Application> & Pick<Application, "name">) => invoke<Application>("save_application", { application }),
  deleteApplication: (id: string) => invoke<void>("delete_application", { id }),
  regenerateKey: (id: string) => invoke<Application>("regenerate_application_key", { id }),
  stats: () => invoke<DashboardStats>("dashboard_stats"),
  providerUsage: () => invoke<ProviderUsage[]>("list_provider_usage"),
  providerStats: (id: string) => invoke<ProviderStats>("provider_stats", { id }),
  setProviderBillingStartDay: (id: string, day: number) => invoke<void>("set_provider_billing_start_day", { id, day }),
  settings: () => invoke<Settings>("get_settings"),
};

export const selectApi = (demo: boolean): AgenthubApi => demo ? demoApi : tauriApi;
export const api: AgenthubApi = selectApi(import.meta.env.VITE_DEMO_MODE === "true");
