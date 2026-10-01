import { ArrowDownToLine, ArrowUpFromLine, Brain, Database } from "lucide-react";

export const usageTokenMetrics = [
  { key: "input_tokens", labelKey: "usage.metric.input", tokensKey: "usage.metricTokens.input", icon: ArrowDownToLine, className: "metric-input", color: "#339c76" },
  { key: "output_tokens", labelKey: "usage.metric.output", tokensKey: "usage.metricTokens.output", icon: ArrowUpFromLine, className: "metric-output", color: "#4c8fbd" },
  { key: "cached_tokens", labelKey: "usage.metric.cached", tokensKey: "usage.metricTokens.cached", icon: Database, className: "metric-cache", color: "#66796c" },
  { key: "reasoning_tokens", labelKey: "usage.metric.reasoning", tokensKey: "usage.metricTokens.reasoning", icon: Brain, className: "metric-reasoning", color: "#a07694" },
] as const;

export const usageProtocolNames: Record<string, string> = { responses: "Responses", openai_chat: "Chat Completions", anthropic: "Anthropic Messages" };
