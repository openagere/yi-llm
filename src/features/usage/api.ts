import { invoke } from "@/shared/api";
import type { UsageDateRange, UsageSnapshot } from "./types";

declare module "@/shared/api/tauri" {
  interface CommandMap {
    get_usage: { args: { startDate: string; endDate: string; model: string | null }; result: UsageSnapshot };
  }
}

export const usageApi = {
  get: (range: UsageDateRange, model: string) => invoke("get_usage", { startDate: range.startDate, endDate: range.endDate, model: model || null }),
};
