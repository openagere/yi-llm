import { invoke } from "@/shared/api";
import type { ModelMapping, Provider, ProviderConfigTransferResult, ProviderSaveResult, ProviderView } from "./types";

declare module "@/shared/api/tauri" {
  interface CommandMap {
    list_providers: { args: void; result: ProviderView[] };
    save_provider: { args: { provider: Provider; models: ModelMapping[] }; result: ProviderSaveResult };
    delete_provider: { args: { id: string }; result: void };
    test_provider: { args: { provider: Provider; models: ModelMapping[]; model: string }; result: string };
    /** 导出加密的 Provider 配置；用户取消保存对话框时返回 null。 */
    export_provider_config: { args: { passphrase: string; title: string }; result: ProviderConfigTransferResult | null };
    /** 导入加密的 Provider 配置。文件由前端选择后把内容交给后端解密。 */
    import_provider_config: { args: { passphrase: string; contents: string; filename: string }; result: ProviderConfigTransferResult };
  }
}

export const providerApi = {
  list: () => invoke("list_providers"),
  save: (provider: Provider, models: ModelMapping[]) => invoke("save_provider", { provider, models }),
  remove: (id: string) => invoke("delete_provider", { id }),
  test: (provider: Provider, models: ModelMapping[], model: string) => invoke("test_provider", { provider, models, model }),
  exportConfig: (passphrase: string, title: string) => invoke("export_provider_config", { passphrase, title }),
  importConfig: (passphrase: string, contents: string, filename: string) => invoke("import_provider_config", { passphrase, contents, filename }),
};
