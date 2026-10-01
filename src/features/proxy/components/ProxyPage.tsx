import { lazy, Suspense, useState } from "react";
import { Activity, ArrowRight, CircleAlert, LoaderCircle, Play, RotateCw, Settings2, Square, Workflow, X } from "lucide-react";
import "@/styles/proxy.css";
import { describeError } from "@/shared/api";
import { useProviders } from "@/features/providers";
import { useI18n } from "@/shared/i18n";
import { useNavigation } from "@/shared/navigation";
import { PageHeader } from "@/shared/ui";
import { formatListenAddress, formatUptime } from "../format";
import { useProxyControl, useProxyRuntime } from "../hooks";
import { ProxyConnections } from "./ProxyConnections";
import { ProxyListenSettings } from "./ProxyListenSettings";

const ProxyMonitor = lazy(() => import("./ProxyMonitor").then((module) => ({ default: module.ProxyMonitor })));
const ProxyLogs = lazy(() => import("./ProxyLogs").then((module) => ({ default: module.ProxyLogs })));
const views = ["monitor", "connections", "logs"] as const;

function openListenSettings() {
  const dialog = document.getElementById("proxy-listen-options") as HTMLDialogElement | null;
  if (dialog && !dialog.open) dialog.showModal();
  dialog?.querySelector<HTMLInputElement>("input")?.focus();
}

export function ProxyPage({ active }: { active: boolean }) {
  const { t } = useI18n();
  const { back, navigate } = useNavigation();
  const runtimeQuery = useProxyRuntime();
  const { control, busy } = useProxyControl();
  const providersQuery = useProviders();
  const [view, setView] = useState<typeof views[number]>("monitor");
  const runtime = runtimeQuery.data;
  const error = runtimeQuery.isError ? describeError(runtimeQuery.error) : "";
  const phase = error ? null : runtime?.phase;
  const running = phase === "running";
  const transitioning = phase === "starting" || phase === "stopping";
  const providers = providersQuery.data ?? [];
  const enabled = providers.filter((provider) => provider.enabled);
  const models = new Set(enabled.flatMap((provider) => provider.models.map((model) => model.route_id)));
  const statusLabel = phase ? t(`nav.phase.${phase}`) : error ? t("proxy.statusUnavailable") : t("proxy.statusConnecting");
  const refreshing = runtimeQuery.isFetching;

  return <section className="proxy-page proxy-workspace">
    <PageHeader navigationInToolbar eyebrow={t("proxy.eyebrow")} title={t("proxy.title")} subtitle={t("pageDescriptions.proxy")} onBack={back} backLabel={t("common.back")} actions={<>
      <button type="button" className="icon-button" title={t("proxy.refresh")} aria-label={t("proxy.refresh")} disabled={refreshing} onClick={() => void runtimeQuery.refetch()}><RotateCw size={16} className={refreshing ? "proxy-spin" : ""} /></button>
      <button type="button" className="icon-button" title={t("proxy.alerts.checkSettings")} aria-label={t("proxy.alerts.checkSettings")} onClick={openListenSettings}><Settings2 size={17} /></button>
    </>} />
    <section className="proxy-runtime" aria-label={t("proxy.title")}>
      <div className="proxy-runtime-main">
        <div className={`proxy-state-symbol ${phase ?? "unknown"}`}>{transitioning || (!runtime && !error) ? <LoaderCircle size={21} className="proxy-spin" /> : phase === "error" || error ? <CircleAlert size={21} /> : <Activity size={21} />}</div>
        <div className="proxy-state-copy"><span>{t("proxy.localProxy")}</span><strong role="status">{statusLabel}</strong></div>
        <div className="proxy-runtime-actions">
          {running && <button type="button" className="icon-button" title={t("proxy.restart")} aria-label={t("proxy.restart")} disabled={!!busy || transitioning} onClick={() => control("restart")}><RotateCw size={16} /></button>}
          <button type="button" className={running ? "secondary-button" : "primary-button"} disabled={!!busy || transitioning || !runtime || !!error} onClick={() => control(running ? "stop" : "start")}>
            {busy || transitioning ? <LoaderCircle size={15} className="proxy-spin" /> : running ? <Square size={13} /> : <Play size={15} />}
            {phase === "stopping" ? t("proxy.stopping") : phase === "starting" ? t("proxy.starting") : running ? t("proxy.stop") : t("proxy.start")}
          </button>
        </div>
      </div>
      <dl className="proxy-runtime-facts">
        <div><dt>{t("proxy.facts.listen")}</dt><dd><code>{runtime?.address ?? (runtime ? formatListenAddress(runtime.settings) : "--")}</code></dd></div>
        <div><dt>{t("proxy.facts.uptime")}</dt><dd>{error ? "--" : formatUptime(runtime?.started_at ?? null, runtimeQuery.dataUpdatedAt, t)}</dd></div>
        <div><dt>{t("proxy.facts.connections")}</dt><dd>{runtime && !error ? runtime.active_connections : "--"}<small>{t("proxy.facts.tcp")}</small><span className="proxy-fact-divider">/</span>{runtime && !error ? runtime.active_requests : "--"}<small>{t("proxy.facts.requests")}</small></dd></div>
        <div><dt>{t("proxy.facts.routes")}</dt><dd>{providersQuery.isPending || providersQuery.isError ? "--" : enabled.length}<small>{t("proxy.facts.providersCount")}</small><span className="proxy-fact-divider">/</span>{providersQuery.isPending || providersQuery.isError ? "--" : models.size}<small>{t("proxy.facts.modelsCount")}</small></dd></div>
      </dl>
    </section>
    {(error || runtime?.last_error) && <div className="proxy-alert error" role="alert"><CircleAlert size={17} /><div><strong>{error ? t("proxy.alerts.statusFailed") : t("proxy.alerts.startFailed")}</strong><p>{error || runtime?.last_error}</p></div><button type="button" className="text-button" onClick={error ? () => void runtimeQuery.refetch() : openListenSettings}>{error ? t("common.retry") : t("proxy.alerts.checkSettings")}<ArrowRight size={14} /></button></div>}
    {!providersQuery.isPending && !providersQuery.isError && enabled.length === 0 && <div className="proxy-alert"><Workflow size={17} /><div><strong>{t("proxy.alerts.noProviders")}</strong></div><button type="button" className="text-button" onClick={() => navigate("provider-list")}>{t("proxy.alerts.manageProviders")}<ArrowRight size={14} /></button></div>}
    {phase === "stopped" && <div className="proxy-stopped-note"><span className="proxy-status-dot stopped" />{t("proxy.alerts.stoppedNote")}</div>}
    <div className="proxy-workspace-tabs" role="tablist" aria-label={t("proxy.workspace")} onKeyDown={(event) => {
      if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const next = event.key === "Home" ? views[0] : event.key === "End" ? views[2] : views[(views.indexOf(view) + (event.key === "ArrowRight" ? 1 : 2)) % 3];
      setView(next); document.getElementById(`proxy-${next}-tab`)?.focus();
    }}>
      <button type="button" id="proxy-monitor-tab" role="tab" tabIndex={view === "monitor" ? 0 : -1} aria-selected={view === "monitor"} aria-controls="proxy-monitor-panel" onClick={() => setView("monitor")}>{t("proxy.tabs.monitor")}</button>
      <button type="button" id="proxy-connections-tab" role="tab" tabIndex={view === "connections" ? 0 : -1} aria-selected={view === "connections"} aria-controls="proxy-connections-panel" onClick={() => setView("connections")}>{t("proxy.tabs.connections")}</button>
      <button type="button" id="proxy-logs-tab" role="tab" tabIndex={view === "logs" ? 0 : -1} aria-selected={view === "logs"} aria-controls="proxy-logs-panel" onClick={() => setView("logs")}>{t("proxy.tabs.logs")}</button>
      <span className={`proxy-live-label ${running ? "online" : ""}`}><span className={`proxy-status-dot ${phase ?? "unknown"}`} />{statusLabel}</span>
    </div>
    <div id="proxy-monitor-panel" role="tabpanel" aria-labelledby="proxy-monitor-tab" hidden={view !== "monitor"}>
      <Suspense fallback={<div className="proxy-loading">{t("proxy.loading.monitor")}</div>}><ProxyMonitor visible={active && view === "monitor"} /></Suspense>
    </div>
    <div id="proxy-connections-panel" role="tabpanel" aria-labelledby="proxy-connections-tab" hidden={view !== "connections"}>
      {runtime ? <ProxyConnections settings={runtime.settings} /> : <div className="proxy-loading">{error ? t("proxy.loading.connectionsFailed") : t("proxy.loading.connections")}</div>}
    </div>
    <div id="proxy-logs-panel" role="tabpanel" aria-labelledby="proxy-logs-tab" hidden={view !== "logs"}>
      <Suspense fallback={<div className="proxy-loading">{t("proxy.logs.loading")}</div>}><ProxyLogs visible={active && view === "logs"} /></Suspense>
    </div>
    <dialog className="proxy-listen-dialog" id="proxy-listen-options" aria-label={t("proxy.listen.heading")}>
      <button type="button" className="icon-button proxy-listen-close" title={t("proxy.closeSettings")} aria-label={t("proxy.closeSettings")} onClick={() => (document.getElementById("proxy-listen-options") as HTMLDialogElement | null)?.close()}><X size={16} /></button>
      <ProxyListenSettings />
    </dialog>
  </section>;
}
