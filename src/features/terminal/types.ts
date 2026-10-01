export type TerminalClient = "codex" | "claude-code" | "opencode";
export interface TerminalSelection { provider_id: string; model: string }
export interface TerminalProfile { client: TerminalClient; models: TerminalSelection[]; default_model: string }
export interface TerminalStatus { client: TerminalClient; config_path: string; exists: boolean; active: boolean; profile: TerminalProfile | null; config_error: string | null }
export interface TerminalPreviewFile { path: string; content: string }
export interface TerminalPreview { config_path: string; endpoint: string; files: TerminalPreviewFile[] }
export interface TerminalApplyResult { config_path: string; endpoint: string; backup_paths: string[]; model_count: number }
