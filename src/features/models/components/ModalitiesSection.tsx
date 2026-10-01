import { Image } from "lucide-react";
import { useI18n } from "@/shared/i18n";
import { MODALITIES } from "../capabilities";
import { MODALITY_FIELDS } from "../draft";
import type { ModelForm } from "../useModelForm";
import { modalityIcons } from "./modalityIcons";

export function ModalitiesSection({ form }: { form: ModelForm }) {
  const { t } = useI18n();
  const { draft, errors, declareModalities, toggleModality } = form;
  const caps = draft.capabilities;
  return <section className="connection-form-section">
    <div className="connection-section-title"><Image size={17} /><h2>{t("model.editor.modalities.title")}</h2></div>
    <div className="standard-model-modalities">{MODALITY_FIELDS.map((field) => {
      const values = caps[field];
      const input = field === "input_modalities";
      return <fieldset className={`modality-fieldset ${values === null ? "undeclared" : ""}`} key={field} aria-invalid={Boolean(errors[field])} tabIndex={errors[field] ? -1 : undefined}>
        <legend className="sr-only">{input ? t("model.editor.modalities.inputLegend") : t("model.editor.modalities.outputLegend")}</legend>
        <div className="modality-heading"><strong>{input ? t("model.editor.modalities.inputHeading") : t("model.editor.modalities.outputHeading")}</strong>
          <span>{values === null ? t("model.editor.modalities.undeclared") : t("model.editor.modalities.count", { count: values.length })}</span>
          <label className="toggle-row"><input type="checkbox" role="switch" aria-label={input ? t("model.editor.modalities.declareInput") : t("model.editor.modalities.declareOutput")} checked={values !== null} onChange={(event) => declareModalities(field, event.target.checked)} /><span className="toggle" /></label>
        </div>
        <div className="modality-options">{MODALITIES.map((id) => {
          const Icon = modalityIcons[id];
          return <label className={`modality-option ${values?.includes(id) ? "selected" : ""}`} key={id}><Icon size={17} /><span>{t(`model.modality.${id}`)}</span><input type="checkbox" disabled={values === null} checked={values?.includes(id) ?? false} onChange={(event) => toggleModality(field, id, event.target.checked)} /></label>;
        })}</div>
        {errors[field] && <small className="field-error">{errors[field]}</small>}
      </fieldset>;
    })}</div>
  </section>;
}
