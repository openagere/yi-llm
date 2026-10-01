import "@/styles/models.css";
import { Brain } from "lucide-react";
import { useI18n } from "@/shared/i18n";
import type { ProviderType } from "@/shared/lib";
import { defaultCapabilities, effectiveEfforts, effectiveModalities, tokenLabel } from "../capabilities";
import type { Modality, ModelCapabilities } from "../types";
import { modalityIcons } from "./modalityIcons";

interface Props {
  capabilities?: ModelCapabilities;
  client?: ProviderType;
  upstream?: ProviderType;
  inputFilter?: Modality[];
}

export function ModelCapabilitySummary({ capabilities, client, upstream, inputFilter }: Props) {
  const { t } = useI18n();
  const caps = capabilities ?? defaultCapabilities();
  const effective: Modality[] | null = client && upstream ? effectiveModalities(caps.input_modalities, client, upstream) : caps.input_modalities;
  const modalities = effective?.filter((modality) => !inputFilter || inputFilter.includes(modality)) ?? null;
  const levels = client && upstream ? effectiveEfforts(caps, client, upstream) : caps.effort.levels;
  const effortAvailable = caps.effort.support === "supported" && levels.length > 0;
  return <span className="model-capability-summary">
    {modalities === null ? <span className="capability-undeclared">{t("model.summary.undeclared")}</span> : modalities.map((modality) => {
      const Icon = modalityIcons[modality];
      const label = t(`model.modality.${modality}`);
      return <span key={modality} title={t("model.summary.modalityInput", { label })}><Icon size={12} />{label}</span>;
    })}
    {caps.context_window !== null && <span title={t("model.summary.contextTitle")}>Context {tokenLabel(caps.context_window)}</span>}
    {caps.max_output_tokens !== null && <span title={t("model.summary.outputTitle")}>{t("model.summary.output")}{tokenLabel(caps.max_output_tokens)}</span>}
    {caps.effort.support !== "unknown" && <span title={effortAvailable ? t("model.summary.effortAvailableTitle", { levels: levels.join(" / ") }) : t("model.summary.effortUnsupportedTitle")}><Brain size={12} />Effort {effortAvailable ? t("model.summary.effortAvailable") : t("model.summary.effortUnavailable")}</span>}
  </span>;
}
