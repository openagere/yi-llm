import type { Translate } from "@/shared/i18n";
import type { ServerSettings } from "./types";

export function formatBytes(value: number): string {
  if (value < 1024) return `${Math.round(value)} B`;
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KiB`;
  if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MiB`;
  return `${(value / 1024 ** 3).toFixed(2)} GiB`;
}

export function formatCount(value: number, language: string): string {
  return value.toLocaleString(language);
}

export function formatClock(timestamp: number, language: string): string {
  return new Date(timestamp).toLocaleTimeString(language, { hour12: false });
}

export function formatDateTime(timestamp: number, language: string): string {
  return new Date(timestamp).toLocaleString(language, { hour12: false });
}

/** `now` 由调用方传入（如最近一次轮询时间），保持渲染纯净。 */
export function formatUptime(startedAt: number | null, now: number, t: Translate): string {
  if (!startedAt) return "--";
  const seconds = Math.max(0, Math.floor((now - startedAt) / 1000));
  if (seconds < 60) return t("proxy.uptime.seconds", { value: seconds });
  const minutes = Math.floor(seconds / 60);
  return minutes < 60 ? t("proxy.uptime.minutes", { m: minutes, s: seconds % 60 }) : t("proxy.uptime.hours", { h: Math.floor(minutes / 60), m: minutes % 60 });
}

export function formatListenAddress(settings: ServerSettings): string {
  return `${settings.host.includes(":") ? `[${settings.host}]` : settings.host}:${settings.port}`;
}
