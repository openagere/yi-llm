import { useState } from "react";
import { Activity, Boxes, ChartNoAxesColumnIncreasing, Search, Settings2, Terminal, Workflow, X } from "lucide-react";
import { useStandardModels } from "@/features/models";
import { useProviders } from "@/features/providers";
import { useProxyPhase } from "@/features/proxy";
import { useI18n } from "@/shared/i18n";
import { useNavigationStore } from "@/shared/navigation";
import { AppLogo } from "@/shared/ui";

const NAV_GROUPS = [["provider-list", "model-list"], ["proxy", "usage", "terminal"]] as const;
type NavId = (typeof NAV_GROUPS)[number][number];
const NAV_ICONS: Record<NavId, typeof Workflow> = {
  "provider-list": Workflow, "model-list": Boxes, proxy: Activity,
  usage: ChartNoAxesColumnIncreasing, terminal: Terminal,
};

export function Sidebar() {
  const { t } = useI18n();
  const page = useNavigationStore((state) => state.page);
  const navigate = useNavigationStore((state) => state.navigate);
  const providerCount = useProviders().data?.length ?? 0;
  const modelCount = useStandardModels().data?.length ?? 0;
  const proxyPhase = useProxyPhase();
  const [search, setSearch] = useState("");
  const navLabels: Record<NavId | "settings", string> = {
    "provider-list": t("nav.providers"), "model-list": t("nav.models"),
    proxy: t("nav.proxy"), usage: t("nav.usage"), terminal: t("nav.terminal"), settings: t("nav.settings"),
  };
  const phaseLabel = proxyPhase ? t(`nav.phase.${proxyPhase}`) : t("nav.phase.unknown");
  const query = search.trim().toLocaleLowerCase();
  const matches = (id: NavId | "settings") => `${navLabels[id]} ${id}`.toLocaleLowerCase().includes(query);
  const noResults = ![...NAV_GROUPS.flat(), "settings" as const].some(matches);

  return <aside className="sidebar" id="workspace-sidebar" aria-label={t("nav.workspace")}>
    <div className="brand"><AppLogo alt="" /></div>
    <div className="sidebar-search">
      <Search size={16} aria-hidden="true" />
      <input type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("nav.search")} aria-label={t("nav.search")} />
      {search && <button type="button" title={t("nav.clearSearch")} aria-label={t("nav.clearSearch")} onClick={() => setSearch("")}><X size={14} /></button>}
    </div>
    <nav className="primary-nav" aria-label={t("nav.workspace")}>
      {NAV_GROUPS.map((ids, index) => {
        const visible = ids.filter(matches);
        if (!visible.length) return null;
        return <div className="nav-group" key={index}>
          <div className="side-section-label">{t(index === 0 ? "nav.configuration" : "nav.tools")}</div>
          {visible.map((id) => {
            const Icon = NAV_ICONS[id];
            const selected = page === id || (id === "provider-list" && page === "provider-editor") || (id === "model-list" && page === "model-editor");
            return <button key={id} type="button" className={`provider-nav ${selected ? "selected" : ""}`} title={navLabels[id]} aria-label={navLabels[id]} aria-current={selected ? "page" : undefined} onClick={() => navigate(id)}>
              <Icon size={18} strokeWidth={1.7} aria-hidden="true" />
              <span className="provider-nav-name">{navLabels[id]}</span>
              {id === "provider-list" && <span className="nav-count">{providerCount}</span>}
              {id === "model-list" && <span className="nav-count">{modelCount}</span>}
              {id === "proxy" && <span className={`proxy-nav-dot ${proxyPhase ?? "unknown"}`} title={phaseLabel} />}
            </button>;
          })}
        </div>;
      })}
      {noResults && <p className="sidebar-no-results" role="status">{t("nav.noResults")}</p>}
    </nav>
    <div className="sidebar-bottom">
      {matches("settings") && <button type="button" className={`provider-nav ${page === "settings" ? "selected" : ""}`} title={t("nav.settings")} aria-label={t("nav.settings")} aria-current={page === "settings" ? "page" : undefined} onClick={() => navigate("settings")}><Settings2 size={18} strokeWidth={1.7} aria-hidden="true" /><span className="provider-nav-name">{t("nav.settings")}</span></button>}
      <button type="button" className="side-footer proxy-side-status" title={`${t("nav.proxy")} · ${phaseLabel}`} onClick={() => navigate("proxy")}><span className={`proxy-status-dot ${proxyPhase ?? "unknown"}`} /> <span>{t("nav.proxy")} · {phaseLabel}</span></button>
    </div>
  </aside>;
}
