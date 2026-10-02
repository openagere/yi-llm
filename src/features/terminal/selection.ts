import { supportsTerminal } from "@/features/models";
import type { ProviderType, ProviderView } from "@/features/providers";
import type { TerminalClient, TerminalDirectProfile, TerminalProfile, TerminalSelection } from "./types";

export interface TerminalDefinition {
  id: TerminalClient;
  name: string;
  protocol: ProviderType;
  brand: string;
}

export const TERMINALS = [
  { id: "codex", name: "Codex CLI", protocol: "responses", brand: "openai" },
  { id: "claude-code", name: "Claude Code", protocol: "anthropic", brand: "claude" },
  { id: "opencode", name: "OpenCode", protocol: "openai_chat", brand: "opencode" },
] as const satisfies readonly TerminalDefinition[];

export const EMPTY_PROFILES: Record<TerminalClient, TerminalProfile> = {
  codex: { client: "codex", models: [], default_model: "" },
  "claude-code": { client: "claude-code", models: [], default_model: "" },
  opencode: { client: "opencode", models: [], default_model: "" },
};

export const selectionKey = (selection: TerminalSelection) => `${selection.provider_id}\0${selection.model}`;

export const groupKey = (client: TerminalClient, providerId: string) => `${client}:${providerId}`;

export const providerSelections = (provider: ProviderView): TerminalSelection[] => provider.models.map((model) => ({ provider_id: provider.id, model: model.route_id }));

/** 保留已启用、协议可用且模型能被该终端使用的 Provider。 */
const DIRECT_PROTOCOLS: Record<TerminalClient, readonly ProviderType[]> = {
  codex: ["responses"],
  "claude-code": ["anthropic"],
  opencode: ["openai_chat", "responses"],
};

/** Direct access must match the upstream native protocol, not a translated client protocol. */
export function directCompatibleProviders(providers: readonly ProviderView[], client: TerminalClient): ProviderView[] {
  return providers.filter((provider) => provider.enabled && DIRECT_PROTOCOLS[client].includes(provider.provider_type));
}

export function emptyDirectProfile(client: TerminalClient, providerId = ""): TerminalDirectProfile {
  return { client, provider_id: providerId, model_source: client === "opencode" ? "provider" : "native", model: null };
}

export function compatibleProviders(providers: readonly ProviderView[], protocol: ProviderType): ProviderView[] {
  return providers
    .filter((provider) => provider.enabled && provider.protocol_support[protocol])
    .map((provider) => ({ ...provider, models: provider.models.filter((model) => supportsTerminal(model.capabilities, protocol, provider.provider_type)) }))
    .filter((provider) => provider.models.length);
}

export function filterProviders(providers: readonly ProviderView[], query: string): ProviderView[] {
  return providers
    .map((provider) => ({ ...provider, models: provider.models.filter((model) => `${provider.name} ${model.route_id}`.toLowerCase().includes(query)) }))
    .filter((provider) => provider.models.length);
}

export function availableSelectionKeys(providers: readonly ProviderView[]): Set<string> {
  return new Set(providers.flatMap((provider) => providerSelections(provider).map(selectionKey)));
}

/** 生成新草稿；默认模型不在集合中时回落到第一个模型。 */
export function buildProfile(client: TerminalClient, models: TerminalSelection[], defaultModel: string): TerminalProfile {
  return { client, models, default_model: models.some((model) => model.model === defaultModel) ? defaultModel : models[0]?.model ?? "" };
}

/** 固定尚未记录的 Provider 分组展开状态，避免选择变化时分组自行展开或收起。 */
export function pinExpansion(current: Record<string, boolean>, client: TerminalClient, providers: readonly ProviderView[], profile: TerminalProfile): Record<string, boolean> {
  const next = { ...current };
  for (const provider of providers) {
    const id = groupKey(client, provider.id);
    if (!(id in next)) next[id] = profile.models.some((model) => model.provider_id === provider.id);
  }
  return next;
}
