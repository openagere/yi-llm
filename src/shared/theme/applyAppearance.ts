import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppSettings } from "../settings/types";
import { FONT_STACKS } from "./fonts";

export type ResolvedTheme = "light" | "dark";

const darkSchemeQuery = "(prefers-color-scheme: dark)";
const systemDark = typeof window !== "undefined" && typeof window.matchMedia === "function" ? window.matchMedia(darkSchemeQuery) : null;

let releaseSystemWatcher: (() => void) | null = null;

function applyWindowTheme(theme: ResolvedTheme) {
  if (isTauri()) void getCurrentWindow().setTheme(theme).catch(error => console.warn("Unable to synchronize native window theme", error));
}

export function resolveThemeMode(mode: AppSettings["theme"]): ResolvedTheme {
  if (mode !== "system") return mode;
  return systemDark?.matches ? "dark" : "light";
}

/**
 * 将设置应用到文档根节点：
 * - data-theme 驱动 CSS 变量切换（light / dark，system 时跟随系统并监听变化）
 * - --font-sans 覆盖 Tailwind 主题变量以切换界面字体
 * - lang 属性跟随界面语言（便于浏览器与辅助技术识别）
 */
export function applyAppearance(settings: AppSettings): void {
  const root = document.documentElement;
  root.dataset.theme = resolveThemeMode(settings.theme);
  applyWindowTheme(resolveThemeMode(settings.theme));
  root.style.setProperty("--font-sans", FONT_STACKS[settings.font]);
  root.lang = settings.language;

  releaseSystemWatcher?.();
  releaseSystemWatcher = null;
  if (settings.theme === "system" && systemDark) {
    const onChange = () => {
      root.dataset.theme = resolveThemeMode("system");
      applyWindowTheme(resolveThemeMode("system"));
    };
    systemDark.addEventListener("change", onChange);
    releaseSystemWatcher = () => systemDark.removeEventListener("change", onChange);
  }
}
