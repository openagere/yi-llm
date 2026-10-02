import { useState } from "react";
import { Activity, Boxes, ChartNoAxesColumnIncreasing, ChevronDown, ChevronRight, Search, Settings2, Terminal, Workflow, X } from "lucide-react";
import { useStandardModels } from "@/features/models";
import { useProviders } from "@/features/providers";
import { useProxyPhase } from "@/features/proxy";
import { useTerminalProfiles } from "@/features/terminal/hooks";
import { TERMINALS } from "@/features/terminal/selection";
import { useTerminalPendingClients } from "@/features/terminal/useTerminalDrafts";
import { useI18n } from "@/shared/i18n";
import { TERMINAL_PAGE_CLIENTS, useNavigationStore, type TerminalPageId } from "@/shared/navigation";
import { AppLogo, ProviderIcon } from "@/shared/ui";

const NAV_GROUPS = [["provider-list", "model-list"], ["proxy", "usage"]] as const;
type NavId = (typeof NAV_GROUPS)[number][number];
const NAV_ICONS: Record<NavId, typeof Workflow> = {
  "provider-list": Workflow, "model-list": Boxes, proxy: Activity, usage: ChartNoAxesColumnIncreasing,
};
const TERMINAL_PAGES = TERMINALS.map((item) => ({ ...item, page: `terminal-${item.id}` as TerminalPageId }));

export function Sidebar() {
  const { t } = useI18n();
  const page = useNavigationStore((state) => state.page);
  const navigate = useNavigationStore((state) => state.navigate);
  const providerCount = useProviders().data?.length ?? 0;
  const modelCount = useStandardModels().data?.length ?? 0;
  const proxyPhase = useProxyPhase();
  const terminalProfiles = useTerminalProfiles();
  const pendingClients = useTerminalPendingClients(terminalProfiles.data ?? []);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [search, setSearch] = useState("");
  const navLabels: Record<NavId | "terminal" | "settings", string> = {
    "provider-list": t("nav.providers"), "model-list": t("nav.models"),
    proxy: t("nav.proxy"), usage: t("nav.usage"), terminal: t("nav.terminal"), settings: t("nav.settings"),
  };
  const phaseLabel = proxyPhase ? t(`nav.phase.${proxyPhase}`) : t("nav.phase.unknown");
  const query = search.trim().toLocaleLowerCase();
  const matches = (id: NavId | "terminal" | "settings") => `${navLabels[id]} ${id}`.toLocaleLowerCase().includes(query);
  const terminalGroupVisible = matches("terminal") || TERMINAL_PAGES.some((item) => item.name.toLocaleLowerCase().includes(query));
  const visibleTerminals = TERMINAL_PAGES.filter((item) => matches("terminal") || !query || item.name.toLocaleLowerCase().includes(query));
  const activeTerminal = (TERMINAL_PAGES.find((item) => item.page === page) ?? null)?.page ?? null;
  const hasOtherMatches = [...NAV_GROUPS.flat(), "settings" as const].some(matches);
  const noResults = !hasOtherMatches && !terminalGroupVisible;

  const [lastActiveTerminal, setLastActiveTerminal] = useState(activeTerminal);
  if (activeTerminal && activeTerminal !== lastActiveTerminal) {
    setLastActiveTerminal(activeTerminal);
    setTerminalOpen(true);
  }

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
      {terminalGroupVisible && <div className="nav-group nav-terminal-group">
        <button type="button" className={`provider-nav nav-terminal-toggle ${activeTerminal ? "selected" : ""}`} title={navLabels.terminal} aria-label={navLabels.terminal} aria-expanded={terminalOpen} aria-controls="nav-terminal-items" onClick={() => setTerminalOpen((open) => !open)}>
          <Terminal size={18} strokeWidth={1.7} aria-hidden="true" />
          <span className="provider-nav-name">{navLabels.terminal}</span>
          {pendingClients.size > 0 && <span className="nav-terminal-dot pending" aria-hidden="true" />}
          {terminalOpen ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronRight size={14} aria-hidden="true" />}
        </button>
        {terminalOpen && <div className="nav-terminal-items" id="nav-terminal-items">
          {visibleTerminals.map((item) => {
            const selected = page === item.page;
            const itemStatus = terminalProfiles.data?.find((entry) => entry.client === TERMINAL_PAGE_CLIENTS[item.page]);
            const dotClass = pendingClients.has(TERMINAL_PAGE_CLIENTS[item.page]) ? "pending" : itemStatus?.active ? "connected" : "";
            return <button key={item.id} type="button" className={`provider-nav nav-terminal-item ${selected ? "selected" : ""}`} title={item.name} aria-label={item.name} aria-current={selected ? "page" : undefined} onClick={() => navigate(item.page)}>
              <ProviderIcon brand={item.brand} size={16} />
              <span className="provider-nav-name">{item.name}</span>
              <span className={`nav-terminal-dot ${dotClass}`} aria-hidden="true" />
            </button>;
          })}
        </div>}
      </div>}
      {noResults && <p className="sidebar-no-results" role="status">{t("nav.noResults")}</p>}
    </nav>
    <div className="sidebar-bottom">
      {matches("settings") && <button type="button" className={`provider-nav ${page === "settings" ? "selected" : ""}`} title={t("nav.settings")} aria-label={t("nav.settings")} aria-current={page === "settings" ? "page" : undefined} onClick={() => navigate("settings")}><Settings2 size={18} strokeWidth={1.7} aria-hidden="true" /><span className="provider-nav-name">{t("nav.settings")}</span></button>}
      <button type="button" className="side-footer proxy-side-status" title={`${t("nav.proxy")} · ${phaseLabel}`} onClick={() => navigate("proxy")}><span className={`proxy-status-dot ${proxyPhase ?? "unknown"}`} /> <span>{t("nav.proxy")} · {phaseLabel}</span></button>
    </div>
  </aside>;
}