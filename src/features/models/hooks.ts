import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { modelApi } from "./api";
import type { StandardModel } from "./types";

export const modelKeys = {
  all: ["standard-models"] as const,
  catalogInfo: ["standard-models", "catalog-info"] as const,
};

export function useStandardModels() {
  return useQuery({ queryKey: modelKeys.all, queryFn: modelApi.list, meta: { errorPrefix: "model.loadFailed" } });
}

export function useModelCatalogInfo() {
  return useQuery({ queryKey: modelKeys.catalogInfo, queryFn: modelApi.catalogInfo });
}

/** 标准模型的变更会影响 Provider 视图里的映射能力与 provider_count，需一并失效。 */
export function useInvalidateModelData() {
  const client = useQueryClient();
  return () => Promise.all([
    client.invalidateQueries({ queryKey: modelKeys.all }),
    client.invalidateQueries({ queryKey: ["providers"] }),
  ]);
}

export function useSaveStandardModel() {
  const invalidate = useInvalidateModelData();
  return useMutation({ mutationFn: (model: StandardModel) => modelApi.save(model), onSuccess: invalidate });
}

export function useDeleteStandardModel() {
  const invalidate = useInvalidateModelData();
  return useMutation({ mutationFn: (id: string) => modelApi.remove(id), onSuccess: invalidate });
}
