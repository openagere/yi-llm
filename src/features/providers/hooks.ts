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
