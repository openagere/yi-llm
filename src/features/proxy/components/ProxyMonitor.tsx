import { useState } from "react";
import { Activity, CircleAlert, Download, RefreshCw } from "lucide-react";
import { CartesianGrid, Legend, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { describeError } from "@/shared/api";
import { useI18n, type MessageKey } from "@/shared/i18n";
import { PROTOCOLS_BY_ID, type ProviderType } from "@/shared/lib";
import { ProviderIcon } from "@/shared/ui";
import { formatBytes, formatClock, formatCount, formatDateTime } from "../format";
import { useProxyMonitor } from "../hooks";

const SERIES = {
  traffic: [{ key: "received_rate", label: "proxy.monitor.series.receivedRate", color: "var(--chart-received, #25815b)" }, { key: "sent_rate", label: "proxy.monitor.series.sentRate", color: "var(--chart-sent, #427ea0)" }],
  connections: [{ key: "connections", label: "proxy.monitor.series.connections", color: "var(--chart-sent, #427ea0)" }, { key: "active_requests", label: "proxy.monitor.series.activeRequests", color: "var(--chart-received, #25815b)" }],
  requests: [{ key: "requests", label: "proxy.monitor.series.requests", color: "var(--chart-received, #25815b)" }, { key: "errors", label: "proxy.monitor.series.errors", color: "var(--chart-errors, #b34f46)" }],
} as const satisfies Record<string, readonly { key: string; label: MessageKey; color: string }[]>;

const CLIENTS: Record<string, MessageKey | undefined> = {
  api: "proxy.monitor.clients.api",
  unknown: "proxy.monitor.clients.unknown",
};
const CLIENT_NAMES: Record<string, string> = { codex: "Codex CLI", "claude-code": "Claude Code", opencode: "OpenCode" };

export function ProxyMonitor({ visible }: { visible: boolean }) {
  const { t, language } = useI18n();
  const [minutes, setMinutes] = useState(15);
  const [mode, setMode] = useState<keyof typeof SERIES>("traffic");
  const query = useProxyMonitor(minutes, { active: visible });
  const snapshot = query.data;
  const stale = query.isError;
  const error = stale ? describeError(query.error) : "";
  const total = snapshot?.totals;
  const count = (value: number) => formatCount(value, language);
  const clock = (timestamp: number) => formatClock(timestamp, language);
  const series = snapshot?.samples.filter((sample) => mode === "connections" || sample.timestamp + 5000 <= snapshot.sampled_at).map((sample) => ({ ...sample, received_rate: sample.received_bytes / 5, sent_rate: sample.sent_bytes / 5 })) ?? [];
  const fact = (label: string, value: string, detail: string) => <div><dt>{label}</dt><dd>{stale ? "--" : value}</dd><small>{stale ? t("proxy.monitor.unavailable") : detail}</small></div>;

  function exportReport() {
    if (!snapshot) return;
    const url = URL.createObjectURL(new Blob([JSON.stringify(snapshot, null, 2)], { type: "application/json" }));
    const link = document.createElement("a"); link.href = url; link.download = `proxy-monitor-${Date.now()}.json`; link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  return <section className="proxy-monitor" aria-label={t("proxy.monitor.report")}>
    <div className="proxy-monitor-heading"><div className="proxy-section-title"><Activity size={16} /><h2>{t("proxy.monitor.heading")}</h2><span>{snapshot ? t("proxy.monitor.updatedAt", { time: clock(snapshot.sampled_at) }) : t("proxy.monitor.reading")}</span></div><div className="proxy-monitor-tools"><button type="button" className="icon-button" title={t("proxy.monitor.refresh")} aria-label={t("proxy.monitor.refresh")} onClick={() => void query.refetch()}><RefreshCw size={15} /></button><button type="button" className="icon-button" title={t("proxy.monitor.export")} aria-label={t("proxy.monitor.export")} onClick={exportReport} disabled={!snapshot || stale}><Download size={15} /></button></div></div>
    {error && <div className="proxy-log-error" role="alert"><CircleAlert size={15} /><span>{t("proxy.errors.monitorFailed")}{error}</span></div>}
    <dl className="proxy-monitor-facts">
      {fact(t("proxy.monitor.facts.connections"), total ? count(total.connections) : "--", t("proxy.monitor.facts.connectionsDetail", { peak: total?.peak_connections ?? "--" }))}
      {fact(t("proxy.monitor.facts.accepted"), total ? count(total.accepted_connections) : "--", t("proxy.monitor.facts.acceptedDetail", { active: total?.active_requests ?? "--" }))}
      {fact(t("proxy.monitor.facts.requests"), total ? count(total.requests) : "--", t("proxy.monitor.facts.requestsDetail", { completed: total?.completed ?? "--" }))}
      {fact(t("proxy.monitor.facts.errorRate"), total ? `${(total.completed ? total.errors / total.completed * 100 : 0).toFixed(1)}%` : "--", t("proxy.monitor.facts.errorRateDetail", { errors: total?.errors ?? "--" }))}
      {fact(t("proxy.monitor.facts.received"), total ? formatBytes(total.received_bytes) : "--", snapshot ? `${formatBytes(snapshot.received_per_second)}/s` : "--")}
      {fact(t("proxy.monitor.facts.sent"), total ? formatBytes(total.sent_bytes) : "--", snapshot ? `${formatBytes(snapshot.sent_per_second)}/s` : "--")}
    </dl>
    <div className="proxy-monitor-chart-toolbar"><div className="monitor-segments" role="group" aria-label={t("proxy.monitor.metrics")}>{(["traffic", "connections", "requests"] as const).map((id) => <button type="button" key={id} aria-pressed={mode === id} onClick={() => setMode(id)}>{t(`proxy.monitor.metric.${id}`)}</button>)}</div><div className="monitor-segments" role="group" aria-label={t("proxy.monitor.range")}>{[5, 15, 60].map((value) => <button type="button" key={value} aria-pressed={minutes === value} onClick={() => setMinutes(value)}>{t("proxy.monitor.minutes", { value })}</button>)}</div></div>
    <div className="proxy-monitor-chart" aria-label={t("proxy.monitor.trend")}>
      {visible && snapshot && !stale && series.length ? <ResponsiveContainer width="100%" height="100%" minWidth={0}><LineChart data={series} margin={{ top: 15, right: 15, left: 0, bottom: 5 }}><CartesianGrid stroke="var(--border)" strokeDasharray="3 3" vertical={false} /><XAxis dataKey="timestamp" type="number" domain={[snapshot.sampled_at - minutes * 60_000, snapshot.sampled_at]} tickFormatter={clock} tick={{ fontSize: 10 }} tickLine={false} axisLine={false} minTickGap={60} /><YAxis tickFormatter={mode === "traffic" ? formatBytes : (value: number) => count(value)} allowDecimals={mode === "traffic"} tick={{ fontSize: 10 }} tickLine={false} axisLine={false} width={65} /><Tooltip labelFormatter={(value) => clock(Number(value))} formatter={(value, name) => [mode === "traffic" ? `${formatBytes(Number(value))}/s` : count(Number(value)), name]} /><Legend wrapperStyle={{ fontSize: 11 }} />{SERIES[mode].map((line) => <Line key={line.key} dataKey={line.key} name={t(line.label)} stroke={line.color} strokeWidth={2} type={mode === "connections" ? "stepAfter" : "linear"} dot={series.length === 1} isAnimationActive={false} />)}</LineChart></ResponsiveContainer> : <div className="proxy-loading">{stale ? t("proxy.monitor.dataUnavailable") : snapshot ? t("proxy.monitor.waiting") : t("proxy.loading.monitor")}</div>}
    </div>
    <div className="proxy-monitor-summary"><span title={t("proxy.monitor.avgDurationHint")}>{t("proxy.monitor.avgDurationValue", { value: total && !stale ? `${Math.round(total.duration_ms / Math.max(1, total.completed))} ms` : "--" })}</span><span title={t("proxy.monitor.perSecondHint")}>{t("proxy.monitor.perSecond", { value: snapshot && !stale ? snapshot.requests_per_second.toFixed(2) : "--" })}</span></div>
    <details className="monitor-routes-details"><summary>{t("proxy.monitorRoutes")}<span>{snapshot?.routes.length ?? 0}</span></summary>
      <div className="proxy-section-title monitor-route-title"><h2>{t("proxy.monitor.routes.title")}</h2><span title={t("proxy.monitor.routes.payloadHint")}>{t("proxy.monitor.routes.payload")}</span></div>
      <div className="monitor-route-table"><table><thead><tr><th>{t("proxy.monitor.routes.entry")}</th><th>{t("proxy.monitor.routes.protocol")}</th><th>{t("proxy.monitor.routes.requests")}</th><th>{t("proxy.monitor.routes.active")}</th><th>{t("proxy.monitor.routes.failed")}</th><th>{t("proxy.monitor.routes.requestBody")}</th><th>{t("proxy.monitor.routes.responseBody")}</th><th>{t("proxy.monitor.avgDuration")}</th></tr></thead><tbody>{!stale && snapshot?.routes.map((route) => {
        const protocol = PROTOCOLS_BY_ID[route.protocol as ProviderType];
        const clientKey = CLIENTS[route.client];
        const client = clientKey ? t(clientKey) : CLIENT_NAMES[route.client] ?? route.client;
        const protocolName = protocol?.shortName ?? (route.protocol === "models" ? t("proxy.monitor.routes.modelList") : t("proxy.monitor.routes.other"));
        return <tr key={`${route.client}:${route.protocol}`}><td>{client}</td><td><span className="monitor-protocol">{protocol && <ProviderIcon brand={protocol.brand} size={13} />}{protocolName}</span></td><td>{count(route.requests)}</td><td>{route.active_requests}</td><td className={route.errors ? "monitor-errors" : ""}>{route.errors}</td><td>{formatBytes(route.received_bytes)}</td><td>{formatBytes(route.sent_bytes)}</td><td>{Math.round(route.duration_ms / Math.max(1, route.completed))} ms</td></tr>;
      })}{(stale || !snapshot?.routes.length) && <tr><td colSpan={8} className="monitor-empty">{stale ? t("proxy.monitor.unavailable") : t("proxy.monitor.routes.empty")}</td></tr>}</tbody></table></div>
      <p className="proxy-monitor-scope">{t("proxy.monitor.scope", { since: snapshot ? formatDateTime(snapshot.since, language) : "--" })}</p>
    </details>
  </section>;
}
