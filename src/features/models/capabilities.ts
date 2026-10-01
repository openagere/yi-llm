import { translate } from "@/shared/i18n";
import type { ProviderType } from "@/shared/lib";
import type { Modality, ModelCapabilities } from "./types";

/** 模态标识列表；展示名使用 `modalityLabel`。 */
export const MODALITIES: readonly Modality[] = ["text", "image", "audio", "video", "pdf"];

export const modalityLabel = (modality: Modality) => translate(`model.modality.${modality}`);

export function defaultCapabilities(): ModelCapabilities {
  return { input_modalities: null, output_modalities: null, context_window: null, max_output_tokens: null,
    effort: { support: "unknown", levels: [], default: null } };
}

export const effortLevels = (protocol: ProviderType) => protocol === "anthropic"
  ? ["low", "medium", "high", "xhigh", "max", "ultracode"] : ["low", "medium", "high", "xhigh", "max", "ultra"];

export function effectiveModalities(values: Modality[] | null, client: ProviderType, upstream: ProviderType, output = false): Modality[] {
  return (values ?? ["text"]).filter((value) => client === upstream || value === "text" || (!output && value === "image"));
}

export function supportsTerminal(caps: ModelCapabilities | undefined, client: ProviderType, upstream: ProviderType) {
  const value = caps ?? defaultCapabilities();
  return effectiveModalities(value.input_modalities, client, upstream).includes("text")
    && effectiveModalities(value.output_modalities, client, upstream, true).includes("text");
}

export function effectiveEfforts(caps: ModelCapabilities, client: ProviderType, upstream: ProviderType) {
  return caps.effort.support === "supported" ? caps.effort.levels.filter((level) =>
    effortLevels(client).includes(level) && effortLevels(upstream).includes(level)) : [];
}

export function capabilityErrors(caps: ModelCapabilities, protocol: ProviderType) {
  const errors: Record<string, string> = {};
  for (const field of ["input_modalities", "output_modalities"] as const) {
    if (caps[field] && !caps[field].length) errors[field] = translate("model.capability.modalityRequired");
  }
  for (const field of ["context_window", "max_output_tokens"] as const) {
    const value = caps[field];
    if (value !== null && (!Number.isInteger(value) || value < 1 || value > 100_000_000)) errors[field] = translate("model.capability.tokenRange");
  }
  if (caps.context_window !== null && caps.max_output_tokens !== null && caps.max_output_tokens > caps.context_window)
    errors.max_output_tokens = translate("model.capability.outputExceedsContext");
  if (caps.effort.support === "supported") {
    if (!caps.effort.levels.length || caps.effort.levels.some((level) => !effortLevels(protocol).includes(level))) errors.effort = translate("model.capability.effortInvalid");
    if (caps.effort.default && !caps.effort.levels.includes(caps.effort.default)) errors.effort = translate("model.capability.effortDefaultMissing");
  }
  return errors;
}

const TOKEN_K = 1024;
const TOKEN_M = TOKEN_K * TOKEN_K;

export function parseTokenCapacity(text: string): { value: number | null; error?: string } {
  const input = text.trim();
  if (!input) return { value: null };
  const match = /^(\d+(?:\.\d+)?)\s*([km])?$/i.exec(input);
  if (!match) return { value: null, error: translate("model.capability.tokenParse") };
  const unit = match[2]?.toUpperCase();
  const value = Number(match[1]) * (unit === "M" ? TOKEN_M : unit === "K" ? TOKEN_K : 1);
  if (!Number.isSafeInteger(value) || value < 1 || value > 100_000_000)
    return { value: null, error: translate("model.capability.tokenConverted") };
  return { value };
}

export function tokenInput(value: number | null): string {
  if (value === null) return "";
  if (value >= TOKEN_M && Number.isInteger(value / TOKEN_M * 100)) return `${value / TOKEN_M}M`;
  if (value % TOKEN_K === 0) return `${value / TOKEN_K}K`;
  return String(value);
}

export const tokenLabel = (value: number) => /[KM]$/.test(tokenInput(value)) ? tokenInput(value) : value.toLocaleString("en-US");
