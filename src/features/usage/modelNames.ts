import type { StandardModel } from "@/features/models";
import type { ProviderView } from "@/features/providers";
import { modelBrand } from "@/shared/ui";

export type UsageModelFormatter = (routeId: string, historicalProvider?: string) => string;
export type UsageModelBrandResolver = (routeId: string) => string | undefined;

const GENERATED_ROUTE_PREFIX = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function createUsageModelFormatter(providers: readonly ProviderView[]): UsageModelFormatter {
  const codes = new Map(providers.map((provider) => [provider.id, provider.short_code]));
  return (routeId) => {
    const separator = routeId.indexOf("/");
    if (separator <= 0 || separator === routeId.length - 1) return routeId;
    const prefix = routeId.slice(0, separator);
    const code = codes.get(prefix);
    // 只剥离已知的 Provider ID，或生成路由使用的 UUID 前缀。
    if (!code && !GENERATED_ROUTE_PREFIX.test(prefix)) return routeId;
    const model = routeId.slice(separator + 1);
    return code ? `${model}(${code})` : model;
  };
}

export function createUsageBrandResolver(providers: readonly ProviderView[], standardModels: readonly StandardModel[]): UsageModelBrandResolver {
  const standards = new Map(standardModels.map((standard) => [standard.id, standard]));
  const brands = new Map(providers.flatMap((provider) => provider.models.map((binding) => {
    const standard = binding.standard_model_id ? standards.get(binding.standard_model_id) : undefined;
    return [binding.route_id, standard?.brand || (standard ? modelBrand(standard.name) : undefined) || modelBrand(binding.name)] as const;
  })));
  return (routeId) => brands.get(routeId) || modelBrand(routeId);
}
