import { useMemo, useState } from "react";
import { EMPTY_PROFILES } from "./selection";
import type { TerminalClient, TerminalProfile, TerminalStatus } from "./types";

type DraftMap = Partial<Record<TerminalClient, TerminalProfile>>;

/**
 * 每个终端客户端独立的草稿。草稿只记录用户改动，未改动时直接取自服务端状态，
 * 因此重新读取配置后无需再同步本地状态。
 */
export function useTerminalDrafts(statuses: readonly TerminalStatus[]) {
  const [overrides, setOverrides] = useState<DraftMap>({});

  const changedClients = useMemo(() => new Set(statuses
    .filter((item) => {
      const draft = overrides[item.client];
      return draft !== undefined && JSON.stringify(draft) !== JSON.stringify(item.profile ?? EMPTY_PROFILES[item.client]);
    })
    .map((item) => item.client)), [statuses, overrides]);

  const draftOf = (client: TerminalClient): TerminalProfile => overrides[client] ?? statuses.find((item) => item.client === client)?.profile ?? EMPTY_PROFILES[client];

  const setDraft = (profile: TerminalProfile) => setOverrides((current) => ({ ...current, [profile.client]: profile }));

  const discardDraft = (client: TerminalClient) => setOverrides((current) => {
    const next = { ...current };
    delete next[client];
    return next;
  });

  return { changedClients, draftOf, setDraft, discardDraft };
}
