import { useEffect, useRef, useState } from "react";
import { Check, Clipboard, Link2 } from "lucide-react";
import { useProviders } from "@/features/providers";
import { useI18n } from "@/shared/i18n";
import { PROTOCOLS, proxyOrigin } from "@/shared/lib";
import { notify } from "@/shared/notify";
import { ProviderIcon } from "@/shared/ui";
import type { ServerSettings } from "../types";

const PATHS = { responses: "/v1/responses", openai_chat: "/v1/chat/completions", anthropic: "/v1/messages" };

export function ProxyConnections({ settings }: { settings: ServerSettings }) {
  const { t } = useI18n();
  const providers = useProviders();
  const routesReady = !providers.isPending && !providers.isError;
  const enabled = (providers.data ?? []).filter((provider) => provider.enabled);
  const [copied, setCopied] = useState("");
  const copyTimer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(copyTimer.current), []);
  const root = proxyOrigin(settings.host, settings.port);

  async function copy(url: string, label: string) {
    try {
      await navigator.clipboard.writeText(url);
      window.clearTimeout(copyTimer.current);
      setCopied(label);
      copyTimer.current = window.setTimeout(() => setCopied(""), 1800);
      notify(t("proxy.connections.addressCopied"));
    } catch {
      notify(t("proxy.connections.clipboardFailed"), "error");
    }
  }

  function address(url: string, label: string) {
    const copyLabel = t("proxy.connections.copy", { label });
    return <div className="proxy-url-value"><code>{url}</code><button type="button" className="icon-button proxy-copy" title={copied === label ? t("proxy.connections.copied") : copyLabel} aria-label={copyLabel} onClick={() => void copy(url, label)}>{copied === label ? <Check size={15} /> : <Clipboard size={15} />}</button></div>;
  }

  return <section className="proxy-connections" aria-label={t("proxy.connections.title")}>
    <div className="proxy-section-title"><Link2 size={16} /><h2>{t("proxy.connections.title")}</h2></div>
    <div className="proxy-url-table">
      <div className="proxy-url-table-head"><span>{t("proxy.connections.protocol")}</span><span>Base URL</span><span>{t("proxy.connections.requestUrl")}</span><span>{t("proxy.connections.routes")}</span></div>
      {PROTOCOLS.map((item) => <div className="proxy-url-row" key={item.id}>
        <strong className="proxy-url-protocol"><ProviderIcon brand={item.brand} size={18} /><span>{item.name}</span></strong>
        <div><small>Base URL</small>{address(item.id === "anthropic" ? root : `${root}/v1`, `${item.shortName} Base URL`)}</div>
        <div><small>POST</small>{address(`${root}${PATHS[item.id]}`, t("proxy.connections.requestUrlLabel", { name: item.shortName }))}</div>
        <span>{routesReady ? enabled.filter((provider) => provider.protocol_support[item.id]).length : "--"} <small>{t("proxy.facts.providersCount")}</small></span>
      </div>)}
    </div>
    <div className="proxy-url-secondary"><div><span>{t("proxy.connections.modelList")}</span>{address(`${root}/v1/models`, t("proxy.connections.modelListLabel"))}</div><div><span>{t("proxy.connections.health")}</span>{address(`${root}/health`, t("proxy.connections.healthLabel"))}</div></div>
  </section>;
}
