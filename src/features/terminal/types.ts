import type { ProviderType } from "@/shared/lib";

export type TerminalClient = "codex" | "claude-code" | "opencode" | "pi" | "deepseek-harness";
export interface TerminalSelection {
  provider_id: string;
  model: string;
}
export interface TerminalDirectProfile {
  client: TerminalClient;
  provider_id: string;
  model_source: "native" | "provider";
  model: string | null;
}
export interface TerminalProfile {
  client: TerminalClient;
  models: TerminalSelection[];
  default_model: string;
  protocol: ProviderType | null;
}
export interface TerminalStatus {
  client: TerminalClient;
  config_path: string;
  exists: boolean;
  active: boolean;
  active_mode?: "none" | "proxy" | "direct";
  profile: TerminalProfile | null;
  direct_profile?: TerminalDirectProfile | null;
  config_error: string | null;
}
export interface TerminalPreviewFile {
  path: string;
  content: string;
}
export interface TerminalPreview {
  config_path: string;
  endpoint: string;
  files: TerminalPreviewFile[];
}
export interface TerminalApplyResult {
  config_path: string;
  endpoint: string;
  backup_paths: string[];
  model_count: number;
}
