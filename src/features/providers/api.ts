import { invoke } from "@/shared/api";
import type { ModelMapping, Provider, ProviderSaveResult, ProviderView } from "./types";

declare module "@/shared/api/tauri" {
  interface CommandMap {
    list_providers: { args: void; result: ProviderView[] };
    save_provider: { args: { provider: Provider; models: ModelMapping[] }; result: ProviderSaveResult };
    delete_provider: { args: { id: string }; result: void };
    test_provider: { args: { provider: Provider; models: ModelMapping[]; model: string }; result: string };
  }
}

export const providerApi = {
  list: () => invoke("list_providers"),
  save: (provider: Provider, models: ModelMapping[]) => invoke("save_provider", { provider, models }),
  remove: (id: string) => invoke("delete_provider", { id }),
  test: (provider: Provider, models: ModelMapping[], model: string) => invoke("test_provider", { provider, models, model }),
};
