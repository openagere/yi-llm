import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Check, ChevronDown, ChevronRight, ChevronsDownUp, ChevronsUpDown, Clipboard, ExternalLink, LoaderCircle, Search, Trash2, X } from "lucide-react";
import "@/styles/terminal.css";
import { ModelCapabilitySummary } from "@/features/models";
import { useProviders, type ProviderView } from "@/features/providers";
import { describeError } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { PROTOCOLS_BY_ID } from "@/shared/lib";
import { useEditorGuard, useNavigation } from "@/shared/navigation";
import { PageHeader, ProviderIcon, Select } from "@/shared/ui";
import { useApplyTerminalConfig, useTerminalPreview, useTerminalProfiles } from "../hooks";
import { TERMINALS, availableSelectionKeys, buildProfile, compatibleProviders, filterProviders, groupKey, providerSelections, selectionKey } from "../selection";
import type { TerminalClient, TerminalSelection } from "../types";
import { useTerminalDrafts } from "../useTerminalDrafts";

const EMPTY_PROVIDERS: ProviderView[] = [];

export function TerminalPage() {
  const { t } = useI18n();
  const { back, close } = useNavigation();
  const providers = useProviders();
  const profiles = useTerminalProfiles();
  const apply = useApplyTerminalConfig();
  const [client, setClient] = useState<TerminalClient>("codex");
  const { changedClients, draftOf, setDraft } = useTerminalDrafts(profiles.data ?? []);
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [feedback, setFeedback] = useState<{ ok: boolean; text: string; backups: string[] } | null>(null);
  const [fileState, setFileState] = useState({ key: "", index: 0 });
  const [copied, setCopied] = useState(false);
  const terminal = TERMINALS.find((item) => item.id === client)!;
  const profile = draftOf(client);
  const profileKey = JSON.stringify(profile);
  const status = profiles.data?.find((item) => item.client === client);
  const providerRows = providers.data ?? EMPTY_PROVIDERS;
  const compatible = useMemo(() => compatibleProviders(providerRows, terminal.protocol), [providerRows, terminal.protocol]);
  const available = useMemo(() => availableSelectionKeys(compatible), [compatible]);
  const stale = profile.models.filter((model) => !available.has(selectionKey(model)));
  const selected = new Set(profile.models.map(selectionKey));
  const ready = !profiles.isPending && !profiles.isError && !providers.isPending && !providers.isError;
  const previewEnabled = ready && profile.models.length > 0 && stale.length === 0 && Boolean(profile.default_model);
  const preview = useTerminalPreview(profile, previewEnabled);
  const fileIndex = fileState.key === profileKey ? fileState.index : 0;
  const previewFile = preview.preview?.files[fileIndex];
  const query = search.trim().toLowerCase();
  const filtered = filterProviders(compatible, query);
  const matchingModels = filtered.flatMap((provider) => providerSelections(provider));
  const busy = apply.isPending;
  useEditorGuard(changedClients.size > 0, busy);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1800);
    return () => window.clearTimeout(timer);
  }, [copied]);

  function chooseClient(next: TerminalClient) {
    setClient(next);
    setSearch("");
    setFeedback(null);
  }
  function update(models: TerminalSelection[], defaultModel = profile.default_model) {
    setDraft(buildProfile(client, models, defaultModel));
    setFeedback(null);
  }
  function toggle(models: TerminalSelection[], enabled: boolean) {
    const changed = new Set(models.map(selectionKey));
    update(enabled ? [...profile.models, ...models.filter((model) => !selected.has(selectionKey(model)))] : profile.models.filter((model) => !changed.has(selectionKey(model))));
  }
  function applyConfig() {
    if (busy || !preview.preview || stale.length || !ready) return;
    setFeedback(null);
    apply.mutate(profile, {
      onSuccess: (applied) => setFeedback({ ok: true, text: t("terminal.feedback.applied", { name: terminal.name, count: applied.model_count }), backups: applied.backup_paths }),
      onError: (error) => setFeedback({ ok: false, text: describeError(error), backups: [] }),
    });
  }
  async function copyPreview() {
    if (!previewFile) return;
    try {
      await navigator.clipboard.writeText(previewFile.content);
      setCopied(true);
    } catch {
      setFeedback({ ok: false, text: t("terminal.feedback.clipboardUnavailable"), backups: [] });
    }
  }

  return <section className="terminal-page">
    <PageHeader navigationInToolbar eyebrow={t("terminal.header.eyebrow")} title={t("terminal.header.title")} subtitle={t("pageDescriptions.terminal")} onBack={back} onClose={close} closeLabel={t("terminal.header.closeLabel")} disabled={busy} />
    <div className="terminal-client-tabs" role="tablist" aria-label={t("terminal.tabs.label")} onKeyDown={(event) => {
      if (busy || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const index = TERMINALS.findIndex((item) => item.id === client);
      const next = event.key === "Home" ? 0 : event.key === "End" ? TERMINALS.length - 1 : (index + (event.key === "ArrowRight" ? 1 : TERMINALS.length - 1)) % TERMINALS.length;
      chooseClient(TERMINALS[next].id);
      document.getElementById(`terminal-${TERMINALS[next].id}`)?.focus();
    }}>{TERMINALS.map((item) => {
      const itemStatus = profiles.data?.find((entry) => entry.client === item.id);
      const pending = changedClients.has(item.id);
      return <button type="button" key={item.id} id={`terminal-${item.id}`} role="tab" tabIndex={item.id === client ? 0 : -1} aria-selected={item.id === client} aria-controls="terminal-panel" title={pending ? t("terminal.tabs.pendingTitle", { name: item.name }) : itemStatus?.active ? t("terminal.tabs.connectedTitle", { name: item.name }) : t("terminal.tabs.disconnectedTitle", { name: item.name })} disabled={busy} onClick={() => chooseClient(item.id)}><ProviderIcon brand={item.brand} size={18} />{item.name}<span aria-hidden="true" className={`terminal-tab-dot ${pending ? "pending" : itemStatus?.active ? "connected" : ""}`} /></button>;
    })}</div>
    {profiles.isError && <div className="terminal-load-error" role="alert"><AlertTriangle size={15} /><span>{t("terminal.loadError.message")}{describeError(profiles.error)}</span><button type="button" className="text-button" onClick={() => void profiles.refetch()}>{t("common.retry")}</button></div>}
    <div id="terminal-panel" role="tabpanel" aria-labelledby={`terminal-${client}`}>
      <div className="terminal-status-row"><div className={`integration-status ${status?.active ? "connected" : "disconnected"}`}><span className="integration-status-dot" />{profiles.isPending ? t("terminal.status.loading") : status?.active ? t("terminal.status.connected") : t("terminal.status.disconnected")}</div><span className="integration-path" title={status?.config_path}>{t("terminal.status.configFile")}<code>{status?.config_path ?? "--"}</code></span></div>
      <div className="terminal-layout">
        <section className="terminal-config-section">
          <div className="integration-title"><span className="integration-icon"><ProviderIcon brand={terminal.brand} size={24} /></span><div><h2>{terminal.name}</h2><p className="terminal-protocol-label"><ProviderIcon brand={PROTOCOLS_BY_ID[terminal.protocol].brand} size={12} />{PROTOCOLS_BY_ID[terminal.protocol].name}{client === "claude-code" && <span className="terminal-min-version" title={t("terminal.client.minVersionTitle")}> ≥ 2.1.242</span>}</p></div></div>
          <fieldset disabled={busy || !ready} className="terminal-model-fields">
            <div className="terminal-model-heading"><h3>{t("terminal.models.heading")}<span>{profile.models.length}</span></h3><div><button type="button" className="icon-button" title={t("terminal.models.expandAll")} aria-label={t("terminal.models.expandAll")} disabled={Boolean(query) || !compatible.length} onClick={() => setExpanded((current) => ({ ...current, ...Object.fromEntries(compatible.map((provider) => [groupKey(client, provider.id), true])) }))}><ChevronsUpDown size={14} /></button><button type="button" className="icon-button" title={t("terminal.models.collapseAll")} aria-label={t("terminal.models.collapseAll")} disabled={Boolean(query) || !compatible.length} onClick={() => setExpanded((current) => ({ ...current, ...Object.fromEntries(compatible.map((provider) => [groupKey(client, provider.id), false])) }))}><ChevronsDownUp size={14} /></button><button type="button" className="text-button" disabled={!matchingModels.length} onClick={() => toggle(matchingModels, true)}>{query ? t("terminal.models.selectMatches") : t("terminal.models.selectAll")}</button><button type="button" className="text-button" disabled={!profile.models.length} onClick={() => update([])}>{t("terminal.models.clear")}</button></div></div>
            <div className="terminal-model-search"><Search size={15} /><input type="search" aria-label={t("terminal.models.searchLabel")} placeholder={t("terminal.models.searchPlaceholder")} value={search} onChange={(event) => setSearch(event.target.value)} />{search && <button type="button" title={t("terminal.models.clearSearchTitle")} aria-label={t("terminal.models.clearSearchLabel")} onClick={() => setSearch("")}><X size={14} /></button>}</div>
            <div className="terminal-model-list">
              {filtered.map((provider) => <ProviderGroup key={provider.id} provider={provider} client={client} protocol={terminal.protocol} query={query} open={Boolean(query) || (expanded[groupKey(client, provider.id)] ?? profile.models.some((model) => model.provider_id === provider.id))} checkedCount={providerSelections(provider).filter((model) => selected.has(selectionKey(model))).length} selected={selected} defaultModel={profile.default_model} onToggle={toggle} onOpen={(open) => setExpanded((current) => ({ ...current, [groupKey(client, provider.id)]: open }))} />)}
              {!filtered.length && <div className="terminal-model-empty">{!ready ? profiles.isPending || providers.isPending ? t("terminal.models.loading") : t("terminal.models.unavailable") : compatible.length ? t("terminal.models.noMatch") : t("terminal.models.noneEnabled")}</div>}
            </div>
            {stale.length > 0 && <div className="terminal-stale-models"><strong><AlertTriangle size={14} />{t("terminal.stale.heading")}</strong>{stale.map((model) => <div key={selectionKey(model)}><span>{model.model}</span><button type="button" className="icon-button action-danger" title={t("terminal.stale.removeTitle")} aria-label={t("terminal.stale.removeLabel", { model: model.model })} onClick={() => update(profile.models.filter((item) => selectionKey(item) !== selectionKey(model)))}><Trash2 size={14} /></button></div>)}</div>}
            <label className="field terminal-default-model"><span>{t("terminal.defaultModel.label")}</span><Select value={profile.default_model} onValueChange={(model) => update(profile.models, model)} options={profile.models.filter((model) => available.has(selectionKey(model))).map((model) => ({ value: model.model, label: model.model }))} label={t("terminal.defaultModel.select")} placeholder={t("terminal.defaultModel.placeholder")} disabled={!profile.models.length || busy || !ready} /></label>
          </fieldset>
          {feedback && <div className={`terminal-message ${feedback.ok ? "success" : "error"}`} role="status">{feedback.ok ? <Check size={15} /> : <AlertTriangle size={15} />}{feedback.text}</div>}
          {!!feedback?.backups.length && <details className="terminal-backups"><summary>{t("terminal.feedback.backups", { count: feedback.backups.length })}</summary>{feedback.backups.map((path) => <code key={path}>{path}</code>)}</details>}
          <div className="terminal-actions"><span>{changedClients.has(client) ? t("terminal.actions.dirty") : t("terminal.actions.summary", { models: profile.models.length, providers: new Set(profile.models.map((model) => model.provider_id)).size })}</span><button type="button" className="primary-button" onClick={applyConfig} disabled={busy || !ready || !preview.preview || preview.pending || stale.length > 0 || Boolean(preview.error)}>{busy ? <LoaderCircle size={15} className="spinning" /> : <ExternalLink size={15} />}{busy ? t("terminal.actions.applying") : status?.active ? t("terminal.actions.update") : t("terminal.actions.apply")}</button></div>
        </section>
        <section className="terminal-preview-section">
          <div className="terminal-preview-heading"><div><h2>{t("terminal.preview.heading")}</h2><p>{preview.preview?.endpoint ?? PROTOCOLS_BY_ID[terminal.protocol].name}</p></div><button type="button" className="icon-button copy-config" onClick={() => void copyPreview()} title={t("terminal.preview.copyTitle")} aria-label={t("terminal.preview.copyLabel")} disabled={!previewFile || preview.pending}>{copied ? <Check size={16} /> : <Clipboard size={16} />}</button></div>
          {preview.preview && preview.preview.files.length > 1 && <div className="terminal-file-tabs" role="group" aria-label={t("terminal.preview.filesLabel")}>{preview.preview.files.map((file, index) => <button type="button" key={file.path} aria-pressed={fileIndex === index} onClick={() => { setFileState({ key: profileKey, index }); setCopied(false); }}>{file.path.split(/[\\/]/).pop()}</button>)}</div>}
          <pre className="terminal-preview"><code>{preview.pending ? t("terminal.preview.generating") : preview.error || status?.config_error || previewFile?.content || (stale.length ? t("terminal.preview.removeStale") : t("terminal.preview.empty"))}</code></pre>
        </section>
      </div>
    </div>
  </section>;
}

function ProviderGroup({ provider, client, protocol, query, open, checkedCount, selected, defaultModel, onToggle, onOpen }: {
  provider: ProviderView; client: TerminalClient; protocol: TerminalDefinitionProtocol; query: string; open: boolean; checkedCount: number;
  selected: Set<string>; defaultModel: string; onToggle: (models: TerminalSelection[], enabled: boolean) => void; onOpen: (open: boolean) => void;
}) {
  const { t } = useI18n();
  const selections = providerSelections(provider);
  const contentId = `terminal-models-${client}-${encodeURIComponent(provider.id)}`;
  return <div className="terminal-provider-group">
    <div className="terminal-provider-heading"><input type="checkbox" ref={(node) => { if (node) node.indeterminate = checkedCount > 0 && checkedCount < selections.length; }} checked={checkedCount === selections.length} onChange={(event) => onToggle(selections, event.target.checked)} aria-label={t("terminal.group.select", { name: provider.name })} /><button type="button" className="terminal-provider-disclosure" disabled={Boolean(query)} aria-expanded={open} aria-controls={contentId} aria-label={t(open ? "terminal.group.collapseLabel" : "terminal.group.expandLabel", { name: provider.name })} title={t(open ? "terminal.group.collapseTitle" : "terminal.group.expandTitle")} onClick={() => onOpen(!open)}><ProviderIcon provider={provider} size={17} /><strong>{provider.name}</strong><span>{checkedCount} / {selections.length}</span>{open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</button></div>
    <div id={contentId} hidden={!open}>{provider.models.map((model) => <label className="terminal-model-option" key={model.route_id}><input type="checkbox" checked={selected.has(selectionKey({ provider_id: provider.id, model: model.route_id }))} onChange={(event) => onToggle([{ provider_id: provider.id, model: model.route_id }], event.target.checked)} aria-label={model.route_id} /><span><strong>{model.route_id}</strong><ModelCapabilitySummary capabilities={model.capabilities} client={protocol} upstream={provider.provider_type} inputFilter={client === "codex" ? ["text", "image"] : undefined} /></span>{defaultModel === model.route_id && <em>{t("terminal.models.defaultBadge")}</em>}</label>)}</div>
  </div>;
}

type TerminalDefinitionProtocol = (typeof TERMINALS)[number]["protocol"];
