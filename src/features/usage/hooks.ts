import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { useStandardModels, type StandardModel } from "@/features/models";
import { useProviders, type ProviderView } from "@/features/providers";
import { usageApi } from "./api";
import { createUsageBrandResolver, createUsageModelFormatter } from "./modelNames";
import type { UsageDateRange } from "./types";

export const usageKeys = {
  all: ["usage"] as const,
  snapshot: (range: UsageDateRange, model: string) => ["usage", range.startDate, range.endDate, model] as const,
};

/** 读取某个日期范围（可选按模型过滤）的用量快照；切换范围或模型时保留上一份数据用于占位。 */
export function useUsage(range: UsageDateRange, model: string) {
  return useQuery({
    queryKey: usageKeys.snapshot(range, model),
    queryFn: () => usageApi.get(range, model),
    placeholderData: keepPreviousData,
    staleTime: 0,
    refetchOnWindowFocus: false,
    meta: { errorPrefix: "usage.page.loadFailedPrefix" },
  });
}

const NO_PROVIDERS: ProviderView[] = [];
const NO_STANDARD_MODELS: StandardModel[] = [];

/** 路由 ID 的展示名与品牌图标解析器，依赖 Provider 与标准模型数据。 */
export function useUsageModelNames() {
  const providers = useProviders().data ?? NO_PROVIDERS;
  const standardModels = useStandardModels().data ?? NO_STANDARD_MODELS;
  const formatModel = useMemo(() => createUsageModelFormatter(providers), [providers]);
  const resolveBrand = useMemo(() => createUsageBrandResolver(providers, standardModels), [providers, standardModels]);
  return { formatModel, resolveBrand };
}
