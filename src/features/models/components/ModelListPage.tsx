import { useMemo, useState } from "react";
import { Boxes, Copy, FileText, LoaderCircle, Pencil, Plus, RefreshCw, Search, Trash2, X } from "lucide-react";
import "@/styles/models.css";
import { describeError } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { PROTOCOLS, PROTOCOLS_BY_ID, type ProviderType } from "@/shared/lib";
import { useNavigation } from "@/shared/navigation";
import { notify } from "@/shared/notify";
import { ConfirmDialog, LIST_PAGE_SIZE, ListPagination, modelBrand, PageHeader, ProviderIcon } from "@/shared/ui";
import { tokenLabel } from "../capabilities";
import { duplicateModel } from "../draft";
import { useDeleteStandardModel, useModelCatalogInfo, useStandardModels } from "../hooks";
import { useModelSelection } from "../selection";
import type { StandardModel } from "../types";
import { ModelCapabilitySummary } from "./ModelCapabilitySummary";

const EMPTY_MODELS: StandardModel[] = [];

export function ModelListPage() {
  const { t } = useI18n();
  const { openEditor } = useNavigation();
  const modelsQuery = useStandardModels();
  const catalog = useModelCatalogInfo();
  const removeModel = useDeleteStandardModel();
  const select = useModelSelection((state) => state.select);
  const models = modelsQuery.data ?? EMPTY_MODELS;
  const loading = modelsQuery.isPending;
  const failed = modelsQuery.isError;
  const [search, setSearch] = useState("");
  const [protocol, setProtocol] = useState<ProviderType | "all">("all");
  const [page, setPage] = useState(1);
  const [deleting, setDeleting] = useState<StandardModel | null>(null);
  const [error, setError] = useState("");

  const visible = useMemo(() => models.filter((model) => (protocol === "all" || model.protocol === protocol)
    && `${model.name} ${PROTOCOLS_BY_ID[model.protocol].name}`.toLowerCase().includes(search.trim().toLowerCase())), [models, search, protocol]);
  const pageCount = Math.max(1, Math.ceil(visible.length / LIST_PAGE_SIZE));
  const currentPage = Math.min(page, pageCount);
  const pageItems = visible.slice((currentPage - 1) * LIST_PAGE_SIZE, currentPage * LIST_PAGE_SIZE);
  const catalogInfo = catalog.data;

  function edit(model: StandardModel | null) {
    openEditor("model-editor", () => select(model));
  }
  async function copyCatalogPath() {
    if (!catalogInfo) return;
    try {
      await navigator.clipboard.writeText(catalogInfo.path);
      notify(t("model.list.catalog.copied"));
    } catch (reason) {
      notify(t("model.list.catalog.copyFailed", { reason: describeError(reason) }), "error");
    }
  }
  function remove() {
    if (!deleting || removeModel.isPending) return;
    setError("");
    removeModel.mutate(deleting.id, {
      onSuccess: () => {
        notify(t("model.list.delete.done", { name: deleting.name }));
        setDeleting(null);
      },
      onError: (reason) => setError(describeError(reason)),
    });
  }
  const effortLabel = (model: StandardModel) => model.capabilities.effort.support === "supported"
    ? t("model.list.row.effortSupported")
    : model.capabilities.effort.support === "unsupported" ? t("model.list.row.effortUnsupported") : t("model.list.row.undeclared");

  return <section className="provider-list-page model-list-page">
    <PageHeader eyebrow={t("model.list.eyebrow")} title={t("model.list.title")} subtitle={t("pageDescriptions.models")} actions={
      <button type="button" className="icon-button" title={t("model.list.refresh")} aria-label={t("model.list.refresh")} disabled={loading || removeModel.isPending} onClick={() => void modelsQuery.refetch()}><RefreshCw size={17} className={loading ? "spinning" : ""} /></button>
    } primaryAction={<button type="button" className="primary-button" disabled={loading || failed || removeModel.isPending} onClick={() => edit(null)}><Plus size={16} />{t("model.list.addButton")}</button>} />
    {catalogInfo && <div className="model-catalog-source"><FileText size={15} /><span>{t("model.list.catalog.label")}</span><code title={catalogInfo.path}>{catalogInfo.path}</code><button type="button" className="icon-button" title={t("model.list.catalog.copy")} aria-label={t("model.list.catalog.copy")} onClick={() => void copyCatalogPath()}><Copy size={14} /></button></div>}
    <div className="connection-toolbar">
      <div className="connection-filters" role="group" aria-label={t("model.list.filter.label")}>{[{ id: "all" as const, shortName: t("model.list.filter.all") }, ...PROTOCOLS].map((item) => <button type="button" key={item.id} aria-pressed={protocol === item.id} className={protocol === item.id ? "selected" : ""} onClick={() => { setProtocol(item.id); setPage(1); }}>{item.shortName}<span>{item.id === "all" ? models.length : models.filter((model) => model.protocol === item.id).length}</span></button>)}</div>
      <div className="connection-search"><Search size={15} /><input type="search" aria-label={t("model.list.search.label")} placeholder={t("model.list.search.placeholder")} value={search} onChange={(event) => { setSearch(event.target.value); setPage(1); }} />{search && <button type="button" title={t("model.list.search.clearTitle")} aria-label={t("model.list.search.clearLabel")} onClick={() => { setSearch(""); setPage(1); }}><X size={14} /></button>}</div>
    </div>
    {loading ? <div className="connection-empty" role="status"><LoaderCircle className="spinning" size={24} />{t("model.list.loading")}</div> : failed ? <div className="connection-empty" role="alert"><h2>{t("model.list.failedTitle")}</h2><button type="button" className="secondary-button" onClick={() => void modelsQuery.refetch()}><RefreshCw size={15} />{t("common.retry")}</button></div> : <>
      <div className="standard-model-table">
        <div className="standard-model-head"><span>{t("model.list.columns.model")}</span><span>{t("model.list.columns.limits")}</span><span>Reasoning Effort</span><span>{t("model.list.columns.references")}</span><span>{t("model.list.columns.actions")}</span></div>
        {pageItems.map((model) => <article className="standard-model-row" key={model.id}>
          <div className="standard-model-identity"><button type="button" onClick={() => edit(model)} title={t("model.list.row.editTitle", { name: model.name })}><ProviderIcon brand={model.brand || modelBrand(model.name)} size={25} /><span>{model.name}</span></button><div className="standard-model-description"><span className="standard-model-protocol" title={PROTOCOLS_BY_ID[model.protocol].name}><ProviderIcon brand={PROTOCOLS_BY_ID[model.protocol].brand} size={15} />{PROTOCOLS_BY_ID[model.protocol].shortName}</span><ModelCapabilitySummary capabilities={{ ...model.capabilities, context_window: null, max_output_tokens: null, effort: { support: "unknown", levels: [], default: null } }} /></div></div>
          <div className="standard-model-limits"><span>{model.capabilities.context_window === null ? t("model.list.row.undeclared") : tokenLabel(model.capabilities.context_window)}</span><small>{model.capabilities.max_output_tokens === null ? t("model.list.row.undeclared") : tokenLabel(model.capabilities.max_output_tokens)}</small></div>
          <div className="standard-model-effort"><span>{effortLabel(model)}</span><small>{model.capabilities.effort.support === "supported" ? model.capabilities.effort.levels.join(" · ") : "—"}</small></div>
          <span className="standard-model-refs" title={t("model.list.row.referencesTitle", { count: model.provider_count })}>{model.provider_count}</span>
          <div className="standard-model-actions"><button type="button" className="icon-button" title={t("model.list.row.edit")} aria-label={t("model.list.row.editLabel", { name: model.name })} onClick={() => edit(model)}><Pencil size={15} /></button><button type="button" className="icon-button" title={t("model.list.row.duplicate")} aria-label={t("model.list.row.duplicateLabel", { name: model.name })} onClick={() => edit(duplicateModel(model, t("model.list.row.duplicateName", { name: model.name })))}><Copy size={15} /></button><button type="button" className="icon-button action-danger" disabled={model.provider_count > 0} title={model.provider_count ? t("model.list.row.removeBlocked") : t("model.list.row.remove")} aria-label={t("model.list.row.removeLabel", { name: model.name })} onClick={() => { setDeleting(model); setError(""); }}><Trash2 size={15} /></button></div>
        </article>)}
      </div>
      {!visible.length && <div className="connection-empty"><Boxes size={25} /><h2>{models.length ? t("model.list.empty.noMatch") : t("model.list.empty.none")}</h2>{!models.length && <button type="button" className="primary-button" onClick={() => edit(null)}><Plus size={15} />{t("model.list.addButton")}</button>}</div>}
      <ListPagination page={currentPage} count={visible.length} totalCount={models.length} unit={t("model.list.pagination.unit")} label={t("model.list.pagination.label")} onPage={setPage} disabled={loading || removeModel.isPending} />
    </>}
    {deleting && <ConfirmDialog destructive title={t("model.list.delete.title", { name: deleting.name })} description={t("model.list.delete.description")} confirmLabel={t("model.list.delete.confirm")} busy={removeModel.isPending} error={error} onCancel={() => setDeleting(null)} onConfirm={remove} />}
  </section>;
}
