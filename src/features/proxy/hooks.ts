import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { describeError } from "@/shared/api";
import { translate } from "@/shared/i18n";
import { notify } from "@/shared/notify";
import { proxyApi } from "./api";
import type { ProxyAction, ProxyPhase, ServerSettings } from "./types";

export const proxyKeys = {
  all: ["proxy"] as const,
  runtime: ["proxy", "runtime"] as const,
  settings: ["proxy", "settings"] as const,
  monitor: (minutes: number) => ["proxy", "monitor", minutes] as const,
  logs: ["proxy", "logs"] as const,
  control: ["proxy", "control"] as const,
};

const POLL_INTERVAL = 2500;
const TRANSITION_INTERVAL = 600;

/** 运行状态：启动/停止过渡期加快轮询；页面隐藏时 TanStack 默认暂停。 */
export function useProxyRuntime() {
  return useQuery({
    queryKey: proxyKeys.runtime,
    queryFn: proxyApi.runtime,
    staleTime: 0,
    refetchInterval: (query) => {
      const phase = query.state.data?.phase;
      return phase === "starting" || phase === "stopping" ? TRANSITION_INTERVAL : POLL_INTERVAL;
    },
  });
}

/** 供其他功能（如侧栏）读取代理阶段；读取失败或尚无数据时为 null。 */
export function useProxyPhase(): ProxyPhase | null {
  const { data, error } = useProxyRuntime();
  return error ? null : data?.phase ?? null;
}

export function useProxyControl() {
  const client = useQueryClient();
  const mutation = useMutation({
    mutationKey: proxyKeys.control,
    mutationFn: async (action: ProxyAction) => {
      await proxyApi.control(action);
      await client.refetchQueries({ queryKey: proxyKeys.runtime });
    },
    onError: (error) => notify(`${translate("proxy.errors.controlFailed")}${describeError(error)}`, "error"),
  });
  const { mutate } = mutation;

  const control = (action: ProxyAction) => {
    if (client.isMutating({ mutationKey: proxyKeys.control }) > 0) return;
    mutate(action);
  };
  return { control, busy: mutation.isPending ? mutation.variables : null };
}

export function useProxySettings() {
  return useQuery({ queryKey: proxyKeys.settings, queryFn: proxyApi.settings });
}

export function useSaveProxySettings() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (settings: ServerSettings) => proxyApi.saveSettings(settings),
    onSuccess: (_result, settings) => {
      client.setQueryData(proxyKeys.settings, settings);
      return client.invalidateQueries({ queryKey: proxyKeys.runtime });
    },
  });
}

export function useProxyMonitor(minutes: number, { active }: { active: boolean }) {
  return useQuery({
    queryKey: proxyKeys.monitor(minutes),
    queryFn: () => proxyApi.monitor(minutes),
    enabled: active,
    staleTime: 0,
    refetchOnWindowFocus: false,
    refetchInterval: active ? POLL_INTERVAL : false,
    placeholderData: keepPreviousData,
  });
}

export function useProxyLogs({ active, autoRefresh = true }: { active: boolean; autoRefresh?: boolean }) {
  return useQuery({
    queryKey: proxyKeys.logs,
    queryFn: proxyApi.logs,
    enabled: active,
    staleTime: 0,
    refetchOnWindowFocus: false,
    refetchInterval: active && autoRefresh ? POLL_INTERVAL : false,
  });
}
