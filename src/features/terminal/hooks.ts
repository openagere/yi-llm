import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { describeError } from "@/shared/api";
import { terminalApi } from "./api";
import type { TerminalDirectProfile, TerminalProfile, TerminalStatus } from "./types";

export const terminalKeys = {
  profiles: ["terminal", "profiles"] as const,
  preview: (profileKey: string) => ["terminal", "preview", profileKey] as const,
  previewDirect: (profileKey: string) => ["terminal", "preview-direct", profileKey] as const,
};

const PREVIEW_DEBOUNCE_MS = 250;

function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delay);
    return () => window.clearTimeout(timer);
  }, [value, delay]);
  return debounced;
}

/** 各终端客户端的接入状态；每次进入页面都重新读取，以便发现用户手动修改过的配置文件。 */
export function useTerminalProfiles() {
  return useQuery({ queryKey: terminalKeys.profiles, queryFn: terminalApi.profiles, staleTime: 0 });
}

/**
 * 按草稿生成配置预览。草稿变化后先等待 250ms 再请求；等待与请求期间 `pending` 为 true，
 * 且不会返回旧草稿对应的结果。
 */
export function useTerminalPreview(profile: TerminalProfile, enabled: boolean) {
  const profileKey = JSON.stringify(profile);
  const settledKey = useDebouncedValue(profileKey, PREVIEW_DEBOUNCE_MS);
  const settled = settledKey === profileKey;
  const active = enabled && settled;
  const query = useQuery({
    queryKey: terminalKeys.preview(settledKey),
    queryFn: () => terminalApi.preview(JSON.parse(settledKey) as TerminalProfile),
    enabled: active,
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });
  return {
    preview: active ? query.data : undefined,
    error: active && query.error ? describeError(query.error) : "",
    pending: enabled && (!settled || query.isFetching),
  };
}

export function useTerminalDirectPreview(profile: TerminalDirectProfile, enabled: boolean) {
  const profileKey = JSON.stringify(profile);
  const settledKey = useDebouncedValue(profileKey, PREVIEW_DEBOUNCE_MS);
  const settled = settledKey === profileKey;
  const active = enabled && settled;
  const query = useQuery({
    queryKey: terminalKeys.previewDirect(settledKey),
    queryFn: () => terminalApi.previewDirect(JSON.parse(settledKey) as TerminalDirectProfile),
    enabled: active, staleTime: 0, gcTime: 0, refetchOnWindowFocus: false,
  });
  return { preview: active ? query.data : undefined, error: active && query.error ? describeError(query.error) : "", pending: enabled && (!settled || query.isFetching) };
}

export function useApplyTerminalDirectConfig() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (profile: TerminalDirectProfile) => terminalApi.applyDirect(profile),
    onSuccess: (_result, profile) => {
      queryClient.setQueryData<TerminalStatus[]>(terminalKeys.profiles, (items) => items?.map((item) => item.client === profile.client
        ? { ...item, active: true, active_mode: "direct", exists: true, config_error: null, direct_profile: structuredClone(profile) }
        : item));
      void queryClient.invalidateQueries({ queryKey: terminalKeys.profiles });
    },
  });
}

/** 写入终端配置；成功后先本地更新该客户端状态，再重新读取以对齐磁盘内容。 */
export function useApplyTerminalConfig() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (profile: TerminalProfile) => terminalApi.apply(profile),
    onSuccess: (_result, profile) => {
      queryClient.setQueryData<TerminalStatus[]>(terminalKeys.profiles, (items) => items?.map((item) => item.client === profile.client
        ? { ...item, active: true, exists: true, config_error: null, profile: structuredClone(profile) }
        : item));
      void queryClient.invalidateQueries({ queryKey: terminalKeys.profiles });
    },
  });
}
