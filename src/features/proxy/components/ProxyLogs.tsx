import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowDownToLine, CircleAlert, Clipboard, Download, FileText, Maximize2, Minimize2, Pause, Play, RefreshCw, Search, X } from "lucide-react";
import { describeError } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { notify } from "@/shared/notify";
import { Select } from "@/shared/ui";
import { useProxyLogs } from "../hooks";
import { parseLogs } from "../logs";
import { LogTable } from "./LogTable";

export function ProxyLogs({ visible }: { visible: boolean }) {
  const { t, language } = useI18n();
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [follow, setFollow] = useState(true);
  const [expanded, setExpanded] = useState(false);
  const [query, setQuery] = useState("");
  const [level, setLevel] = useState("all");
  const output = useRef<HTMLDivElement>(null);
  const logsQuery = useProxyLogs({ active: visible, autoRefresh });
  const logs = logsQuery.data ?? "";
  const loaded = logsQuery.data !== undefined;
  const error = logsQuery.isError ? `${t("proxy.errors.logsFailed")}${describeError(logsQuery.error)}` : "";
  const entries = useMemo(() => parseLogs(logs, language), [logs, language]);
  const filtered = useMemo(() => entries.filter((entry) =>
    (level === "all" || level === "issues" && (entry.level === "WARN" || entry.level === "ERROR") || entry.level === level)
    && entry.raw.toLowerCase().includes(query.trim().toLowerCase())), [entries, level, query]);
  const issues = entries.filter((entry) => entry.level === "ERROR" || entry.level === "WARN").length;
  const levels = [
    { value: "all", label: t("proxy.logs.levelAll") },
    { value: "issues", label: t("proxy.logs.levelIssues") },
    { value: "ERROR", label: "ERROR" }, { value: "WARN", label: "WARN" }, { value: "INFO", label: "INFO" },
    { value: "DEBUG", label: "DEBUG" }, { value: "TRACE", label: "TRACE" },
  ];

  useEffect(() => { if (follow && visible && output.current) output.current.scrollTop = output.current.scrollHeight; }, [filtered, follow, visible, expanded]);

  async function copy() {
    try { await navigator.clipboard.writeText(filtered.map((entry) => entry.raw).join("\n")); notify(t("proxy.logs.copied")); }
    catch { notify(t("proxy.logs.copyFailed"), "error"); }
  }
  function download() {
    const blob = new Blob([filtered.map((entry) => entry.raw).join("\n")], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url; link.download = `yi-llm-${new Date().toISOString().replace(/[:.]/g, "-")}.log`;
    link.click(); window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  const empty = !loaded && logsQuery.isFetching ? t("proxy.logs.loading") : error && !loaded ? t("proxy.logs.unavailable") : entries.length ? t("proxy.logs.noMatch") : t("proxy.logs.empty");

  return <section className={`proxy-logs ${expanded ? "expanded" : ""}`} aria-label={t("proxy.logs.title")}>
    <div className="proxy-log-heading"><div className="proxy-section-title"><FileText size={16} /><h2>{t("proxy.logs.title")}</h2><span>{entries.length}</span>{issues > 0 && <button type="button" className="proxy-issue-count" title={t("proxy.logs.filterIssues")} onClick={() => setLevel(level === "issues" ? "all" : "issues")}><CircleAlert size={12} />{issues}</button>}</div><span className={`proxy-log-mode ${autoRefresh ? "live" : ""}`}><span className={`proxy-status-dot ${autoRefresh ? "running" : "stopped"}`} />{autoRefresh ? t("proxy.logs.live") : t("proxy.logs.paused")}</span></div>
    <div className="proxy-log-toolbar">
      <div className="proxy-log-search"><Search size={15} /><input aria-label={t("proxy.logs.searchLabel")} value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("proxy.logs.searchPlaceholder")} />{query && <button type="button" title={t("proxy.logs.clearSearch")} aria-label={t("proxy.logs.clearSearch")} onClick={() => setQuery("")}><X size={14} /></button>}</div>
      <Select value={level} onValueChange={setLevel} options={levels} label={t("proxy.logs.levelLabel")} className="proxy-log-level" />
      <div className="proxy-log-tools">
        <button type="button" className="icon-button" title={autoRefresh ? t("proxy.logs.pauseRefresh") : t("proxy.logs.resumeRefresh")} aria-label={autoRefresh ? t("proxy.logs.pauseRefresh") : t("proxy.logs.resumeRefresh")} onClick={() => setAutoRefresh((value) => !value)}>{autoRefresh ? <Pause size={15} /> : <Play size={15} />}</button>
        <button type="button" className="icon-button" title={t("proxy.logs.refresh")} aria-label={t("proxy.logs.refresh")} disabled={logsQuery.isFetching} onClick={() => void logsQuery.refetch()}><RefreshCw size={15} className={logsQuery.isFetching ? "proxy-spin" : ""} /></button>
        <button type="button" className="icon-button" title={t("proxy.logs.copy")} aria-label={t("proxy.logs.copy")} disabled={!filtered.length} onClick={() => void copy()}><Clipboard size={15} /></button>
        <button type="button" className="icon-button" title={t("proxy.logs.export")} aria-label={t("proxy.logs.export")} disabled={!filtered.length} onClick={download}><Download size={15} /></button>
        <button type="button" className="icon-button" title={expanded ? t("proxy.logs.collapse") : t("proxy.logs.expand")} aria-label={expanded ? t("proxy.logs.collapse") : t("proxy.logs.expand")} aria-pressed={expanded} onClick={() => setExpanded((value) => !value)}>{expanded ? <Minimize2 size={15} /> : <Maximize2 size={15} />}</button>
      </div>
    </div>
    {error && <div className="proxy-log-error" role="alert"><CircleAlert size={15} /><span>{error}</span><button type="button" className="text-button" onClick={() => void logsQuery.refetch()}>{t("common.retry")}</button></div>}
    <div className="proxy-log-console" ref={output} tabIndex={0} aria-label={t("proxy.logs.content")} onScroll={() => {
      if (output.current && output.current.scrollHeight - output.current.scrollTop - output.current.clientHeight > 40) setFollow(false);
    }}>
      {filtered.length ? <LogTable entries={filtered} />
        : <div className="proxy-log-empty"><FileText size={24} /><strong>{empty}</strong>{entries.length > 0 && <button type="button" className="text-button" onClick={() => { setQuery(""); setLevel("all"); }}>{t("proxy.logs.clearFilters")}</button>}</div>}
    </div>
    <div className="proxy-log-footer"><span>{t("proxy.logs.count", { shown: filtered.length, total: entries.length })}<span className="proxy-footer-separator">·</span>{t("proxy.logs.recent")}</span><span>{logsQuery.dataUpdatedAt ? new Date(logsQuery.dataUpdatedAt).toLocaleTimeString(language, { hour12: false }) : "--"}</span><label><input type="checkbox" checked={follow} onChange={(event) => setFollow(event.target.checked)} /><ArrowDownToLine size={13} />{t("proxy.logs.follow")}</label></div>
  </section>;
}
