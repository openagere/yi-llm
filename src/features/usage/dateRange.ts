import type { MessageKey } from "@/shared/i18n";
import { localDay, rangeDays } from "./format";
import type { UsageDateRange } from "./types";

export type UsageDatePreset = "today" | "yesterday" | "7" | "30" | "90" | "month" | "lastMonth" | "custom";

export const usageDatePresets: { value: UsageDatePreset; labelKey: MessageKey }[] = [
  { value: "today", labelKey: "usage.date.today" },
  { value: "yesterday", labelKey: "usage.date.yesterday" },
  { value: "7", labelKey: "usage.date.days7" },
  { value: "30", labelKey: "usage.date.days30" },
  { value: "90", labelKey: "usage.date.days90" },
  { value: "month", labelKey: "usage.date.month" },
  { value: "lastMonth", labelKey: "usage.date.lastMonth" },
  { value: "custom", labelKey: "usage.date.custom" },
];

export function presetRange(preset: Exclude<UsageDatePreset, "custom">, now = new Date()): UsageDateRange {
  const end = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 12);
  const start = new Date(end);
  if (preset === "yesterday") { start.setDate(start.getDate() - 1); end.setDate(end.getDate() - 1); }
  else if (preset === "month") start.setDate(1);
  else if (preset === "lastMonth") { start.setDate(1); start.setMonth(start.getMonth() - 1); end.setDate(0); }
  else if (preset !== "today") start.setDate(start.getDate() - Number(preset) + 1);
  return { startDate: localDay(start), endDate: localDay(end) };
}

/** 校验自定义日期范围；合法时返回 null，否则返回错误文案的 i18n 键。 */
export function validateRange(range: UsageDateRange): MessageKey | null {
  if (![range.startDate, range.endDate].every((day) => /^\d{4}-\d{2}-\d{2}$/.test(day) && Number.isFinite(Date.parse(day)) && new Date(day).toISOString().slice(0, 10) === day)) return "usage.date.errors.invalid";
  if (range.startDate > range.endDate) return "usage.date.errors.startAfterEnd";
  if (range.endDate > localDay(new Date())) return "usage.date.errors.endAfterToday";
  if (rangeDays(range) > 365) return "usage.date.errors.tooLong";
  return null;
}
