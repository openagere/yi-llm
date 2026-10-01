import { Brain, Boxes, CircleHelp, FileText, LoaderCircle, MinusCircle, Save } from "lucide-react";
import "@/styles/models.css";
import { useI18n } from "@/shared/i18n";
import { useNavigation } from "@/shared/navigation";
import { modelBrand, PageHeader, ProtocolSelector, Select } from "@/shared/ui";
import { effortLevels } from "../capabilities";
import { CAPACITY_FIELDS } from "../draft";
import { useStandardModels } from "../hooks";
import { useModelSelection } from "../selection";
import type { EffortSupport, StandardModel } from "../types";
import { useModelForm } from "../useModelForm";
import { BrandIconPicker } from "./BrandIconPicker";
import { ModalitiesSection } from "./ModalitiesSection";
import { TokenCapacityField } from "./TokenCapacityField";

const EFFORT_STATES = [
  { value: "unknown", icon: CircleHelp, labelKey: "model.editor.effort.unknown" },
  { value: "unsupported", icon: MinusCircle, labelKey: "model.editor.effort.unsupported" },
  { value: "supported", icon: Brain, labelKey: "model.editor.effort.supported" },
] as const satisfies readonly { value: EffortSupport; icon: typeof Brain; labelKey: "model.editor.effort.unknown" | "model.editor.effort.unsupported" | "model.editor.effort.supported" }[];

function ModelEditorForm({ model, models }: { model: StandardModel | null; models: StandardModel[] }) {
  const { t } = useI18n();
  const { back } = useNavigation();
  const form = useModelForm(model, models);
  const { draft, tokenDrafts, errors, busy, dirty, existing, protocolConflict, initial } = form;
  const caps = draft.capabilities;
  const footer = busy ? t("model.editor.footer.saving") : dirty ? t("model.editor.footer.unsaved") : existing ? t("model.editor.footer.saved") : t("model.editor.footer.fresh");

  return <section className="editor-page connection-editor-page standard-model-editor">
    <PageHeader eyebrow={existing ? initial.name : t("model.editor.eyebrowNew")} title={existing ? t("model.editor.titleEdit") : t("model.editor.titleAdd")} onBack={back} backLabel={t("model.editor.back")} disabled={busy} primaryAction={<button type="submit" form="standard-model-form" className="primary-button" disabled={busy || (existing && !dirty)}>{busy ? <LoaderCircle className="spinning" size={16} /> : <Save size={16} />}{busy ? t("model.editor.saving") : t("model.editor.save")}</button>} />
    <form id="standard-model-form" noValidate onSubmit={(event) => { event.preventDefault(); form.submit(); }}>
      <fieldset className="connection-form-fields" disabled={busy}>
        <section className="connection-form-section">
          <div className="connection-section-title"><Boxes size={17} /><h2>{t("model.editor.info.title")}</h2>{draft.provider_count > 0 && <span className="standard-model-reference-count">{t("model.editor.info.references", { count: draft.provider_count })}</span>}</div>
          <div className="standard-model-identity-grid"><label className="field"><span>{t("model.editor.info.name")}<span className="required-mark">*</span></span><input autoFocus aria-invalid={Boolean(errors.name)} value={draft.name} onChange={(event) => form.setName(event.target.value)} placeholder={t("model.editor.info.namePlaceholder")} />{errors.name && <small className="field-error">{errors.name}</small>}</label>
            <BrandIconPicker value={draft.brand} automatic={modelBrand(draft.name)} disabled={busy} onChange={form.setBrand} />
            <div className="field standard-model-protocol-field"><span>{t("model.editor.info.protocol")}</span>
              <ProtocolSelector value={draft.protocol} onChange={form.changeProtocol} label={t("model.editor.info.protocolLabel")} disabled={busy} invalid={Boolean(protocolConflict)} describedBy={protocolConflict ? "model-protocol-error" : undefined} />
              {protocolConflict && <small id="model-protocol-error" className="field-error" role="alert">{protocolConflict}</small>}
            </div>
          </div>
        </section>
        <ModalitiesSection form={form} />
        <section className="connection-form-section">
          <div className="connection-section-title"><FileText size={17} /><h2>{t("model.editor.capacity.title")}</h2></div>
          <div className="standard-model-info-grid">{CAPACITY_FIELDS.map((field) => <TokenCapacityField key={field} field={field} text={tokenDrafts[field]} error={errors[field]} disabled={busy} onChange={(value) => form.changeCapacity(field, value)} onBlur={() => form.blurCapacity(field)} />)}</div>
        </section>
        <section className="connection-form-section">
          <div className="connection-section-title"><Brain size={17} /><h2>Reasoning Effort</h2></div>
          <div className="effort-support-selector" role="radiogroup" aria-label={t("model.editor.effort.supportLabel")}>{EFFORT_STATES.map(({ value, labelKey, icon: Icon }) => <label key={value} className={caps.effort.support === value ? "selected" : ""}>
            <input type="radio" name="effort-support" value={value} checked={caps.effort.support === value} onChange={() => form.changeEffortSupport(value)} /><Icon size={15} /><span>{t(labelKey)}</span>
          </label>)}</div>
          {caps.effort.support === "supported" && <div className="effort-configuration">
            <fieldset className="effort-level-fieldset" aria-invalid={Boolean(errors.effort)} tabIndex={errors.effort ? -1 : undefined}><legend>{t("model.editor.effort.levels")}</legend><div className="effort-levels">{effortLevels(draft.protocol).map((level) => <label className={caps.effort.levels.includes(level) ? "selected" : ""} key={level}>
              <span><strong>{level}</strong><small>{t(`model.editor.effort.names.${level as "low"}`)}</small></span><input type="checkbox" aria-label={level} checked={caps.effort.levels.includes(level)} onChange={(event) => form.toggleEffortLevel(level, event.target.checked)} /></label>)}</div>{errors.effort && <small className="field-error" role="alert">{errors.effort}</small>}</fieldset>
            <div className="field effort-default-field"><span>{t("model.editor.effort.defaultLegend")}</span><Select value={caps.effort.default ?? ""} label={t("model.editor.effort.defaultLabel")} disabled={busy}
              onValueChange={form.setDefaultEffort}
              options={[{ value: "", label: t("model.editor.effort.unspecified") }, ...caps.effort.levels.map((value) => ({ value, label: `${value} · ${t(`model.editor.effort.names.${value as "low"}`)}` }))]} /></div>
          </div>}
        </section>
      </fieldset>
      <footer className="connection-editor-footer"><span className={`draft-state ${dirty ? "unsaved" : ""}`}><span />{footer}</span><div><button type="button" className="secondary-button" disabled={busy} onClick={back}>{t("model.editor.cancel")}</button><button type="submit" className="primary-button" disabled={busy || (existing && !dirty)}><Save size={15} />{t("model.editor.save")}</button></div></footer>
    </form>
  </section>;
}

export function ModelEditorPage() {
  const selected = useModelSelection((state) => state.selectedModel);
  const models = useStandardModels().data ?? [];
  return <ModelEditorForm key={selected?.id ?? "new"} model={selected} models={models} />;
}
