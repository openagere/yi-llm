import { useMemo } from "react";
import { Bar, CartesianGrid, ComposedChart, Line, ResponsiveContainer, Tooltip, XAxis, YAxis, type TooltipContentProps } from "recharts";
import { useI18n } from "@/shared/i18n";
import { ProviderIcon } from "@/shared/ui";
import { compactNumber, dateRange, localDay, rangeDays, shortDay } from "../format";
import { usageTokenMetrics } from "../metrics";
import type { UsageModelBrandResolver, UsageModelFormatter } from "../modelNames";
import type { TrendMode, UsageDateRange, UsageSnapshot } from "../types";

const modelColors = ["#339c76", "#4c8fbd", "#d69344", "#af758e", "#79a260", "#9ba5ac"];
interface TrendSeries { key: string; label: string; color: string; kind: "line" | "bar"; model?: string }

export function UsageTrend({ snapshot, range, mode, formatModel, resolveBrand }: { snapshot: UsageSnapshot; range: UsageDateRange; mode: TrendMode; formatModel: UsageModelFormatter; resolveBrand: UsageModelBrandResolver }) {
  const { t, language } = useI18n();
  const days = rangeDays(range);
  const { points, series } = useMemo(() => {
    const leaders = snapshot.models.slice(0, 5);
    const leadingNames = new Set(leaders.map((model) => model.model));
    const grouped = snapshot.models.length > leaders.length;
    const series: TrendSeries[] = mode === "tokens"
      ? usageTokenMetrics.slice(0, 3).map((metric) => ({ key: metric.key, label: t(metric.tokensKey), color: metric.color, kind: metric.key === "cached_tokens" ? "line" : "bar" }))
      : [...leaders.map((model, index) => ({ key: `model${index}`, model: model.model, label: formatModel(model.model), color: modelColors[index], kind: "bar" as const })), ...(grouped ? [{ key: "other", label: t("usage.trend.other"), color: modelColors[5], kind: "bar" as const }] : [])];
    const daily = new Map(snapshot.days.map((day) => [day.day, day]));
    const dailyModels = new Map<string, Map<string, number>>();
    for (const item of snapshot.daily_models) {
      const models = dailyModels.get(item.day) ?? new Map<string, number>();
      models.set(item.model, (models.get(item.model) ?? 0) + item.tokens);
      dailyModels.set(item.day, models);
    }
    const points = Array.from({ length: days }, (_, index) => {
      const date = new Date(`${range.startDate}T12:00:00`);
      date.setDate(date.getDate() + index);
      const day = localDay(date);
      const point: Record<string, string | number> = { day };
      usageTokenMetrics.forEach(({ key }) => { point[key] = daily.get(day)?.[key] ?? 0; });
      const models = dailyModels.get(day) ?? new Map<string, number>();
      leaders.forEach((model, modelIndex) => { point[`model${modelIndex}`] = models.get(model.model) ?? 0; });
      point.other = [...models].reduce((sum, [model, tokens]) => sum + (leadingNames.has(model) ? 0 : tokens), 0);
      return point;
    });
    return { points, series };
  }, [snapshot, days, range.startDate, mode, formatModel, t]);
  const formatTick = (value: number) => compactNumber(value, language);

  return <>
    <div className="analytics-chart" role="img" aria-label={t(mode === "tokens" ? "usage.trend.chartTokens" : "usage.trend.chartModels", { range: dateRange(range) })}>
      <ResponsiveContainer width="100%" height="100%" minWidth={0}>
        <ComposedChart data={points} margin={{ top: 16, right: 8, bottom: 0, left: 0 }} barCategoryGap={days > 30 ? "14%" : "32%"} accessibilityLayer>
          <CartesianGrid stroke="var(--border)" strokeDasharray="3 4" vertical={false} />
          <XAxis dataKey="day" tickFormatter={shortDay} tick={{ fontSize: 10 }} tickLine={false} axisLine={false} minTickGap={28} interval="preserveStartEnd" height={30} />
          <YAxis tickFormatter={formatTick} tick={{ fontSize: 10 }} tickLine={false} axisLine={false} width={47} allowDecimals={false} tickCount={5} />
          <Tooltip content={(props) => <TrendTooltip active={props.active} payload={props.payload} label={props.label} series={series} resolveBrand={resolveBrand} />} cursor={{ fill: "var(--surface-subtle)" }} isAnimationActive={false} />
          {series.map((item) => item.kind === "line" ? <Line key={item.key} dataKey={item.key} name={item.label} type="linear" stroke={item.color} strokeWidth={2} dot={days <= 31 ? { r: 3, fill: "var(--surface)", strokeWidth: 2 } : false} activeDot={{ r: 4, fill: "var(--surface)", strokeWidth: 2 }} isAnimationActive={false} /> : <Bar key={item.key} dataKey={item.key} name={item.label} stackId="usage" fill={item.color} maxBarSize={38} isAnimationActive={false} />)}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
    <div className="analytics-chart-legend" aria-label={t("usage.trend.legend")}>{series.map((item) => <span key={item.key} title={item.label}><i data-series-kind={item.kind} style={{ background: item.color }} />{item.model && <ProviderIcon brand={resolveBrand(item.model)} size={16} />}{item.label}</span>)}</div>
  </>;
}

function TrendTooltip({ active, payload, label, series, resolveBrand }: Pick<TooltipContentProps<number, string>, "active" | "payload" | "label"> & { series: TrendSeries[]; resolveBrand: UsageModelBrandResolver }) {
  const { t, language } = useI18n();
  if (!active || !payload?.length) return null;
  const total = payload.reduce((sum, item) => sum + (item.dataKey === "cached_tokens" ? 0 : Number(item.value)), 0);
  return <div className="analytics-chart-tooltip"><strong>{label}</strong>{payload.map((item) => {
    const model = series.find((entry) => entry.key === item.dataKey)?.model;
    return <div key={String(item.dataKey)}><i style={{ background: item.color }} />{model && <ProviderIcon brand={resolveBrand(model)} size={14} />}<span>{item.name}</span><b>{compactNumber(Number(item.value), language)}</b></div>;
  })}{payload.length > 1 && <div className="tooltip-total"><span>{t("usage.common.totalTokens")}</span><b>{compactNumber(total, language)}</b></div>}</div>;
}
