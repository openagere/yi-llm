export type LogLevelSetting = "error" | "warn" | "info" | "debug" | "trace";

export interface ServerSettings {
  host: string;
  port: number;
  log_level: LogLevelSetting;
}

export type ProxyPhase = "starting" | "running" | "stopping" | "stopped" | "error";
export type ProxyAction = "start" | "stop" | "restart";

export interface ProxyRuntime {
  phase: ProxyPhase;
  desired_running: boolean;
  started_at: number | null;
  address: string | null;
  last_error: string | null;
  active_requests: number;
  active_connections: number;
  settings: ServerSettings;
}

export interface MonitorCounters {
  connections: number;
  accepted_connections: number;
  peak_connections: number;
  active_requests: number;
  peak_requests: number;
  requests: number;
  completed: number;
  errors: number;
  received_bytes: number;
  sent_bytes: number;
  duration_ms: number;
}

export interface MonitorSample {
  timestamp: number;
  connections: number;
  active_requests: number;
  requests: number;
  errors: number;
  received_bytes: number;
  sent_bytes: number;
}

export interface MonitorRoute {
  client: string;
  protocol: string;
  requests: number;
  active_requests: number;
  completed: number;
  errors: number;
  received_bytes: number;
  sent_bytes: number;
  duration_ms: number;
}

export interface ProxyMonitorSnapshot {
  since: number;
  sampled_at: number;
  totals: MonitorCounters;
  rate_seconds: number;
  received_per_second: number;
  sent_per_second: number;
  requests_per_second: number;
  samples: MonitorSample[];
  routes: MonitorRoute[];
}
