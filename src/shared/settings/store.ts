import { create } from "zustand";
import { applyAppearance } from "../theme/applyAppearance";
import { DEFAULT_SETTINGS } from "./defaults";
import { loadSettings, saveSettings } from "./storage";
import type { AppSettings } from "./types";

interface SettingsState {
  settings: AppSettings;
  /** 更新单个设置项；变更会立即持久化并应用到外观。 */
  update: <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => void;
  /** 恢复出厂默认设置。 */
  reset: () => void;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  settings: loadSettings(),
  update: (key, value) => set((state) => ({ settings: { ...state.settings, [key]: value } })),
  reset: () => set({ settings: { ...DEFAULT_SETTINGS } }),
}));

// 模块级订阅：无需 Provider，非 React 代码（如 QueryCache 回调）也能读到最新设置。
applyAppearance(useSettingsStore.getState().settings);
useSettingsStore.subscribe((state, previous) => {
  if (state.settings === previous.settings) return;
  applyAppearance(state.settings);
  saveSettings(state.settings);
});
