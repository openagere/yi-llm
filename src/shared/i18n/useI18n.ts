import { useMemo } from "react";
import { useSettingsStore } from "../settings/store";
import type { LanguageTag } from "../settings/types";
import { createTranslator, type Translate } from "./translate";

export interface I18n {
  language: LanguageTag;
  setLanguage: (language: LanguageTag) => void;
  /** 按 "settings.theme.heading" 形式的键取文案，支持 {name} 插值。 */
  t: Translate;
}

export function useI18n(): I18n {
  const language = useSettingsStore((state) => state.settings.language);
  const update = useSettingsStore((state) => state.update);
  return useMemo(() => ({
    language,
    setLanguage: (next) => update("language", next),
    t: createTranslator(language),
  }), [language, update]);
}
