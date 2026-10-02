import { useMemo, useState } from "react";
import { Boxes, Cable, Check, LoaderCircle, LockKeyhole, Minus, Pencil, Plus, Power, RefreshCw, Search, Star, Trash2, X } from "lucide-react";
import { describeError } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { PROTOCOLS, PROTOCOLS_BY_ID, type ProviderType } from "@/shared/lib";
import { useNavigation } from "@/shared/navigation";
import { notify } from "@/shared/notify";
import { ConfirmDialog, LIST_PAGE_SIZE, ListPagination, PageHeader, ProviderIcon } from "@/shared/ui";
import { useDeleteProvider, useProviders, useSaveProvider } from "../hooks";
import { useProviderSelection } from "../selection";
import type { ProviderView } from "../types";

const EMPTY_PROVIDERS: ProviderView[] = [];

export function ProviderListPage() {
  const { t } = useI18n();
  const { openEditor } = useNavigation();
  const providersQuery = useProviders();
  const saveProvider = useSaveProvider();
  const deleteProvider = useDeleteProvider();
  const select = useProviderSelection((state) => state.select);
  const providers = providersQuery.data ?? EMPTY_PROVIDERS;
  const loading = providersQuery.isPending;
  const loadFailed = providersQuery.isError;
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ProviderView | null>(null);
  const [deleteError, setDeleteError] = useState("");
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<"all" | "enabled" | "disabled">("all");
  const [page, setPage] = useState(1);
  const pending = pendingId !== null;

  const counts = { all: providers.length, enabled: providers.filter((item) => item.enabled).length, disabled: providers.filter((item) => !item.enabled).length };
  const visible = useMemo(() => {
    const query = search.trim().toLowerCase();
    return providers.filter((provider) => (filter === "all" || provider.enabled === (filter === "enabled")) &&
      `${provider.name} ${provider.short_code} ${provider.base_url} ${PROTOCOLS_BY_ID[provider.provider_type].name} ${provider.models.map((model) => model.route_id).join(" ")}`.toLowerCase().includes(query));
  }, [providers, search, filter]);
  const pageCount = Math.max(1, Math.ceil(visible.length / LIST_PAGE_SIZE));
  const currentPage = Math.min(page, pageCount);
  const pageItems = visible.slice((currentPage - 1) * LIST_PAGE_SIZE, currentPage * LIST_PAGE_SIZE);

  function edit(id: string | null) {
    openEditor("provider-editor", () => select(id));
  }
  function persist(provider: ProviderView, message: string) {
    if (pendingId) return;
    setPendingId(provider.id);
    saveProvider.mutate(provider, {
      onSuccess: () => notify(message),
      onError: (error) => {
        if (deleteTarget?.id === provider.id) setDeleteError(t("provider.list.delete.failed", { reason: describeError(error) }));
        else notify(describeError(error), "error");
      },
      onSettled: () => setPendingId(null),
    });
  }
  function remove() {
    if (!deleteTarget || pendingId) return;
    setDeleteError("");
    setPendingId(deleteTarget.id);
    const target = deleteTarget;
    deleteProvider.mutate(target.id, {
      onSuccess: () => {
        notify(t("provider.list.delete.done", { name: target.name || "Provider" }));
        setDeleteTarget(null);
      },
      onError: (error) => setDeleteError(t("provider.list.delete.failed", { reason: describeError(error) })),
      onSettled: () => setPendingId(null),
    });
  }

  return (
    <section className="provider-list-page">
      <PageHeader eyebrow={t("provider.list.eyebrow")} title="Providers" subtitle={t("pageDescriptions.providers")} actions={
        <button type="button" className="icon-button refresh-providers" title={t("provider.list.refresh")} aria-label={t("provider.list.refresh")} disabled={loading || pending} onClick={() => void providersQuery.refetch()}><RefreshCw size={17} className={loading ? "spinning" : ""} /></button>
      } primaryAction={<button type="button" className="outline-action-button add-provider-button" disabled={loading || loadFailed || pending} onClick={() => edit(null)}><Plus size={17} />{t("provider.list.add")}</button>} />
      <div className="connection-toolbar">
        <div className="connection-filters" role="group" aria-label={t("provider.list.filterLabel")}>
          {(["all", "enabled", "disabled"] as const).map((id) => <button key={id} type="button" aria-pressed={filter === id} className={filter === id ? "selected" : ""} onClick={() => { setFilter(id); setPage(1); }}>{t(`provider.list.filter.${id}`)}<span>{counts[id]}</span></button>)}
        </div>
        <div className="connection-search"><Search size={15} /><input type="search" aria-label={t("provider.list.searchLabel")} placeholder={t("provider.list.searchPlaceholder")} value={search} onChange={(event) => { setSearch(event.target.value); setPage(1); }} />{search && <button type="button" title={t("provider.list.clearSearch")} aria-label={t("provider.list.clearSearch")} onClick={() => { setSearch(""); setPage(1); }}><X size={14} /></button>}</div>
      </div>
      {loading && providers.length === 0 ? <div className="connection-empty"><LoaderCircle className="spinning" size={25} /><h2>{t("provider.list.loading")}</h2></div> : loadFailed ? <div className="connection-empty"><Cable size={30} /><h2>{t("provider.list.failedTitle")}</h2><button type="button" className="secondary-button" disabled={loading} onClick={() => void providersQuery.refetch()}><RefreshCw size={15} />{t("provider.list.reload")}</button></div> : providers.length === 0 ? <div className="connection-empty"><Cable size={32} /><h2>{t("provider.list.emptyTitle")}</h2><button type="button" className="primary-button" onClick={() => edit(null)}><Plus size={16} />{t("provider.list.add")}</button></div> : <>
        <div className="connection-table" aria-busy={loading}>
          <div className="connection-table-heading"><span>{t("provider.list.columns.connection")}</span><span>{t("provider.list.columns.protocols")}</span><span><Power size={13} />{t("provider.list.columns.enabled")}</span><span>{t("provider.list.columns.actions")}</span></div>
          <div role="list" aria-label={t("provider.list.listLabel")}>
            {pageItems.map((provider) => {
              const busy = loading || pendingId === provider.id;
              return <article className={`connection-row ${provider.enabled ? "" : "is-disabled"}`} key={provider.id} role="listitem" aria-busy={busy}>
                <div className="connection-identity">
                  <div className="connection-name"><span className="connection-brand-mark"><ProviderIcon provider={provider} size={24} /></span><button type="button" disabled={busy || pending} title={t("provider.list.editTitle", { name: provider.name })} onClick={() => edit(provider.id)}>{provider.name || t("provider.list.unnamed")}</button>{provider.is_default && <span className="connection-default" title={t("provider.list.defaultProvider")}><Star size={11} fill="currentColor" /></span>}<span className="connection-alias" title={provider.short_code}>{provider.short_code}</span><span className="connection-meta"><span><Boxes size={12} />{provider.models.length ? t("provider.list.modelCount", { count: provider.models.length }) : provider.is_default ? t("provider.list.defaultRoute") : t("provider.list.noModels")}</span>{busy && <LoaderCircle size={12} className="spinning" />}</span></div>
                  <span className="connection-url" title={provider.base_url}>{provider.base_url}</span>
                </div>
                <div className="connection-protocols">
                  {PROTOCOLS.map(({ id, name, brand, shortName }) => {
                    const native = provider.provider_type === id;
                    const enabled = native || provider.protocol_support[id];
                    const className = `connection-protocol protocol-${id} ${enabled ? "active" : ""} ${native ? "native" : ""}`;
                    const content = <><span className="connection-protocol-mark" aria-hidden="true"><ProviderIcon brand={brand} size={14} /></span><span className="connection-protocol-copy">{shortName}</span><span className="connection-protocol-state" aria-hidden="true">{native ? <LockKeyhole size={10} strokeWidth={1.6} /> : enabled ? <Check size={10} strokeWidth={1.8} /> : <Minus size={10} strokeWidth={1.6} />}</span></>;
                    return native
                      ? <span key={id} className={className} title={t("provider.list.nativeTitle", { name })} aria-label={t("provider.list.nativeLabel", { name })}>{content}</span>
                      : <button key={id} type="button" className={className} aria-pressed={enabled} disabled={busy} title={t("provider.list.protocolTitle", { name, action: enabled ? t("provider.list.protocolOff") : t("provider.list.protocolOn") })} aria-label={t("provider.list.protocolLabel", { name: provider.name, shortName })} onClick={() => persist({ ...provider, protocol_support: { ...provider.protocol_support, [id]: !provider.protocol_support[id as ProviderType] } }, t("provider.list.toggled", { name: provider.name, shortName, state: enabled ? t("provider.list.stateOff") : t("provider.list.stateOn") }))}>{content}</button>;
                  })}
                </div>
                <label className="toggle-row connection-enabled" title={provider.enabled ? t("provider.list.disable") : t("provider.list.enable")}><input type="checkbox" checked={provider.enabled} disabled={busy} aria-label={t("provider.list.enableLabel", { name: provider.name })} onChange={() => persist({ ...provider, enabled: !provider.enabled }, t("provider.list.enabledState", { name: provider.name, state: provider.enabled ? t("provider.list.filter.disabled") : t("provider.list.filter.enabled") }))} /><span className="toggle" /><span>{provider.enabled ? t("provider.list.filter.enabled") : t("provider.list.filter.disabled")}</span></label>
                <div className="connection-actions">
                  <button type="button" className="icon-button" title={t("provider.list.edit")} aria-label={t("provider.list.editTitle", { name: provider.name })} disabled={busy || pending} onClick={() => edit(provider.id)}><Pencil size={16} /></button>
                  <button type="button" className="icon-button action-danger" title={t("provider.list.remove")} aria-label={t("provider.list.removeLabel", { name: provider.name })} disabled={busy} onClick={() => { setDeleteError(""); setDeleteTarget(provider); }}><Trash2 size={16} /></button>
                </div>
              </article>;
            })}
          </div>
          {visible.length === 0 && <div className="connection-no-results"><Search size={23} /><h2>{t("provider.list.noMatch")}</h2><button type="button" className="text-button" onClick={() => { setSearch(""); setFilter("all"); setPage(1); }}>{t("provider.list.clearFilters")}</button></div>}
        </div>
        <ListPagination page={currentPage} count={visible.length} totalCount={providers.length} unit={t("provider.list.paginationUnit")} label={t("provider.list.paginationLabel")} onPage={setPage} disabled={loading || pending}><span><Power size={12} />{t("provider.list.enabledCount", { count: counts.enabled })}</span></ListPagination>
      </>}
      {deleteTarget && <ConfirmDialog destructive title={t("provider.list.delete.title", { name: deleteTarget.name || t("provider.list.unnamed") })} description={t("provider.list.delete.description", { count: deleteTarget.models.length, defaultNote: deleteTarget.is_default ? t("provider.list.delete.defaultNote") : "" })} confirmLabel={t("provider.list.delete.confirm")} error={deleteError} busy={pendingId === deleteTarget.id} onConfirm={remove} onCancel={() => setDeleteTarget(null)} />}
    </section>
  );
}
