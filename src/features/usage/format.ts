import { currentLanguage } from "@/shared/i18n";
import type { UsageDateRange, UsageModel } from "./types";

const numberFormat = (locale = currentLanguage()) => new Intl.NumberFormat(locale, { maximumFractionDigits: 2 });
const integerFormat = (locale = currentLanguage()) => new Intl.NumberFormat(locale, { maximumFractionDigits: 0 });
const units = [{ value: 1e9, label: "b" }, { value: 1e6, label: "m" }, { value: 1e3, label: "k" }];

export function compactNumber(value: number, locale = currentLanguage()): string {
  let index = units.findIndex((item) => Math.abs(value) >= item.value);
  if (index > 0 && Math.round(Math.abs(value) / units[index].value * 100) / 100 >= 1000) index -= 1;
  const unit = units[index];
  return unit ? `${numberFormat(locale).format(value / unit.value)}${unit.label}` : integerFormat(locale).format(value);
}

export function totalForModel(model: UsageModel): number { return model.input_tokens + model.output_tokens; }
export function percentage(value: number, total: number): string { return `${numberFormat().format(total ? value / total * 100 : 0)}%`; }
export function localDay(date: Date): string { return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`; }
export function shortDay(day: string): string { return `${Number(day.slice(5, 7))}/${Number(day.slice(8, 10))}`; }
export function dateRange(range: UsageDateRange): string {
  const start = range.startDate.replace(/-/g, "/");
  const end = range.endDate.replace(/-/g, "/");
  return range.startDate === range.endDate ? start : `${start} ~ ${end}`;
}
export function rangeDays(range: UsageDateRange): number {
  return Math.round((Date.parse(range.endDate) - Date.parse(range.startDate)) / 86400000) + 1;
}

export function downloadCsv(filename: string, headers: string[], rows: (string | number)[][]) {
  const csv = [headers, ...rows].map((row) => row.map((cell) => {
    const value = String(cell);
    const safe = typeof cell === "string" && /^[=+\-@\t\r]/.test(value) ? `'${value}` : value;
    return `"${safe.replace(/"/g, '""')}"`;
  }).join(",")).join("\r\n");
  const url = URL.createObjectURL(new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
