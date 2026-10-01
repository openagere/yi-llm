import { useMemo, useRef, useState } from "react";
import { ArrowRightLeft, Boxes, Check, Eye, EyeOff, FlaskConical, LoaderCircle, Plus, Save, SlidersHorizontal, Trash2 } from "lucide-react";
import { describeError } from "@/shared/api";
import { useStandardModels, ModelCapabilitySummary, tokenLabel, type StandardModel } from "@/features/models";
import { useI18n, type Translate } from "@/shared/i18n";
import { PROTOCOLS, type ProviderType } from "@/shared/lib";
import { useEditorGuard, useNavigation } from "@/shared/navigation";
import { notify } from "@/shared/notify";
import { modelBrand, PageHeader, ProtocolSelector, ProviderIcon, Select } from "@/shared/ui";
import { providerApi } from "../api";
import { useProviders, useSaveProvider } from "../hooks";
import { useProviderSelection } from "../selection";
import type { ProviderView } from "../types";

const TYPE_URLS: Record<ProviderType, string> = {
  anthropic: "https://api.anthropic.com",
  openai_chat: "https://api.openai.com/v1",
  responses: "https://api.openai.com/v1",
};

function blankProvider(isDefault: boolean): ProviderView {
  return {
    id: crypto.randomUUID(), provider_type: "anthropic", name: "", short_code: "", base_url: TYPE_URLS.anthropic, api_key: "",
    enabled: true, thinking: "medium", extra: {}, is_default: isDefault, models: [],
    protocol_support: { anthropic: true, openai_chat: false, responses: false },
  };
}

function modelDescription(model: StandardModel, t: Translate) {
  const contextWindow = model.capabilities.context_window;
  return contextWindow ? `Context ${tokenLabel(contextWindow)}` : t("provider.editor.models.undeclared");
}

function ProviderEditorForm({ provider, providers, standardModels }: { provider: ProviderView | null; providers: ProviderView[]; standardModels: StandardModel[] }) {
  const { t } = useI18n();
  const { back, goTo } = useNavigation();
  const saveProvider = useSaveProvider();
  const [initial] = useState(() => provider ? structuredClone(provider) : blankProvider(providers.length === 0));
  const [draft, setDraft] = useState<ProviderView>(() => structuredClone(initial));
  const [extraText, setExtraText] = useState(() => JSON.stringify(initial.extra, null, 2));
  const [showKey, setShowKey] = useState(false);
  const [testModel, setTestModel] = useState("");
  const [busy, setBusy] = useState<"save" | "test" | null>(null);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const form = useRef<HTMLFormElement>(null);
  const dirty = useMemo(() => JSON.stringify(draft) !== JSON.stringify(initial) || extraText !== JSON.stringify(initial.extra, null, 2), [draft, initial, extraText]);
  const currentDefault = providers.find((item) => item.is_default && item.id !== draft.id);
  useEditorGuard(dirty, busy !== null);

  function update<K extends keyof ProviderView>(key: K, value: ProviderView[K]) {
    setDraft((current) => ({ ...current, [key]: value }));
    setFieldErrors((current) => ({ ...current, [key]: "" }));
  }
  function chooseUpstreamType(type: ProviderType) {
    setDraft((current) => ({ ...current, provider_type: type,
      base_url: current.base_url === TYPE_URLS[current.provider_type] ? TYPE_URLS[type] : current.base_url,
      protocol_support: { ...current.protocol_support, [type]: true },
      models: current.models.map((model) => ({ ...model, standard_model_id: standardModels.find((standard) => standard.id === model.standard_model_id)?.protocol === type ? model.standard_model_id : null })),
    }));
  }
  const matchingStandards = standardModels.filter((model) => model.protocol === draft.provider_type);
  function parsedExtra() {
    const extra: unknown = JSON.parse(extraText.trim() || "{}");
    if (!extra || Array.isArray(extra) || typeof extra !== "object") throw new Error(t("provider.editor.errors.headersObject"));
    if (Object.values(extra).some((value) => typeof value !== "string")) throw new Error(t("provider.editor.errors.headersString"));
    return extra as Record<string, string>;
  }
  function cleanModels() {
    return draft.models.filter((model) => model.name.trim() && model.standard_model_id)
      .map((model) => ({ ...model, provider_id: draft.id, route_id: `${model.name.trim()}(${draft.short_code.trim()})`, name: model.name.trim() }));
  }
  function focusInvalid() {
    window.requestAnimationFrame(() => {
      const invalid = form.current?.querySelector<HTMLElement>('[aria-invalid="true"]');
      invalid?.scrollIntoView({ block: "center", behavior: "smooth" });
      invalid?.focus({ preventScroll: true });
    });
  }
  function validate(forSave: boolean) {
    const errors: Record<string, string> = {};
    if (forSave && !draft.name.trim()) errors.name = t("provider.editor.errors.nameRequired");
    if (forSave && !/^[a-z0-9_-]{1,24}$/.test(draft.short_code.trim())) errors.short_code = t("provider.editor.errors.aliasFormat");
    else if (forSave && providers.some((item) => item.id !== draft.id && item.short_code === draft.short_code.trim())) errors.short_code = t("provider.editor.errors.aliasDuplicate");
    try {
      const url = new URL(draft.base_url.trim());
      if (!["http:", "https:"].includes(url.protocol) || url.search || url.hash || url.username || url.password) throw new Error("invalid");
    } catch { errors.base_url = t("provider.editor.errors.baseUrl"); }
    try { parsedExtra(); }
    catch (error) { errors.extra = error instanceof SyntaxError ? t("provider.editor.errors.headersJson") : error instanceof Error ? error.message : String(error); }
    if (forSave) {
      const models = cleanModels();
      if (draft.models.some((model) => !model.name.trim() || !matchingStandards.some((standard) => standard.id === model.standard_model_id))) errors.models = t("provider.editor.errors.modelsIncomplete");
      else if (new Set(models.map((model) => model.name)).size !== models.length) errors.models = t("provider.editor.errors.modelsDuplicate");
      else if (!draft.is_default && models.length === 0) errors.models = t("provider.editor.errors.modelsRequired");
    }
    setFieldErrors(errors);
    if (Object.keys(errors).length) { notify(Object.values(errors)[0], "error"); focusInvalid(); return false; }
    return true;
  }
  function payload() {
    return { ...draft, name: draft.name.trim(), short_code: draft.short_code.trim(), base_url: draft.base_url.trim().replace(/\/+$/, ""), extra: parsedExtra(),
      protocol_support: { ...draft.protocol_support, [draft.provider_type]: true }, models: cleanModels() };
  }
  function save() {
    if (busy || !validate(true)) return;
    setBusy("save");
    const next = payload();
    const creating = provider === null;
    saveProvider.mutate(next, {
      onSuccess: (result) => {
        const warnings = result.terminal_config_warnings.join("; ");
        const message = t(creating ? "provider.editor.added" : "provider.editor.saved", { name: next.name });
        goTo("provider-list");
        notify(warnings ? `${message}; ${warnings}` : message, warnings ? "info" : "success");
      },
      onError: (error) => notify(t("provider.editor.saveFailed", { reason: describeError(error) }), "error"),
      onSettled: () => setBusy(null),
    });
  }
  async function testConnection() {
    if (busy || !validate(false)) return;
    const model = testModel.trim() || cleanModels()[0]?.name || "";
    if (!model) {
      setFieldErrors((current) => ({ ...current, test_model: t("provider.editor.test.missing") }));
      notify(t("provider.editor.test.missingDetail"), "error");
      form.current?.querySelector<HTMLInputElement>('[name="test_model"]')?.focus();
      return;
    }
    setBusy("test");
    try {
      const next = payload();
      const response = await providerApi.test(next, next.models, model);
      notify(t("provider.editor.test.success", { response }));
    } catch (error) {
      notify(t("provider.editor.test.failed", { reason: describeError(error) }), "error");
    } finally { setBusy(null); }
  }
  const footer = busy === "test" ? t("provider.editor.footer.testing") : busy === "save" ? t("provider.editor.footer.saving") : dirty ? t("provider.editor.footer.unsaved") : provider ? t("provider.editor.footer.saved") : t("provider.editor.footer.fresh");
  const submitLabel = busy === "save" ? t("provider.editor.saving") : provider ? t("provider.editor.saveChanges") : t("provider.editor.titleAdd");

  return (
    <section className="editor-page connection-editor-page">
      <PageHeader eyebrow={provider ? provider.name : t("provider.editor.eyebrowNew")} title={provider ? t("provider.editor.titleEdit") : t("provider.editor.titleAdd")} onBack={back} backLabel={t("provider.editor.back")} disabled={Boolean(busy)} primaryAction={
        <button type="submit" form="provider-form" className="primary-button" disabled={Boolean(busy) || Boolean(provider && !dirty)}>{busy === "save" ? <LoaderCircle className="spinning" size={16} /> : provider ? <Save size={16} /> : <Plus size={16} />}{submitLabel}</button>
      } />
      <form ref={form} id="provider-form" noValidate onSubmit={(event) => { event.preventDefault(); save(); }}>
        <fieldset disabled={Boolean(busy)} className="connection-form-fields provider-editor-grid">
          <section className="connection-form-section">
            <div className="connection-section-title"><ProviderIcon provider={draft} size={18} /><h2>{t("provider.editor.info")}</h2></div>
            <div className="connection-form-grid">
              <label className="field connection-name-field"><span>{t("provider.editor.name")}<span className="required-mark">*</span></span><input autoFocus name="name" value={draft.name} aria-invalid={Boolean(fieldErrors.name)} onChange={(event) => update("name", event.target.value)} placeholder={t("provider.editor.namePlaceholder")} />{fieldErrors.name && <small className="field-error">{fieldErrors.name}</small>}</label>
              <label className="field connection-short-code-field"><span>{t("provider.editor.alias")}<span className="required-mark">*</span></span><input name="short_code" value={draft.short_code} maxLength={24} aria-invalid={Boolean(fieldErrors.short_code)} onChange={(event) => update("short_code", event.target.value.toLowerCase())} placeholder={t("provider.editor.aliasPlaceholder")} autoComplete="off" spellCheck={false} />{fieldErrors.short_code && <small className="field-error">{fieldErrors.short_code}</small>}</label>
              <div className="field connection-upstream-field"><span>{t("provider.editor.upstream")}</span><ProtocolSelector value={draft.provider_type} onChange={chooseUpstreamType} label={t("provider.editor.upstream")} disabled={Boolean(busy)} /></div>
              <label className="field full"><span>Base URL <span className="required-mark">*</span></span><input name="base_url" value={draft.base_url} aria-invalid={Boolean(fieldErrors.base_url)} onChange={(event) => update("base_url", event.target.value)} placeholder={TYPE_URLS[draft.provider_type]} spellCheck={false} />{fieldErrors.base_url && <small className="field-error">{fieldErrors.base_url}</small>}</label>
              <label className="field full"><span>API Key / Token <small className="optional-label">{t("provider.editor.optional")}</small></span><span className="input-action"><input type={showKey ? "text" : "password"} value={draft.api_key} onChange={(event) => update("api_key", event.target.value)} placeholder={t("provider.editor.apiKey")} autoComplete="new-password" spellCheck={false} /><button type="button" className="secret-toggle" title={showKey ? t("provider.editor.hideKey") : t("provider.editor.showKey")} aria-label={showKey ? t("provider.editor.hideKey") : t("provider.editor.showKey")} onClick={() => setShowKey((value) => !value)}>{showKey ? <EyeOff size={16} /> : <Eye size={16} />}</button></span></label>
            </div>
            <div className="connection-options"><label className="toggle-row"><input type="checkbox" checked={draft.enabled} onChange={(event) => update("enabled", event.target.checked)} /><span className="toggle" /><span>{t("provider.editor.enable")}</span></label><label className="connection-default-choice"><input type="checkbox" checked={draft.is_default} onChange={(event) => { update("is_default", event.target.checked); setFieldErrors((current) => ({ ...current, models: "" })); }} /><span>{t("provider.editor.defaultProvider")}</span></label>{draft.is_default && currentDefault && <small className="default-replacement">{t("provider.editor.replaceDefault", { name: currentDefault.name })}</small>}</div>
          </section>
          <section className="connection-form-section">
            <div className="connection-section-title"><ArrowRightLeft size={17} /><h2>{t("provider.editor.protocols")}</h2></div>
            <div className="editor-client-protocols">{PROTOCOLS.map(({ id, brand, shortName, name }) => {
              const native = id === draft.provider_type;
              const enabled = native || draft.protocol_support[id];
              return <div className={`editor-client-protocol protocol-${id} ${enabled ? "active" : ""}`} key={id}><span className="client-protocol-mark"><ProviderIcon brand={brand} size={21} /></span><div className="client-protocol-copy">{native ? <div className="client-protocol-heading"><strong title={name}>{shortName}</strong><span className="native-protocol-status" title={t("provider.editor.nativeStatus")}><Check size={14} /></span></div> : <label className="toggle-row client-protocol-heading" title={t("provider.editor.translationTitle", { name })}><strong>{shortName}</strong><input type="checkbox" aria-label={t("provider.editor.translationLabel", { shortName })} checked={enabled} onChange={(event) => update("protocol_support", { ...draft.protocol_support, [id]: event.target.checked })} /><span className="toggle" /></label>}<small>{native ? t("provider.editor.nativeAlways") : enabled ? t("provider.editor.translationOn") : t("provider.editor.translationOff")}</small></div></div>;
            })}</div>
          </section>
          <section className="connection-form-section">
            <div className="connection-section-title"><Boxes size={17} /><h2>{t("provider.editor.models.title")}</h2><button type="button" className="text-button" onClick={() => update("models", [...draft.models, { id: 0, provider_id: draft.id, route_id: "", name: "", standard_model_id: null, non_standard: false }])}><Plus size={15} />{t("provider.editor.models.add")}</button></div>
            <div className="connection-model-table" tabIndex={fieldErrors.models ? -1 : undefined} aria-invalid={Boolean(fieldErrors.models)}>
              {draft.models.length > 0 && <div className="connection-model-head"><span>{t("provider.editor.models.name")}</span><span>{t("provider.editor.models.standard")}</span><span>{t("provider.editor.models.nonStandard")}</span><span /></div>}
              {draft.models.map((model, index) => {
                const standard = matchingStandards.find((item) => item.id === model.standard_model_id);
                return <div className="provider-model-binding" key={`${model.id}-${index}`}><div className="connection-model-row"><label className="provider-model-name"><input aria-label={t("provider.editor.models.nameLabel", { index: index + 1 })} value={model.name} placeholder={draft.provider_type === "anthropic" ? "claude-sonnet-4-5" : "gpt-4.1"} spellCheck={false} onChange={(event) => update("models", draft.models.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item))} /></label>
                  <Select label={t("provider.editor.models.standardLabel", { index: index + 1 })} value={standard?.id ?? ""} icon={Boxes} className="provider-standard-model-select" menuClassName="standard-model-select-menu" disabled={Boolean(busy) || !matchingStandards.length}
                    onValueChange={(value) => update("models", draft.models.map((item, itemIndex) => itemIndex === index ? { ...item, standard_model_id: value || null } : item))}
                    options={[{ value: "", label: matchingStandards.length ? t("provider.editor.models.choose") : t("provider.editor.models.noneForProtocol") }, ...matchingStandards.map((item) => ({ value: item.id, label: item.name, leading: <ProviderIcon brand={item.brand || modelBrand(item.name)} size={20} />, description: modelDescription(item, t) }))]} />
                  <label className="toggle-row provider-model-nonstandard"><input type="checkbox" checked={Boolean(model.non_standard)} aria-label={t("provider.editor.models.nonStandardLabel", { index: index + 1 })} onChange={(event) => update("models", draft.models.map((item, itemIndex) => itemIndex === index ? { ...item, non_standard: event.target.checked } : item))} /><span className="toggle" /></label>
                  <button type="button" className="icon-button action-danger" title={t("provider.editor.models.remove")} aria-label={t("provider.editor.models.removeLabel", { index: index + 1 })} onClick={() => update("models", draft.models.filter((_, itemIndex) => itemIndex !== index))}><Trash2 size={15} /></button></div>{standard && <ModelCapabilitySummary capabilities={standard.capabilities} />}</div>;
              })}
              {draft.models.length === 0 && <div className="connection-model-empty">{t("provider.editor.models.empty")}</div>}
              {fieldErrors.models && <small className="field-error">{fieldErrors.models}</small>}
            </div>
          </section>
          <section className="connection-form-section connection-test-section">
            <div className="connection-section-title"><FlaskConical size={17} /><h2>{t("provider.editor.test.title")}</h2></div>
            <div className="connection-test-controls"><label className="field"><span>{t("provider.editor.test.model")}<small className="optional-label">{cleanModels()[0]?.name ? t("provider.editor.optional") : t("provider.editor.required")}</small></span><input name="test_model" aria-invalid={Boolean(fieldErrors.test_model)} value={testModel} onChange={(event) => { setTestModel(event.target.value); setFieldErrors((current) => ({ ...current, test_model: "" })); }} placeholder={cleanModels()[0]?.name || t("provider.editor.test.modelPlaceholder")} spellCheck={false} /></label><button type="button" className="secondary-button" onClick={() => void testConnection()}>{busy === "test" ? <LoaderCircle size={16} className="spinning" /> : <FlaskConical size={16} />}{busy === "test" ? t("provider.editor.test.testing") : t("provider.editor.test.action")}</button></div>
            {fieldErrors.test_model && <small className="field-error">{fieldErrors.test_model}</small>}
          </section>
          <details className="connection-advanced" open={fieldErrors.extra ? true : undefined}><summary><SlidersHorizontal size={16} /><span>{t("provider.editor.advanced")}</span></summary><div className="connection-advanced-content">
            <label className="field"><span>{t("provider.editor.headers")}</span><textarea className="json-input" value={extraText} aria-invalid={Boolean(fieldErrors.extra)} onChange={(event) => { setExtraText(event.target.value); setFieldErrors((current) => ({ ...current, extra: "" })); }} spellCheck={false} placeholder={'{\n  "Header-Name": "value"\n}'} />{fieldErrors.extra && <small className="field-error">{fieldErrors.extra}</small>}</label>
          </div></details>
        </fieldset>
        <footer className="connection-editor-footer"><span className={`draft-state ${dirty ? "unsaved" : ""}`}><span />{footer}</span><div><button type="button" className="secondary-button" disabled={Boolean(busy)} onClick={back}>{t("provider.editor.cancel")}</button><button type="submit" className="primary-button" disabled={Boolean(busy) || Boolean(provider && !dirty)}>{busy === "save" ? <LoaderCircle size={16} className="spinning" /> : provider ? <Save size={16} /> : <Plus size={16} />}{submitLabel}</button></div></footer>
      </form>
    </section>
  );
}

export function ProviderEditorPage() {
  const selectedId = useProviderSelection((state) => state.selectedId);
  const providers = useProviders().data ?? [];
  const standardModels = useStandardModels().data ?? [];
  const provider = providers.find((item) => item.id === selectedId) ?? null;
  return <ProviderEditorForm key={selectedId ?? "new"} provider={selectedId ? provider : null} providers={providers} standardModels={standardModels} />;
}
