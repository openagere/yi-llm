import { useSettingsStore } from "./store";

export { DEFAULT_SETTINGS } from "./defaults";
export { loadSettings, saveSettings } from "./storage";
export { useSettingsStore } from "./store";
export { FONT_IDS, LANGUAGE_TAGS, THEME_MODES, type AppSettings, type FontId, type LanguageTag, type ThemeMode } from "./types";

export function useSettings() {
  const settings = useSettingsStore((state) => state.settings);
  const update = useSettingsStore((state) => state.update);
  const reset = useSettingsStore((state) => state.reset);
  return { settings, update, reset };
}
