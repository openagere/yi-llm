import { create } from "zustand";

interface ProviderSelectionState {
  /** 正在编辑的 Provider id；`null` 表示新增。 */
  selectedId: string | null;
  select: (id: string | null) => void;
}

export const useProviderSelection = create<ProviderSelectionState>((set) => ({
  selectedId: null,
  select: (selectedId) => set({ selectedId }),
}));
