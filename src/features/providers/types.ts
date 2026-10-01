import type { ModelCapabilities } from "@/features/models";
import type { ProviderType } from "@/shared/lib";

export type { ProviderType };
export type ThinkingLevel = "minimal" | "low" | "medium" | "high";
export interface ProtocolSupport { anthropic: boolean; openai_chat: boolean; responses: boolean }

export interface Provider {
  id: string;
  provider_type: ProviderType;
  name: string;
  short_code: string;
  base_url: string;
  api_key: string;
  enabled: boolean;
  thinking: ThinkingLevel;
  extra: Record<string, unknown>;
  is_default: boolean;
  protocol_support: ProtocolSupport;
}

export interface ModelMapping {
  id: number;
  provider_id: string;
  route_id: string;
  name: string;
  standard_model_id: string | null;
  capabilities?: ModelCapabilities;
  non_standard: boolean;
}

export interface ProviderView extends Provider { models: ModelMapping[] }
export interface ProviderSaveResult { terminal_config_warnings: string[] }
