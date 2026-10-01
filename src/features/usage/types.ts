export interface UsageDateRange { startDate: string; endDate: string }
export interface UsageDay { day: string; input_tokens: number; output_tokens: number; cached_tokens: number; reasoning_tokens: number }
export interface UsageDailyModel { day: string; model: string; tokens: number }
export interface UsageModel { model: string; requests: number; input_tokens: number; output_tokens: number; cached_tokens: number; reasoning_tokens: number }
export interface UsageRecord { id: number; requested_at: number; provider_name: string; protocol: string; model: string; input_tokens: number; output_tokens: number; cached_tokens: number; reasoning_tokens: number }
export interface UsageSnapshot { total_requests: number; input_tokens: number; output_tokens: number; cached_tokens: number; reasoning_tokens: number; days: UsageDay[]; daily_models: UsageDailyModel[]; models: UsageModel[]; recent: UsageRecord[] }

export type TrendMode = "tokens" | "models";
