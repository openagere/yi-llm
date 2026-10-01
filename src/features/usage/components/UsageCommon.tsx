import { ArrowDown, ArrowUp, ArrowUpDown, ChartNoAxesColumnIncreasing, ChevronLeft, ChevronRight, type LucideIcon } from "lucide-react";
import { useI18n } from "@/shared/i18n";
import { ProviderIcon, Select } from "@/shared/ui";
import { compactNumber } from "../format";
import { usageTokenMetrics } from "../metrics";
import type { UsageSnapshot } from "../types";

export function UsageNumber({ value, className = "" }: { value: number; className?: string }) {
  const { language } = useI18n();
  const formatted = compactNumber(value, language);
  return <span className={`usage-number ${className}`} title={formatted}>{formatted}</span>;
}

export function UsageTokenMetrics({ values }: { values: Pick<UsageSnapshot, "input_tokens" | "output_tokens" | "cached_tokens" | "reasoning_tokens"> }) {
  const { t } = useI18n();
  return <dl className="analytics-token-metrics">{usageTokenMetrics.map(({ key, tokensKey, icon: Icon, className }) => <div key={key} className={className}><dt><Icon size={14} />{t(tokensKey)}</dt><dd><UsageNumber value={values[key]} /></dd></div>)}</dl>;
}

export function UsageModelLink({ model, brand, onClick, title, ariaLabel }: { model: string; brand?: string; onClick: () => void; title: string; ariaLabel?: string }) {
  return <div className="analytics-model-identity"><ProviderIcon brand={brand} size={18} /><button type="button" className="analytics-model-link" title={title} aria-label={ariaLabel} onClick={onClick}>{model}</button></div>;
}

export function UsageMetricCaption({ label, icon: Icon }: { label: string; icon: LucideIcon }) {
  return <span className="analytics-token-caption"><Icon size={14} aria-hidden="true" />{label}</span>;
}

export function UsageEmpty({ title, action, actionLabel }: { title?: string; action?: () => void; actionLabel?: string }) {
  const { t } = useI18n();
  return <div className="analytics-empty"><ChartNoAxesColumnIncreasing size={30} strokeWidth={1.5} /><h3>{title ?? t("usage.common.emptyRange")}</h3>{action && <button type="button" className="text-button" onClick={action}>{actionLabel ?? t("usage.common.clearFilters")}</button>}</div>;
}

const PAGE_SIZES = [10, 20, 50];

export function UsagePagination({ page, count, pageSize, onPage, onPageSize }: { page: number; count: number; pageSize: number; onPage: (page: number) => void; onPageSize?: (size: number) => void }) {
  const { t } = useI18n();
  const pages = Math.max(1, Math.ceil(count / pageSize));
  const current = Math.min(page, pages);
  const summary = count ? t("usage.common.pagination.range", { from: (current - 1) * pageSize + 1, to: Math.min(current * pageSize, count), total: count }) : t("usage.common.pagination.zero");
  return <div className="analytics-pagination"><span>{summary}</span><div>{onPageSize && <Select className="analytics-page-size" label={t("usage.common.pagination.pageSizeLabel")} value={String(pageSize)} onValueChange={(value) => onPageSize(Number(value))} options={PAGE_SIZES.map((size) => ({ value: String(size), label: t("usage.common.pagination.perPage", { size }) }))} />}<button type="button" className="icon-button" title={t("usage.common.pagination.previous")} aria-label={t("usage.common.pagination.previous")} disabled={current <= 1} onClick={() => onPage(current - 1)}><ChevronLeft size={16} /></button><span className="analytics-page-index">{current} / {pages}</span><button type="button" className="icon-button" title={t("usage.common.pagination.next")} aria-label={t("usage.common.pagination.next")} disabled={current >= pages} onClick={() => onPage(current + 1)}><ChevronRight size={16} /></button></div></div>;
}

export function UsageSortHeading({ label, icon: Icon, selected, direction, onSort, className }: { label: string; icon: LucideIcon; selected: boolean; direction: "asc" | "desc"; onSort: () => void; className?: string }) {
  const SortIcon = selected ? direction === "asc" ? ArrowUp : ArrowDown : ArrowUpDown;
  return <th className={className} aria-sort={selected ? direction === "asc" ? "ascending" : "descending" : "none"}><button type="button" className={`analytics-sort ${selected ? "active" : ""}`} onClick={onSort}><Icon className="analytics-heading-icon" size={14} aria-hidden="true" /><span>{label}</span><SortIcon className="analytics-sort-direction" size={12} aria-hidden="true" /></button></th>;
}
