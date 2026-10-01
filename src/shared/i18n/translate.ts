import { useSettingsStore } from "../settings/store";
import type { LanguageTag } from "../settings/types";
import enUS from "./locales/en-US";
import zhCN from "./locales/zh-CN";
import type { MessageKey, MessageParams, Messages } from "./types";

export const DICTIONARIES: Record<LanguageTag, Messages> = { "zh-CN": zhCN, "en-US": enUS };

function lookup(messages: Messages, key: string): string {
  let node: unknown = messages;
  for (const part of key.split(".")) {
    if (typeof node !== "object" || node === null) return key;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string" ? node : key;
}

function interpolate(template: string, params: MessageParams): string {
  return template.replace(/\{(\w+)\}/g, (raw, name: string) => (name in params ? String(params[name]) : raw));
}

export type Translate = (key: MessageKey, params?: MessageParams) => string;

export function createTranslator(language: LanguageTag): Translate {
  const messages = DICTIONARIES[language];
  return (key, params) => {
    const template = lookup(messages, key);
    return params ? interpolate(template, params) : template;
  };
}

/** 当前界面语言；供非 React 代码（QueryCache、mutation 回调、Intl 格式化）读取。 */
export function currentLanguage(): LanguageTag {
  return useSettingsStore.getState().settings.language;
}

/** 非 React 场景的翻译入口；组件内请使用 useI18n().t 以便语言切换时重渲染。 */
export function translate(key: MessageKey, params?: MessageParams): string {
  return createTranslator(currentLanguage())(key, params);
}
