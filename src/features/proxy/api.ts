import { invoke } from "@/shared/api";
import type { ProxyAction, ProxyMonitorSnapshot, ProxyRuntime, ServerSettings } from "./types";

declare module "@/shared/api/tauri" {
  interface CommandMap {
    get_settings: { args: void; result: ServerSettings };
    save_settings: { args: { host: string; port: number; logLevel: string }; result: void };
    get_proxy_status: { args: void; result: boolean };
    get_proxy_runtime: { args: void; result: ProxyRuntime };
    get_proxy_monitor: { args: { minutes: number }; result: ProxyMonitorSnapshot };
    control_proxy: { args: { action: ProxyAction }; result: void };
    get_logs: { args: void; result: string };
  }
}

export const proxyApi = {
  settings: () => invoke("get_settings"),
  saveSettings: (settings: ServerSettings) => invoke("save_settings", { host: settings.host, port: settings.port, logLevel: settings.log_level }),
  status: () => invoke("get_proxy_status"),
  runtime: () => invoke("get_proxy_runtime"),
  monitor: (minutes: number) => invoke("get_proxy_monitor", { minutes }),
  control: (action: ProxyAction) => invoke("control_proxy", { action }),
  logs: () => invoke("get_logs"),
};
