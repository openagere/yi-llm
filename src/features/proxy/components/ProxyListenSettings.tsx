import { useState } from "react";
import { LoaderCircle, Save } from "lucide-react";
import { describeError } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { proxyBaseUrl } from "@/shared/lib";
import { notify } from "@/shared/notify";
import { useProxySettings, useSaveProxySettings } from "../hooks";
import type { ServerSettings } from "../types";

const LOG_LEVELS: ServerSettings["log_level"][] = ["error", "warn", "info", "debug", "trace"];
const DEFAULT_SETTINGS: ServerSettings = { host: "127.0.0.1", port: 11435, log_level: "info" };

/**
 * 代理监听设置（监听地址 / 端口 / 日志级别）。
 * 属于代理运行配置，位于代理页内；通过 Tauri 命令读写，与本地偏好设置相互独立。
 */
export function ProxyListenSettings() {
  const { t } = useI18n();
  const query = useProxySettings();
  const save = useSaveProxySettings();
  const saved = query.data ?? null;
  const [draft, setDraft] = useState<ServerSettings | null>(null);
  const settings = draft ?? saved ?? DEFAULT_SETTINGS;
  const dirty = draft !== null && saved !== null && JSON.stringify(draft) !== JSON.stringify(saved);
  const busy = save.isPending;

  function update(patch: Partial<ServerSettings>) {
    setDraft({ ...settings, ...patch });
  }

  function submit() {
    if (busy || !dirty) return;
    save.mutate(settings, {
      onSuccess: () => {
        setDraft(null);
        notify(t("proxy.listen.savedToast"));
      },
    });
  }

  const notice = save.isError ? describeError(save.error) : save.isIdle && query.isError ? describeError(query.error) : "";
  const status = notice || (busy ? t("common.saving") : dirty ? t("proxy.listen.dirty") : saved ? t("proxy.listen.allSaved") : t("proxy.listen.loading"));

  return (
    <section id="proxy-listen-settings" className="editor-section" aria-label={t("proxy.listen.heading")}>
      <div className="section-heading">
        <div>
          <h2>{t("proxy.listen.heading")}</h2>
          <p>{t("proxy.listen.description")}</p>
        </div>
        <button type="button" className="secondary-button" disabled={busy || !dirty} onClick={submit}>
          {busy ? <LoaderCircle size={15} className="spinning" /> : <Save size={15} />}
          {busy ? t("common.saving") : t("proxy.listen.save")}
        </button>
      </div>
      <div className="proxy-address">{proxyBaseUrl(settings.host, settings.port)}</div>
      <fieldset disabled={busy || saved === null} className="connection-form-fields form-grid settings-grid">
        <label className="field"><span>{t("proxy.listen.host")}</span><input value={settings.host} onChange={(event) => update({ host: event.target.value })} placeholder="127.0.0.1" /></label>
        <label className="field"><span>{t("proxy.listen.port")}</span><input type="number" min={1} max={65535} value={settings.port} onChange={(event) => update({ port: Number(event.target.value) })} /></label>
        <label className="field"><span>{t("proxy.listen.logLevel")}</span><select value={settings.log_level} onChange={(event) => update({ log_level: event.target.value as ServerSettings["log_level"] })}>{LOG_LEVELS.map((level) => <option key={level} value={level}>{level}</option>)}</select></label>
      </fieldset>
      <p className="settings-note">{t("proxy.listen.note")}</p>
      <div className="settings-actions"><span className={`form-message ${save.isSuccess ? "success" : ""}`} role="status">{status}</span></div>
    </section>
  );
}
