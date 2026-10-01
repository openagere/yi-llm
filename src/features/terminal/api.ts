import { invoke } from "@/shared/api";
import type { TerminalApplyResult, TerminalPreview, TerminalProfile, TerminalStatus } from "./types";

declare module "@/shared/api/tauri" {
  interface CommandMap {
    get_terminal_profiles: { args: void; result: TerminalStatus[] };
    preview_terminal_config: { args: { profile: TerminalProfile }; result: TerminalPreview };
    apply_terminal_config: { args: { profile: TerminalProfile }; result: TerminalApplyResult };
  }
}

export const terminalApi = {
  profiles: () => invoke("get_terminal_profiles"),
  preview: (profile: TerminalProfile) => invoke("preview_terminal_config", { profile }),
  apply: (profile: TerminalProfile) => invoke("apply_terminal_config", { profile }),
};
