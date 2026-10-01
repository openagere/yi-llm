import { useMemo, useState } from "react";
import { Activity, ArrowDown, ArrowRight, ArrowUp, ArrowUpDown, Boxes, Cpu, Download, Search, Sigma, X, type LucideIcon } from "lucide-react";
import { describeError } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { notify } from "@/shared/notify";
import { Select } from "@/shared/ui";
import { downloadCsv, localDay, percentage, totalForModel } from "../format";
import { usageTokenMetrics } from "../metrics";
import type { UsageModelBrandResolver, UsageModelFormatter } from "../modelNames";
import type { UsageSnapshot } from "../types";
import { UsageEmpty, UsageMetricCaption, UsageModelLink, UsageNumber, UsagePagination, UsageSortHeading } from "./UsageCommon";

type SortKey = "model" | "requests" | "total" | typeof usageTokenMetrics[number]["key"];
const PAGE_SIZE = 15;

export function UsageModels({ snapshot, onModel, formatModel, resolveBrand }: { snapshot: UsageSnapshot; onModel: (model: string) => void; formatModel: UsageModelFormatter; resolveBrand: UsageModelBrandResolver }) {
  const { t } = useI18n();
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<{ key: SortKey; direction: "asc" | "desc" }>({ key: "total", direction: "desc" });
  const [page, setPage] = useState(1);
  const total = snapshot.input_tokens + snapshot.output_tokens;
  const rows = useMemo(() => snapshot.models.filter((model) => `${formatModel(model.model)} ${model.model}`.toLowerCase().includes(search.trim().toLowerCase())).map((model) => ({
    ...model, displayName: formatModel(model.model), total: totalForModel(model), average: model.requests ? Math.round(totalForModel(model) / model.requests) : 0,
  })).sort((a, b) => {
    const comparison = sort.key === "model" ? a.displayName.localeCompare(b.displayName) : Number(a[sort.key]) - Number(b[sort.key]);
    return comparison * (sort.direction === "asc" ? 1 : -1) || a.model.localeCompare(b.model);
  }), [snapshot.models, search, sort, formatModel]);
  const currentPage = Math.min(page, Math.max(1, Math.ceil(rows.length / PAGE_SIZE)));
  const sortOptions: { key: SortKey; label: string; icon: LucideIcon }[] = [
    { key: "total", label: t("usage.common.totalTokens"), icon: Sigma },
    { key: "requests", label: t("usage.common.requestCount"), icon: Activity },
    ...usageTokenMetrics.map(({ key, tokensKey, icon }) => ({ key, label: t(tokensKey), icon })),
    { key: "model", label: t("usage.models.sortByName"), icon: Cpu },
  ];
  function changeSort(key: SortKey) {
    setPage(1);
    setSort((current) => ({ key, direction: current.key === key ? current.direction === "desc" ? "asc" : "desc" : key === "model" ? "asc" : "desc" }));
  }
  function exportModels() {
    try {
      downloadCsv(`yi-llm-models-${localDay(new Date())}.csv`, [t("usage.common.model"), t("usage.common.routeId"), t("usage.common.requestCount"), ...usageTokenMetrics.map((metric) => t(metric.tokensKey)), t("usage.common.totalTokens"), t("usage.common.usageShare"), t("usage.common.averageTokens")],
        rows.map((row) => [row.displayName, row.model, row.requests, ...usageTokenMetrics.map((metric) => row[metric.key]), row.total, percentage(row.total, total), row.average]));
      notify(t("usage.models.exported"));
    } catch (error) { notify(`${t("usage.common.exportFailedPrefix")}${describeError(error)}`, "error"); }
  }
  const heading = (key: SortKey, label: string, icon: LucideIcon, className?: string) => <UsageSortHeading key={key} className={className} icon={icon} label={label} selected={sort.key === key} direction={sort.direction} onSort={() => changeSort(key)} />;

  return <section className="analytics-models-section">
    <div className="analytics-section-heading"><div><h2><Boxes size={16} aria-hidden="true" />{t("usage.models.title")}</h2><span>{t("usage.models.count", { count: rows.length })}</span></div><div className="analytics-section-actions"><div className="analytics-search"><Search size={16} /><input type="search" value={search} onChange={(event) => { setSearch(event.target.value); setPage(1); }} placeholder={t("usage.models.search")} aria-label={t("usage.models.searchLabel")} />{search && <button type="button" title={t("usage.models.clearSearch")} aria-label={t("usage.models.clearSearch")} onClick={() => { setSearch(""); setPage(1); }}><X size={14} /></button>}</div><button type="button" className="icon-button" title={t("usage.models.export")} aria-label={t("usage.models.export")} disabled={!rows.length} onClick={exportModels}><Download size={16} /></button></div></div>
    <div className="analytics-mobile-sort"><Select className="analytics-sort-select" icon={ArrowUpDown} label={t("usage.models.sortLabel")} value={sort.key} onValueChange={(value) => { setPage(1); setSort({ key: value as SortKey, direction: value === "model" ? "asc" : "desc" }); }} options={sortOptions.map(({ key, ...option }) => ({ ...option, value: key }))} /><button type="button" className="icon-button" aria-label={sort.direction === "desc" ? t("usage.models.sortAscend") : t("usage.models.sortDescend")} title={sort.direction === "desc" ? t("usage.models.currentDescending") : t("usage.models.currentAscending")} onClick={() => setSort((current) => ({ ...current, direction: current.direction === "asc" ? "desc" : "asc" }))}>{sort.direction === "desc" ? <ArrowDown size={16} /> : <ArrowUp size={16} />}</button></div>
    {rows.length ? <><div className="analytics-table-scroll" tabIndex={0} aria-label={t("usage.models.table")}><table className="analytics-table analytics-models-table"><colgroup><col className="model-column" /><col className="requests-column" />{usageTokenMetrics.map(({ key }) => <col key={key} />)}<col className="total-column" /><col className="action-column" /></colgroup><thead><tr>{heading("model", t("usage.common.model"), Cpu)}{heading("requests", t("usage.models.requestsColumn"), Activity)}{usageTokenMetrics.map(({ key, labelKey, icon, className }) => heading(key, t(labelKey), icon, className))}{heading("total", t("usage.common.totalTokens"), Sigma)}<th aria-label={t("usage.tabs.records")} /></tr></thead><tbody>
      {rows.slice((currentPage - 1) * PAGE_SIZE, currentPage * PAGE_SIZE).map((row) => <tr key={row.model}><td><UsageModelLink model={row.displayName} brand={resolveBrand(row.model)} title={t("usage.models.viewRecords", { name: row.displayName })} onClick={() => onModel(row.model)} /></td><td data-label={t("usage.models.requestsColumn")}><UsageNumber value={row.requests} /></td>{usageTokenMetrics.map(({ key, labelKey, icon, className }) => <td key={key} data-label={t(labelKey)} className={`analytics-token-cell ${className}`}><UsageMetricCaption label={t(labelKey)} icon={icon} /><UsageNumber value={row[key]} /></td>)}<td data-label={t("usage.common.totalTokens")} className="analytics-total-cell"><UsageNumber value={row.total} /></td><td><button type="button" className="icon-button analytics-row-action" title={t("usage.models.viewRecords", { name: row.displayName })} aria-label={t("usage.models.viewRecords", { name: row.displayName })} onClick={() => onModel(row.model)}><ArrowRight size={16} /></button></td></tr>)}
    </tbody></table></div><UsagePagination page={currentPage} count={rows.length} pageSize={PAGE_SIZE} onPage={setPage} /></> : <UsageEmpty title={search ? t("usage.models.noMatch") : t("usage.models.empty")} action={search ? () => { setSearch(""); setPage(1); } : undefined} />}
  </section>;
}
