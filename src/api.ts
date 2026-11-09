import { invoke } from "@tauri-apps/api/core";
import type { Application, DashboardStats, Provider, Settings } from "./types";

export const api = {
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
  settings: () => invoke<Settings>("get_settings"),
};
