import type { ProviderType } from "@/shared/lib";

export type Modality = "text" | "image" | "audio" | "video" | "pdf";
export type EffortSupport = "unknown" | "unsupported" | "supported";

export interface ModelCapabilities {
  input_modalities: Modality[] | null;
  output_modalities: Modality[] | null;
  context_window: number | null;
  max_output_tokens: number | null;
  effort: { support: EffortSupport; levels: string[]; default: string | null };
}

export interface StandardModel {
  id: string;
  name: string;
  protocol: ProviderType;
  brand: string;
  capabilities: ModelCapabilities;
  provider_count: number;
}

export interface ModelCatalogInfo {
  path: string;
  version: number;
}
