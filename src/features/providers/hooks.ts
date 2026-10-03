import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { modelKeys } from "@/features/models";
import { providerApi } from "./api";
import type { ProviderView } from "./types";

export const providerKeys = {
  all: ["providers"] as const,
};

export function useProviders() {
  return useQuery({ queryKey: providerKeys.all, queryFn: providerApi.list, meta: { errorPrefix: "provider.loadFailed" } });
}

function useInvalidateProviderData() {
  const client = useQueryClient();
  return () => Promise.all([
    client.invalidateQueries({ queryKey: providerKeys.all }),
    client.invalidateQueries({ queryKey: modelKeys.all }),
  ]);
}

/** 保存整份 Provider（含模型映射）；同时刷新标准模型的 provider_count。 */
export function useSaveProvider() {
  const invalidate = useInvalidateProviderData();
  return useMutation({
    mutationFn: (provider: ProviderView) => providerApi.save(provider, provider.models),
    onSuccess: invalidate,
  });
}

export function useDeleteProvider() {
  const invalidate = useInvalidateProviderData();
  return useMutation({ mutationFn: (id: string) => providerApi.remove(id), onSuccess: invalidate });
}

/** 导出参数：加密字符串，以及原生保存对话框标题。 */
export interface ProviderConfigExportRequest {
  passphrase: string;
  title: string;
}

/** 导入参数：弹窗内选中的文件内容与文件名，加上解密用的加密字符串。 */
export interface ProviderConfigImportRequest {
  passphrase: string;
  contents: string;
  filename: string;
}

/** 导出加密配置：只读取本地数据，不修改任何配置，因此无需刷新查询。 */
export function useExportProviderConfig() {
  return useMutation({
    mutationFn: (request: ProviderConfigExportRequest) => providerApi.exportConfig(request.passphrase, request.title),
  });
}

/** 导入加密配置：会写入 Provider、模型映射与标准模型，成功后刷新本地数据。 */
export function useImportProviderConfig() {
  const invalidate = useInvalidateProviderData();
  return useMutation({
    mutationFn: (request: ProviderConfigImportRequest) => providerApi.importConfig(request.passphrase, request.contents, request.filename),
    onSuccess: invalidate,
  });
}
