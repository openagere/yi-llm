import { useRef, useState } from "react";
import { Activity, Boxes, ChartNoAxesColumnIncreasing, LoaderCircle, RefreshCw, X } from "lucide-react";
import "@/styles/usage.css";
import { useI18n } from "@/shared/i18n";
import { useNavigation } from "@/shared/navigation";
import { notify } from "@/shared/notify";
import { PageHeader, ProviderIcon, Select } from "@/shared/ui";
import { presetRange, type UsageDatePreset } from "../dateRange";
import { dateRange } from "../format";
import { useUsage, useUsageModelNames } from "../hooks";
import type { TrendMode, UsageDateRange } from "../types";
import { UsageEmpty } from "./UsageCommon";
import { UsageDateFilter } from "./UsageDateFilter";
import { UsageModels } from "./UsageModels";
import { UsageOverview } from "./UsageOverview";
import { UsageRecords } from "./UsageRecords";

type UsageTab = "overview" | "models" | "records";
const TABS = [
  { id: "overview", label: "usage.tabs.overview", icon: ChartNoAxesColumnIncreasing },
  { id: "models", label: "usage.tabs.models", icon: Boxes },
  { id: "records", label: "usage.tabs.records", icon: Activity },
] as const;

export function UsagePage() {
  const { t, language } = useI18n();
  const { back, close } = useNavigation();
  const { formatModel, resolveBrand } = useUsageModelNames();
  const [tab, setTab] = useState<UsageTab>("overview");
  const [trendMode, setTrendMode] = useState<TrendMode>("tokens");
  const [dateSelection, setDateSelection] = useState(() => ({ range: presetRange("7"), preset: "7" as UsageDatePreset }));
  const [model, setModel] = useState("");
  const tabList = useRef<HTMLDivElement>(null);
  const workspace = useRef<HTMLElement>(null);
  const { range, preset } = dateSelection;
  const scope = `${range.startDate}:${range.endDate}:${model}`;
  const catalog = useUsage(range, "");
  const selected = useUsage(range, model);
  const activeQuery = model ? selected : catalog;
  const snapshot = activeQuery.isPlaceholderData ? undefined : activeQuery.data;
  const waiting = activeQuery.isFetching && !snapshot;
  const failed = activeQuery.isError && !snapshot;
  const stale = activeQuery.isError && Boolean(snapshot);
  const modelChoices = [...new Set([...(catalog.data?.models.map((item) => item.model) ?? []), ...(model ? [model] : [])])].sort((a, b) => formatModel(a).localeCompare(formatModel(b)) || a.localeCompare(b));

  function refresh() {
    if (preset !== "custom") setDateSelection({ range: presetRange(preset), preset });
    void activeQuery.refetch().then((result) => { if (!result.isError) notify(t("usage.page.refreshed")); });
  }
  function chooseTab(nextTab: UsageTab) {
    setTab(nextTab);
    workspace.current?.scrollIntoView({ block: "start" });
  }
  function moveTab(key: string) {
    const index = TABS.findIndex((item) => item.id === tab);
    const next = key === "Home" ? 0 : key === "End" ? TABS.length - 1 : (index + (key === "ArrowRight" ? 1 : -1) + TABS.length) % TABS.length;
    chooseTab(TABS[next].id);
    tabList.current?.querySelector<HTMLButtonElement>(`#usage-tab-${TABS[next].id}`)?.focus();
  }
  function changeRange(nextRange: UsageDateRange, nextPreset: UsageDatePreset) {
    setDateSelection({ range: nextRange, preset: nextPreset });
  }

  return <section className="usage-page analytics-workspace" ref={workspace}>
    <PageHeader navigationInToolbar eyebrow={t("usage.page.eyebrow")} title={t("usage.page.title")} subtitle={t("pageDescriptions.usage")} onBack={back} onClose={close} closeLabel={t("usage.page.close")} actions={<button type="button" className="icon-button" title={t("usage.page.refresh")} aria-label={t("usage.page.refresh")} disabled={waiting} onClick={refresh}><RefreshCw size={17} className={waiting ? "spinning" : ""} /></button>} />
    <div className="analytics-tabs" role="tablist" aria-label={t("usage.tabs.label")} ref={tabList} onKeyDown={(event) => { if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) { event.preventDefault(); moveTab(event.key); } }}>
      {TABS.map(({ id, label, icon: Icon }) => <button type="button" role="tab" key={id} id={`usage-tab-${id}`} aria-controls={`usage-panel-${id}`} aria-selected={tab === id} tabIndex={tab === id ? 0 : -1} onClick={() => chooseTab(id)}><Icon size={15} />{t(label)}</button>)}
    </div>
    <div className="analytics-global-filters">
      <UsageDateFilter range={range} preset={preset} onChange={changeRange} />
      <Select className="analytics-model-select" icon={Boxes} label={t("usage.filter.modelSelect")} value={model} onValueChange={setModel} disabled={waiting && !catalog.data} options={[{ value: "", label: t("usage.filter.allModels"), icon: Boxes }, ...modelChoices.map((name) => ({ value: name, label: formatModel(name), leading: <ProviderIcon brand={resolveBrand(name)} size={16} /> }))]} />
      {model && <button type="button" className="icon-button analytics-clear-model" title={t("usage.filter.clearModel")} aria-label={t("usage.filter.clearModel")} onClick={() => setModel("")}><X size={14} /></button>}
    </div>
    <div className={`analytics-content ${activeQuery.isFetching && snapshot ? "is-refreshing" : ""}`} aria-busy={waiting}>
      {snapshot ? <>
        <section role="tabpanel" id="usage-panel-overview" aria-labelledby="usage-tab-overview" hidden={tab !== "overview"} tabIndex={0}><UsageOverview snapshot={snapshot} range={range} active={tab === "overview"} mode={trendMode} onMode={setTrendMode} formatModel={formatModel} resolveBrand={resolveBrand} /></section>
        <section role="tabpanel" id="usage-panel-models" aria-labelledby="usage-tab-models" hidden={tab !== "models"} tabIndex={0}><UsageModels snapshot={snapshot} onModel={(name) => { setModel(name); chooseTab("records"); }} formatModel={formatModel} resolveBrand={resolveBrand} /></section>
        <section role="tabpanel" id="usage-panel-records" aria-labelledby="usage-tab-records" hidden={tab !== "records"} tabIndex={0}><UsageRecords snapshot={snapshot} scope={scope} formatModel={formatModel} resolveBrand={resolveBrand} /></section>
      </> : waiting ? <div className="analytics-loading" role="status"><div className="analytics-summary-skeleton">{[0, 1, 2, 3].map((index) => <span key={index}><i /><i /><i /></span>)}</div><div><LoaderCircle size={20} className="spinning" /><span>{t("usage.page.loading")}</span></div></div> : <UsageEmpty title={t("usage.page.loadFailed")} action={refresh} actionLabel={t("usage.page.reload")} />}
    </div>
    <footer className="analytics-page-footer"><span className={stale || failed ? "analytics-stale" : ""}>{waiting ? <><LoaderCircle size={12} className="spinning" />{t("usage.page.updating")}</> : stale ? t("usage.page.staleFailed") : failed ? t("usage.page.readFailed") : <><span className="analytics-updated-dot" />{t("usage.page.updatedAt", { time: new Date(activeQuery.dataUpdatedAt).toLocaleTimeString(language, { hour12: false }) })}</>}</span><span>{model ? formatModel(model) : t("usage.filter.allModels")} · {dateRange(range)}</span></footer>
  </section>;
}
