import type { Translate } from "@/shared/i18n";
import { capabilityErrors, defaultCapabilities, parseTokenCapacity, tokenInput } from "./capabilities";
import type { ModelCapabilities, StandardModel } from "./types";

export type CapacityField = "context_window" | "max_output_tokens";
export type ModalityField = "input_modalities" | "output_modalities";
export type TokenDrafts = Record<CapacityField, string>;

export const CAPACITY_FIELDS: readonly CapacityField[] = ["context_window", "max_output_tokens"];
export const MODALITY_FIELDS: readonly ModalityField[] = ["input_modalities", "output_modalities"];

export const TOKEN_PRESETS: Record<CapacityField, readonly string[]> = {
  context_window: ["32K", "64K", "128K", "256K", "512K", "1M", "2M"],
  max_output_tokens: ["1K", "4K", "8K", "16K", "32K", "64K", "128K"],
};

export function emptyModel(): StandardModel {
  return {
    id: crypto.randomUUID(), name: "", protocol: "responses", brand: "", provider_count: 0,
    capabilities: { ...defaultCapabilities(), input_modalities: ["text"], output_modalities: ["text"] },
  };
}

export function duplicateModel(model: StandardModel, name: string): StandardModel {
  return { ...structuredClone(model), id: crypto.randomUUID(), name, provider_count: 0 };
}

export function tokenDraftsOf(model: StandardModel): TokenDrafts {
  return {
    context_window: tokenInput(model.capabilities.context_window),
    max_output_tokens: tokenInput(model.capabilities.max_output_tokens),
  };
}

export function isModelDirty(draft: StandardModel, tokenDrafts: TokenDrafts, initial: StandardModel): boolean {
  const initialTokens = tokenDraftsOf(initial);
  return JSON.stringify(draft) !== JSON.stringify(initial)
    || tokenDrafts.context_window !== initialTokens.context_window
    || tokenDrafts.max_output_tokens !== initialTokens.max_output_tokens;
}

export function protocolConflictMessage(draft: StandardModel, initial: StandardModel, t: Translate): string {
  return draft.provider_count > 0 && draft.protocol !== initial.protocol
    ? t("model.editor.errors.protocolConflict", { count: draft.provider_count }) : "";
}

export function validateModel(draft: StandardModel, tokenDrafts: TokenDrafts, models: StandardModel[], protocolConflict: string, t: Translate) {
  const context = parseTokenCapacity(tokenDrafts.context_window);
  const output = parseTokenCapacity(tokenDrafts.max_output_tokens);
  const capabilities: ModelCapabilities = { ...draft.capabilities, context_window: context.value, max_output_tokens: output.value };
  const errors = capabilityErrors(capabilities, draft.protocol);
  if (context.error) errors.context_window = context.error;
  if (output.error) errors.max_output_tokens = output.error;
  if (protocolConflict) errors.protocol = protocolConflict;
  const name = draft.name.trim();
  if (!name) errors.name = t("model.editor.errors.nameRequired");
  else if (models.some((item) => item.id !== draft.id && item.name === name && item.protocol === draft.protocol)) errors.name = t("model.editor.errors.nameDuplicate");
  return { capabilities, errors };
}
