import { useI18n } from "@/shared/i18n";
import type { UsageModelBrandResolver, UsageModelFormatter } from "../modelNames";
import type { TrendMode, UsageDateRange, UsageSnapshot } from "../types";
import { UsageEmpty } from "./UsageCommon";
import { UsageSummary } from "./UsageSummary";
import { UsageTrend } from "./UsageTrend";

export function UsageOverview({ snapshot, range, active, mode, onMode, formatModel, resolveBrand }: { snapshot: UsageSnapshot; range: UsageDateRange; active: boolean; mode: TrendMode; onMode: (mode: TrendMode) => void; formatModel: UsageModelFormatter; resolveBrand: UsageModelBrandResolver }) {
  const { t } = useI18n();
  return <>
    <UsageSummary snapshot={snapshot} />
    <section className="analytics-trend-section">
      <div className="analytics-section-heading"><div><h2>{t("usage.overview.trendTitle")}</h2><span>{t("usage.overview.trendUnit")}</span></div><div className="analytics-mode-control" role="group" aria-label={t("usage.overview.modeGroup")}>{(["tokens", "models"] as const).map((id) => <button type="button" key={id} aria-pressed={mode === id} className={mode === id ? "selected" : ""} onClick={() => onMode(id)}>{t(id === "tokens" ? "usage.overview.modeTokens" : "usage.overview.modeModels")}</button>)}</div></div>
      {snapshot.total_requests === 0 ? <UsageEmpty /> : active && <UsageTrend snapshot={snapshot} range={range} mode={mode} formatModel={formatModel} resolveBrand={resolveBrand} />}
    </section>
  </>;
}
