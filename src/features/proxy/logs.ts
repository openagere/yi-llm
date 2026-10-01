export type LogLevel = "ERROR" | "WARN" | "INFO" | "DEBUG" | "TRACE" | "LOG";

export interface LogEntry {
  raw: string;
  timestamp: string;
  time: string;
  level: LogLevel;
  message: string;
}

const ANSI_COLOR = new RegExp(`${String.fromCharCode(0x1b)}\\[[0-9;]*m`, "g");
const LOG_LINE = /^(\d{4}-\d{2}-\d{2}T\S+)\s+(ERROR|WARN|INFO|DEBUG|TRACE)\s+(.*)$/;

function formatTime(timestamp: string, language: string): string {
  if (!timestamp) return "--";
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return "--";
  return `${date.toLocaleTimeString(language, { hour12: false })}.${String(date.getMilliseconds()).padStart(3, "0")}`;
}

/** 解析后端日志文本；无时间戳的行视为上一条的续行（如堆栈）。 */
export function parseLogs(text: string, language: string): LogEntry[] {
  const entries: LogEntry[] = [];
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.replace(ANSI_COLOR, "");
    if (!line.trim()) continue;
    const match = LOG_LINE.exec(line);
    if (!match && entries.length) {
      const previous = entries[entries.length - 1];
      previous.raw += `\n${line}`;
      previous.message += `\n${line}`;
      continue;
    }
    const timestamp = match?.[1] ?? "";
    entries.push({ raw: line, timestamp, time: formatTime(timestamp, language), level: (match?.[2] ?? "LOG") as LogLevel, message: match?.[3] ?? line });
  }
  return entries;
}
