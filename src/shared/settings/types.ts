/** 本地应用设置：纯前端偏好，不依赖 Tauri/代理运行时。 */
export const THEME_MODES = ["light", "dark", "system"] as const;
export type ThemeMode = (typeof THEME_MODES)[number];

export const FONT_IDS = ["system", "serif", "mono"] as const;
export type FontId = (typeof FONT_IDS)[number];

export const LANGUAGE_TAGS = ["zh-CN", "en-US"] as const;
export type LanguageTag = (typeof LANGUAGE_TAGS)[number];

export interface AppSettings {
  theme: ThemeMode;
  font: FontId;
  language: LanguageTag;
}
