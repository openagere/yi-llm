import { useEffect, useState, type MouseEvent } from "react";
import { Copy, Minus, Moon, PanelLeft, Square, Sun, X } from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useI18n } from "@/shared/i18n";
import { useNavigationStore } from "@/shared/navigation";
import { notify } from "@/shared/notify";
import { useSettings } from "@/shared/settings";
import { AppLogo } from "@/shared/ui";

export function WorkspaceToolbar() {
  const { t } = useI18n();
  const { update } = useSettings();
  const disabled = useNavigationStore((state) => state.busy);
  const collapsed = useNavigationStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useNavigationStore((state) => state.toggleSidebar);
  const guarded = useNavigationStore((state) => state.guarded);
  const native = isTauri();
  const [maximized, setMaximized] = useState(false);
  const title = t("nav.workspace");
  useEffect(() => {
    document.title = title;
    if (native) void getCurrentWindow().setTitle(title).catch(error => console.warn("Unable to synchronize window title", error));
  }, [native, title]);

  useEffect(() => {
    if (!native) return;
    let active = true;
    const sync = () => { void getCurrentWindow().isMaximized().then(value => { if (active) setMaximized(value); }).catch(() => {}); };
    sync();
    window.addEventListener("resize", sync);
    return () => { active = false; window.removeEventListener("resize", sync); };
  }, [native]);

  async function control(action: "minimize" | "maximize" | "drag") {
    if (!native) return;
    try {
      const appWindow = getCurrentWindow();
      if (action === "minimize") await appWindow.minimize();
      else if (action === "drag") await appWindow.startDragging();
      else { await appWindow.toggleMaximize(); setMaximized(await appWindow.isMaximized()); }
    } catch (error) { notify(`${t("window.failed")}${String(error)}`, "error"); }
  }
  function closeWindow() {
    if (!native) return;
    guarded(() => { void getCurrentWindow().close().catch(error => notify(`${t("window.failed")}${String(error)}`, "error")); });
  }
  function interactive(event: MouseEvent) {
    return event.target instanceof Element && !!event.target.closest("[data-window-interactive]");
  }
  return <header className={`workspace-toolbar ${native ? "native-titlebar" : ""}`} aria-label={t("window.titlebar")}
    onMouseDown={event => { if (event.button === 0 && event.detail === 1 && !interactive(event)) void control("drag"); }}
    onDoubleClick={event => { if (event.button === 0 && !interactive(event)) void control("maximize"); }}>
    <div className="workspace-toolbar-navigation">
      <span className="titlebar-logo-slot"><AppLogo className="titlebar-logo" alt="" /></span>
      <button type="button" className="icon-button" data-window-interactive title={t("nav.toggleSidebar")} aria-label={t("nav.toggleSidebar")} aria-controls="workspace-sidebar" aria-expanded={!collapsed} onClick={toggleSidebar}><PanelLeft size={15} /></button>
    </div>
    <div className="titlebar-caption" data-titlebar-drag-region />
    <div className="titlebar-actions" data-window-interactive>
      <button type="button" className="icon-button theme-toggle" title={t("nav.toggleTheme")} aria-label={t("nav.toggleTheme")} onClick={() => update("theme", document.documentElement.dataset.theme === "dark" ? "light" : "dark")}><Moon className="theme-icon-dark" size={15} /><Sun className="theme-icon-light" size={15} /></button>
      {native && <div className="window-controls">
        <button type="button" className="window-control" title={t("window.minimize")} aria-label={t("window.minimize")} onClick={() => void control("minimize")}><Minus size={14} strokeWidth={1.4} /></button>
        <button type="button" className="window-control" title={t(maximized ? "window.restore" : "window.maximize")} aria-label={t(maximized ? "window.restore" : "window.maximize")} onClick={() => void control("maximize")}>{maximized ? <Copy size={12} strokeWidth={1.4} /> : <Square size={12} strokeWidth={1.4} />}</button>
        <button type="button" className="window-control window-control-close" title={t("window.close")} aria-label={t("window.close")} disabled={disabled} onClick={closeWindow}><X size={15} strokeWidth={1.4} /></button>
      </div>}
    </div>
  </header>;
}
