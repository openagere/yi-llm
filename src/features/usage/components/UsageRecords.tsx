import { useMemo, useState } from "react";
import { Activity, Clock, Cpu, Download, Plug, Search, SlidersHorizontal, X } from "lucide-react";
import { describeError } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { notify } from "@/shared/notify";
import { Select } from "@/shared/ui";
import { downloadCsv, localDay } from "../format";
import { usageProtocolNames, usageTokenMetrics } from "../metrics";
import type { UsageModelBrandResolver, UsageModelFormatter } from "../modelNames";
import type { UsageRecord, UsageSnapshot } from "../types";
import { UsageEmpty, UsageMetricCaption, UsageModelLink, UsageNumber, UsagePagination } from "./UsageCommon";
import { UsageRecordDetail } from "./UsageRecordDetail";

export function UsageRecords({ snapshot, scope, formatModel, resolveBrand }: { snapshot: UsageSnapshot; scope: string; formatModel: UsageModelFormatter; resolveBrand: UsageModelBrandResolver }) {
  const { t, language } = useI18n();
  const [filters, setFilters] = useState({ scope, search: "", provider: "", protocol: "" });
  const search = filters.scope === scope ? filters.search : "";
  const provider = filters.scope === scope ? filters.provider : "";
  const protocol = filters.scope === scope ? filters.protocol : "";
  const [pageState, setPageState] = useState({ scope, page: 1, pageSize: 10 });
  const page = pageState.scope === scope ? pageState.page : 1;
  const pageSize = pageState.scope === scope ? pageState.pageSize : 10;
  const [selected, setSelected] = useState<UsageRecord | null>(null);
  const detail = selected && filters.scope === scope ? selected : null;
  const providers = useMemo(() => [...new Set(snapshot.recent.map((record) => record.provider_name))].sort(), [snapshot.recent]);
  const protocols = useMemo(() => [...new Set(snapshot.recent.map((record) => record.protocol))].sort(), [snapshot.recent]);
  const filtered = useMemo(() => snapshot.recent.filter((record) => (!provider || record.provider_name === provider) && (!protocol || record.protocol === protocol) &&
    `${formatModel(record.model, record.provider_name)} ${record.model} ${record.provider_name} ${usageProtocolNames[record.protocol] ?? record.protocol}`.toLowerCase().includes(search.trim().toLowerCase()))
    .sort((a, b) => b.requested_at - a.requested_at || b.id - a.id), [snapshot.recent, provider, protocol, search, formatModel]);
  const currentPage = Math.min(page, Math.max(1, Math.ceil(filtered.length / pageSize)));
  const hasFilters = Boolean(search || provider || protocol);
  function patch(next: Partial<typeof filters>) { setFilters({ scope, search, provider, protocol, ...next }); setPageState((current) => ({ ...current, scope, page: 1 })); }
  function clearFilters() { patch({ search: "", provider: "", protocol: "" }); }
  function exportRecords() {
    try {
      downloadCsv(`yi-llm-calls-${localDay(new Date())}.csv`, [t("usage.records.csvTime"), t("usage.common.model"), t("usage.common.routeId"), "Provider", t("usage.records.csvProtocol"), ...usageTokenMetrics.map((metric) => t(metric.tokensKey))],
        filtered.map((record) => [new Date(record.requested_at * 1000).toISOString(), formatModel(record.model, record.provider_name), record.model, record.provider_name, record.protocol, ...usageTokenMetrics.map((metric) => record[metric.key])]));
      notify(t("usage.records.exported"));
    } catch (error) { notify(`${t("usage.common.exportFailedPrefix")}${describeError(error)}`, "error"); }
  }

  return <section className="analytics-records-section">
    <div className="analytics-section-heading"><div><h2><Activity size={16} aria-hidden="true" />{t("usage.tabs.records")}</h2><span>{t("usage.records.summary", { count: snapshot.recent.length })}</span></div><div className="analytics-section-actions"><button type="button" className="icon-button" title={t("usage.records.export")} aria-label={t("usage.records.export")} disabled={!filtered.length} onClick={exportRecords}><Download size={16} /></button></div></div>
    <div className="analytics-record-filters"><div className="analytics-search"><Search size={16} /><input type="search" aria-label={t("usage.records.searchLabel")} placeholder={t("usage.records.searchPlaceholder")} value={search} onChange={(event) => patch({ search: event.target.value })} />{search && <button type="button" title={t("usage.records.clearSearch")} aria-label={t("usage.records.clearSearch")} onClick={() => patch({ search: "" })}><X size={14} /></button>}</div><Select className="analytics-provider-select" icon={Plug} label={t("usage.records.filterProvider")} value={provider} onValueChange={(value) => patch({ provider: value })} options={[{ value: "", label: t("usage.records.allProviders"), icon: Plug }, ...[...new Set([...providers, ...(provider ? [provider] : [])])].map((name) => ({ value: name, label: name, icon: Plug }))]} /><Select className="analytics-protocol-select" icon={SlidersHorizontal} label={t("usage.records.filterProtocol")} value={protocol} onValueChange={(value) => patch({ protocol: value })} options={[{ value: "", label: t("usage.records.allProtocols"), icon: SlidersHorizontal }, ...[...new Set([...protocols, ...(protocol ? [protocol] : [])])].map((name) => ({ value: name, label: usageProtocolNames[name] ?? name, icon: SlidersHorizontal }))]} />{hasFilters && <button type="button" className="icon-button" title={t("usage.records.clearFilters")} aria-label={t("usage.records.clearFilters")} onClick={clearFilters}><X size={16} /></button>}</div>
    {filtered.length ? <><div className="analytics-table-scroll" tabIndex={0} aria-label={t("usage.records.table")}><table className="analytics-table analytics-records-table"><colgroup><col className="time-column" /><col className="model-column" /><col className="protocol-column" />{usageTokenMetrics.map(({ key }) => <col key={key} />)}</colgroup><thead><tr><th><span className="analytics-column-label"><Clock size={14} aria-hidden="true" />{t("usage.records.timeColumn")}</span></th><th><span className="analytics-column-label"><Cpu size={14} aria-hidden="true" />{t("usage.common.model")}</span></th><th><span className="analytics-column-label"><SlidersHorizontal size={14} aria-hidden="true" />{t("usage.records.clientProtocolColumn")}</span></th>{usageTokenMetrics.map(({ key, labelKey, icon: Icon, className }) => <th key={key} className={className}><span className="analytics-column-label"><Icon size={14} aria-hidden="true" />{t(labelKey)}</span></th>)}</tr></thead><tbody>
      {filtered.slice((currentPage - 1) * pageSize, currentPage * pageSize).map((record) => {
        const requestedAt = new Date(record.requested_at * 1000);
        const date = requestedAt.toLocaleDateString(language, { year: "numeric", month: "2-digit", day: "2-digit" });
        const time = requestedAt.toLocaleTimeString(language, { hour12: false });
        const displayName = formatModel(record.model, record.provider_name);
        return <tr key={record.id}>
          <td><time className="analytics-record-timestamp" dateTime={requestedAt.toISOString()} title={`${date} ${time}`}>{date} {time}</time></td>
          <td><UsageModelLink model={displayName} brand={resolveBrand(record.model)} title={t("usage.records.viewDetail", { name: displayName })} ariaLabel={t("usage.records.viewDetailWithId", { name: displayName, id: record.id })} onClick={() => setSelected(record)} /></td>
          <td><span className={`analytics-protocol-label protocol-${record.protocol}`}>{usageProtocolNames[record.protocol] ?? record.protocol}</span></td>
          {usageTokenMetrics.map(({ key, labelKey, icon, className }) => <td key={key} data-label={t(labelKey)} className={`analytics-token-cell ${className}`}><UsageMetricCaption label={t(labelKey)} icon={icon} /><UsageNumber value={record[key]} /></td>)}
        </tr>;
      })}
    </tbody></table></div><UsagePagination page={currentPage} count={filtered.length} pageSize={pageSize} onPage={(next) => setPageState({ scope, page: next, pageSize })} onPageSize={(size) => setPageState({ scope, page: 1, pageSize: size })} /></> : <UsageEmpty title={hasFilters ? t("usage.records.noMatch") : t("usage.common.emptyRange")} action={hasFilters ? clearFilters : undefined} />}
    {detail && <UsageRecordDetail record={detail} onClose={() => setSelected(null)} formatModel={formatModel} resolveBrand={resolveBrand} />}
  </section>;
}
