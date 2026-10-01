import { DEFAULT_SETTINGS } from "./defaults";
import { FONT_IDS, LANGUAGE_TAGS, THEME_MODES, type AppSettings, type FontId, type LanguageTag, type ThemeMode } from "./types";

/**
 * 设置持久化：localStorage + 版本号。
 * 后续结构变更时递增 SETTINGS_VERSION 并在 MIGRATIONS 中登记迁移函数，
 * 旧数据会自动升级到最新版本，避免破坏用户已有配置。
 */
const STORAGE_KEY = "yi-llm:settings";
/** 旧版本使用的存储键，仅在迁移历史设置时读取。 */
const LEGACY_STORAGE_KEY = "llm-man:settings";
const SETTINGS_VERSION = 1;

type Migration = (stored: Record<string, unknown>) => Record<string, unknown>;
const MIGRATIONS: Record<number, Migration> = {
  // 1: 初始版本，无需迁移。
};

interface StoredSettings {
  version: number;
  /** 磁盘数据不可信，读取时经 sanitize 逐项校验。 */
  settings: unknown;
}

function isThemeMode(value: unknown): value is ThemeMode {
  return typeof value === "string" && (THEME_MODES as readonly string[]).includes(value);
}
function isFontId(value: unknown): value is FontId {
  return typeof value === "string" && (FONT_IDS as readonly string[]).includes(value);
}
function isLanguageTag(value: unknown): value is LanguageTag {
  return typeof value === "string" && (LANGUAGE_TAGS as readonly string[]).includes(value);
}

/** 逐项校验并回落到默认值，保证返回值始终完整合法。 */
function sanitize(raw: Record<string, unknown>): AppSettings {
  return {
    theme: isThemeMode(raw.theme) ? raw.theme : DEFAULT_SETTINGS.theme,
    font: isFontId(raw.font) ? raw.font : DEFAULT_SETTINGS.font,
    language: isLanguageTag(raw.language) ? raw.language : DEFAULT_SETTINGS.language,
  };
}

export function loadSettings(): AppSettings {
  try {
    // 项目重命名后把旧键下的设置迁移到新键，避免用户丢失主题/字体/语言。
    if (!window.localStorage.getItem(STORAGE_KEY)) {
      const legacy = window.localStorage.getItem(LEGACY_STORAGE_KEY);
      if (legacy) window.localStorage.setItem(STORAGE_KEY, legacy);
    }
    const text = window.localStorage.getItem(STORAGE_KEY);
    if (!text) return { ...DEFAULT_SETTINGS };
    const stored = JSON.parse(text) as Partial<StoredSettings>;
    if (typeof stored !== "object" || stored === null || typeof stored.version !== "number") {
      return { ...DEFAULT_SETTINGS };
    }
    let data: Record<string, unknown> = typeof stored.settings === "object" && stored.settings !== null ? stored.settings as Record<string, unknown> : {};
    for (let version = stored.version; version < SETTINGS_VERSION; version += 1) {
      data = MIGRATIONS[version]?.(data) ?? data;
    }
    return sanitize(data);
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(settings: AppSettings): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify({ version: SETTINGS_VERSION, settings } as StoredSettings));
  } catch {
    // 存储不可用时静默失败，设置仅在本次会话内生效。
  }
}
