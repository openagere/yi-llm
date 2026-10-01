import { memo } from "react";
import { useI18n } from "@/shared/i18n";
import type { LogEntry } from "../logs";

const LogRow = memo(function LogRow({ entry }: { entry: LogEntry }) {
  const level = entry.level.toLowerCase();
  return <tr className={`log-${level}`}><td title={entry.timestamp}>{entry.time}</td><td><span className={`proxy-log-badge ${level}`}>{entry.level}</span></td><td>{entry.message}</td></tr>;
}, (previous, next) => previous.entry.raw === next.entry.raw && previous.entry.time === next.entry.time);

/** 日志每次轮询都会重新解析；同位置且内容未变的行跳过重渲染。 */
export function LogTable({ entries }: { entries: LogEntry[] }) {
  const { t } = useI18n();
  return <table>
    <colgroup><col className="proxy-log-time-col" /><col className="proxy-log-level-col" /><col /></colgroup>
    <thead><tr><th>{t("proxy.logs.time")}</th><th>{t("proxy.logs.level")}</th><th>{t("proxy.logs.message")}</th></tr></thead>
    <tbody>{entries.map((entry, index) => <LogRow key={`${index}-${entry.timestamp}`} entry={entry} />)}</tbody>
  </table>;
}
