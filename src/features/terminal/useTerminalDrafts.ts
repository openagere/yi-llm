import { useMemo } from "react";
import { create } from "zustand";
import { EMPTY_PROFILES } from "./selection";
import type { TerminalClient, TerminalDirectProfile, TerminalProfile, TerminalStatus } from "./types";

type ProxyDraftMap = Partial<Record<TerminalClient, TerminalProfile>>;
type DirectDraftMap = Partial<Record<TerminalClient, TerminalDirectProfile>>;

interface TerminalDraftStore {
  proxyOverrides: ProxyDraftMap;
  directOverrides: DirectDraftMap;
  setProxyDraft: (profile: TerminalProfile) => void;
  discardProxyDraft: (client: TerminalClient) => void;
  setDirectDraft: (profile: TerminalDirectProfile) => void;
  discardDirectDraft: (client: TerminalClient) => void;
}

/**
 * 终端草稿存放在全局 store 中：在 Codex / Claude Code / OpenCode 子页面之间切换时
 * 草稿不丢失，侧边栏也能据此显示每个终端的待应用标记。
 */
export const useTerminalDraftStore = create<TerminalDraftStore>((set) => ({
  proxyOverrides: {},
  directOverrides: {},
  setProxyDraft: (profile) =>
    set((state) => ({ proxyOverrides: { ...state.proxyOverrides, [profile.client]: profile } })),
  discardProxyDraft: (client) =>
    set((state) => {
      const next = { ...state.proxyOverrides };
      delete next[client];
      return { proxyOverrides: next };
    }),
  setDirectDraft: (profile) =>
    set((state) => ({ directOverrides: { ...state.directOverrides, [profile.client]: profile } })),
  discardDirectDraft: (client) =>
    set((state) => {
      const next = { ...state.directOverrides };
      delete next[client];
      return { directOverrides: next };
    }),
}));

/**
 * 每个终端客户端独立的代理草稿。草稿只记录用户改动，未改动时直接取自服务端状态，
 * 因此重新读取配置后无需再同步本地状态。
 */
export function useTerminalDrafts(statuses: readonly TerminalStatus[]) {
  const proxyOverrides = useTerminalDraftStore((state) => state.proxyOverrides);
  const setProxyDraft = useTerminalDraftStore((state) => state.setProxyDraft);
  const discardProxyDraft = useTerminalDraftStore((state) => state.discardProxyDraft);

  const changedClients = useMemo(
    () =>
      new Set(
        statuses
          .filter((item) => {
            const draft = proxyOverrides[item.client];
            return (
              draft !== undefined &&
              JSON.stringify(draft) !== JSON.stringify(item.profile ?? EMPTY_PROFILES[item.client])
            );
          })
          .map((item) => item.client),
      ),
    [statuses, proxyOverrides],
  );

  const draftOf = (client: TerminalClient): TerminalProfile =>
    proxyOverrides[client] ??
    statuses.find((item) => item.client === client)?.profile ??
    EMPTY_PROFILES[client];

  const setDraft = (profile: TerminalProfile) => setProxyDraft(profile);

  const discardDraft = (client: TerminalClient) => discardProxyDraft(client);

  return { changedClients, draftOf, setDraft, discardDraft };
}

/** 有未应用修改（代理或直连）的终端客户端集合；侧边栏分组用其显示待应用标记。 */
export function useTerminalPendingClients(statuses: readonly TerminalStatus[]) {
  const proxyOverrides = useTerminalDraftStore((state) => state.proxyOverrides);
  const directOverrides = useTerminalDraftStore((state) => state.directOverrides);
  return useMemo(
    () =>
      new Set(
        statuses
          .filter((item) => {
            const proxyDraft = proxyOverrides[item.client];
            const proxyPending =
              proxyDraft !== undefined &&
              JSON.stringify(proxyDraft) !== JSON.stringify(item.profile ?? EMPTY_PROFILES[item.client]);
            const directDraft = directOverrides[item.client];
            const directPending =
              directDraft !== undefined &&
              JSON.stringify(directDraft) !== JSON.stringify(item.direct_profile ?? null);
            return proxyPending || directPending;
          })
          .map((item) => item.client),
      ),
    [statuses, proxyOverrides, directOverrides],
  );
}

/** 直连草稿与脏标记，供终端直连区块读写。 */
export function useTerminalDirectDraft(client: TerminalClient, statuses: readonly TerminalStatus[]) {
  const directOverrides = useTerminalDraftStore((state) => state.directOverrides);
  const setDirectDraft = useTerminalDraftStore((state) => state.setDirectDraft);
  const discardDirectDraft = useTerminalDraftStore((state) => state.discardDirectDraft);
  const status = statuses.find((item) => item.client === client);
  const override = directOverrides[client];
  const draft = override ?? status?.direct_profile ?? null;
  const changed =
    override !== undefined && JSON.stringify(override) !== JSON.stringify(status?.direct_profile ?? null);
  return { directDraft: draft, directChanged: changed, setDirectDraft, discardDirectDraft };
}
