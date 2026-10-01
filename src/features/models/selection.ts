import { create } from "zustand";
import type { StandardModel } from "./types";

interface ModelSelectionState {
  /** 正在编辑的标准模型；`null` 表示新增（复制场景为带新 id 的副本）。 */
  selectedModel: StandardModel | null;
  select: (model: StandardModel | null) => void;
}

export const useModelSelection = create<ModelSelectionState>((set) => ({
  selectedModel: null,
  select: (selectedModel) => set({ selectedModel }),
}));
