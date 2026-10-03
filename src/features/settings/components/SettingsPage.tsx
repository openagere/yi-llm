import { Check, Languages, Monitor, Moon, Sun, Type } from "lucide-react";
import type { ReactNode } from "react";
import "@/styles/settings.css";
import { useI18n } from "@/shared/i18n";
import { useNavigation } from "@/shared/navigation";
import { useSettings } from "@/shared/settings";
import { FONT_STACKS } from "@/shared/theme";
import { PageHeader } from "@/shared/ui";

interface OptionCardProps {
  selected: boolean;
  onSelect: () => void;
  title: ReactNode;
  description: string;
  preview?: ReactNode;
  variant?: "light" | "dark" | "system";
}

/** 设置页通用选项卡片：主题、字体、语言共用同一交互模式。 */
function OptionCard({ selected, onSelect, title, description, preview, variant }: OptionCardProps) {
  return (
    <button type="button" className={`preference-option ${variant ? `preference-option-${variant}` : ""} ${selected ? "selected" : ""}`} aria-pressed={selected} onClick={onSelect}>
      <span className="preference-option-head">{title}</span>
      <span className="preference-option-desc">{description}</span>
      {preview && <span className="preference-option-preview">{preview}</span>}
      {selected && <span className="preference-check" aria-hidden="true"><Check size={12} strokeWidth={3} /></span>}
    </button>
  );
}

export function SettingsPage() {
  const { t } = useI18n();
  const { back, close } = useNavigation();
  const { settings, update } = useSettings();

  const themeOptions = [
    { id: "light", icon: Sun, label: t("settings.theme.light"), desc: t("settings.theme.lightDesc") },
    { id: "dark", icon: Moon, label: t("settings.theme.dark"), desc: t("settings.theme.darkDesc") },
    { id: "system", icon: Monitor, label: t("settings.theme.system"), desc: t("settings.theme.systemDesc") },
  ] as const;

  const fontOptions = [
    { id: "system", label: t("settings.font.system"), desc: t("settings.font.systemDesc") },
    { id: "serif", label: t("settings.font.serif"), desc: t("settings.font.serifDesc") },
    { id: "mono", label: t("settings.font.mono"), desc: t("settings.font.monoDesc") },
  ] as const;

  const languageOptions = [
    { id: "zh-CN", label: t("settings.language.zh-CN") },
    { id: "en-US", label: t("settings.language.en-US") },
  ] as const;

  return (
    <section className="settings-page">
      <PageHeader navigationInToolbar eyebrow={t("settings.eyebrow")} title={t("settings.title")} subtitle={t("pageDescriptions.settings")} onBack={back} onClose={close} backLabel={t("common.back")} closeLabel={t("common.close")} />
      <div className="settings-section">
        <section className="editor-section" aria-label={t("settings.theme.heading")}>
          <div className="section-heading">
            <div>
              <h2>{t("settings.theme.heading")}</h2>
              <p>{t("settings.theme.description")}</p>
            </div>
          </div>
          <div className="preference-options" role="group" aria-label={t("settings.theme.heading")}>
            {themeOptions.map(({ id, icon: Icon, label, desc }) => (
              <OptionCard key={id} variant={id} selected={settings.theme === id} onSelect={() => update("theme", id)} title={<><Icon size={16} strokeWidth={1.9} />{label}</>} description={desc} />
            ))}
          </div>
        </section>

        <section className="editor-section" aria-label={t("settings.font.heading")}>
          <div className="section-heading">
            <div>
              <h2>{t("settings.font.heading")}</h2>
              <p>{t("settings.font.description")}</p>
            </div>
          </div>
          <div className="preference-options" role="group" aria-label={t("settings.font.heading")}>
            {fontOptions.map(({ id, label, desc }) => (
              <OptionCard
                key={id}
                selected={settings.font === id}
                onSelect={() => update("font", id)}
                title={<><Type size={16} strokeWidth={1.9} />{label}</>}
                description={desc}
                preview={<span style={{ fontFamily: FONT_STACKS[id] }}>{t("settings.font.preview")}</span>}
              />
            ))}
          </div>
        </section>

        <section className="editor-section" aria-label={t("settings.language.heading")}>
          <div className="section-heading">
            <div>
              <h2>{t("settings.language.heading")}</h2>
              <p>{t("settings.language.description")}</p>
            </div>
          </div>
          <div className="preference-options" role="group" aria-label={t("settings.language.heading")}>
            {languageOptions.map(({ id, label }) => (
              <OptionCard key={id} selected={settings.language === id} onSelect={() => update("language", id)} title={<><Languages size={16} strokeWidth={1.9} />{label}</>} description={id} />
            ))}
          </div>
        </section>

        <p className="preference-status">{t("settings.autoSaved")}</p>
      </div>
    </section>
  );
}
