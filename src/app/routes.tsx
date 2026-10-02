import { lazy, Suspense, type ComponentType } from "react";
import { useI18n } from "@/shared/i18n";
import { useNavigationStore, type Page } from "@/shared/navigation";

function lazyPage<K extends string>(loader: () => Promise<Record<K, ComponentType>>, name: K) {
  return lazy(() => loader().then((module) => ({ default: module[name] })));
}

const ProviderListPage = lazyPage(() => import("@/features/providers/pages"), "ProviderListPage");
const ProviderEditorPage = lazyPage(() => import("@/features/providers/pages"), "ProviderEditorPage");
const ModelListPage = lazyPage(() => import("@/features/models/pages"), "ModelListPage");
const ModelEditorPage = lazyPage(() => import("@/features/models/pages"), "ModelEditorPage");
const UsagePage = lazyPage(() => import("@/features/usage/pages"), "UsagePage");
const TerminalPage = lazyPage(() => import("@/features/terminal/pages"), "TerminalPage");
const SettingsPage = lazyPage(() => import("@/features/settings/pages"), "SettingsPage");
const ProxyPage = lazy(() => import("@/features/proxy/pages").then((module) => ({ default: module.ProxyPage })));

const PAGES: Record<Exclude<Page, "proxy">, ComponentType> = {
  "provider-list": ProviderListPage,
  "provider-editor": ProviderEditorPage,
  "model-list": ModelListPage,
  "model-editor": ModelEditorPage,
  usage: UsagePage,
  "terminal-codex": TerminalPage,
  "terminal-claude-code": TerminalPage,
  "terminal-opencode": TerminalPage,
  settings: SettingsPage,
};

function PageFallback() {
  const { t } = useI18n();
  return <div className="page-fallback" role="status">{t("common.opening")}</div>;
}

/** 当前页面出口。代理页首次访问后保持挂载（仅隐藏），以保留监控/日志的本地状态与轮询上下文。 */
export function PageOutlet() {
  const page = useNavigationStore((state) => state.page);
  const proxyVisited = useNavigationStore((state) => state.proxyVisited);
  const Current = page === "proxy" ? null : PAGES[page];
  return <>
    {Current && <Suspense fallback={<PageFallback />}><Current /></Suspense>}
    {proxyVisited && <div className="proxy-page-host" hidden={page !== "proxy"}>
      <Suspense fallback={<PageFallback />}><ProxyPage active={page === "proxy"} /></Suspense>
    </div>}
  </>;
}
