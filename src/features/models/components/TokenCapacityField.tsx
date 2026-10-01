import { useId } from "react";
import { Info } from "lucide-react";
import { useI18n } from "@/shared/i18n";
import { Select } from "@/shared/ui";
import { parseTokenCapacity } from "../capabilities";
import { TOKEN_PRESETS, type CapacityField } from "../draft";

interface Props {
  field: CapacityField;
  text: string;
  error?: string;
  disabled: boolean;
  onChange: (value: string) => void;
  onBlur: () => void;
}

export function TokenCapacityField({ field, text, error, disabled, onChange, onBlur }: Props) {
  const { t } = useI18n();
  const id = useId();
  const parsed = parseTokenCapacity(text);
  const context = field === "context_window";
  return <div className="field token-capacity-field">
    <label htmlFor={id}>{context ? "Context Size" : t("model.editor.capacity.maxOutput")}<small className="optional-label">Token</small></label>
    <div className="token-capacity-input">
      <input id={id} type="text" value={text} aria-invalid={Boolean(error)} aria-describedby={`${id}-detail`}
        placeholder={context ? "256K" : "32K"} autoComplete="off" spellCheck={false}
        onChange={(event) => onChange(event.target.value)} onBlur={onBlur} />
      <Select value="" onValueChange={(value) => onChange(value === "clear" ? "" : value)} disabled={disabled}
        label={context ? t("model.editor.capacity.contextPresets") : t("model.editor.capacity.outputPresets")}
        options={[
          { value: "", label: t("model.editor.capacity.presetPlaceholder"), disabled: true },
          { value: "clear", label: t("model.editor.capacity.undeclared") },
          ...TOKEN_PRESETS[field].map((value) => ({ value, label: value })),
        ]} />
    </div>
    <div id={`${id}-detail`} className="token-capacity-detail">
      {error ? <small className="field-error" role="alert">{error}</small>
        : <small>{parsed.error ? t("model.editor.capacity.formatPending") : parsed.value === null ? t("model.editor.capacity.undeclared") : `${parsed.value.toLocaleString("en-US")} Tokens`}</small>}
      <span tabIndex={0} aria-label={t("model.editor.capacity.unitHelp")} title={t("model.editor.capacity.unitTitle")}><Info size={13} /></span>
    </div>
  </div>;
}
