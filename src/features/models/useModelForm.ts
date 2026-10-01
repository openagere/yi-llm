import { useRef, useState } from "react";
import { describeError } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import type { ProviderType } from "@/shared/lib";
import { useEditorGuard, useNavigation } from "@/shared/navigation";
import { notify } from "@/shared/notify";
import { effortLevels, MODALITIES, parseTokenCapacity, tokenInput } from "./capabilities";
import { emptyModel, isModelDirty, protocolConflictMessage, tokenDraftsOf, validateModel, type CapacityField, type ModalityField } from "./draft";
import { useSaveStandardModel } from "./hooks";
import type { EffortSupport, Modality, ModelCapabilities, StandardModel } from "./types";

type Effort = ModelCapabilities["effort"];

const keepValidDefault = (levels: string[], value: string | null) => value && levels.includes(value) ? value : null;

/** 标准模型编辑表单的全部状态与操作；保存成功后跳回列表。 */
export function useModelForm(model: StandardModel | null, models: StandardModel[]) {
  const { t } = useI18n();
  const { goTo } = useNavigation();
  const saveModel = useSaveStandardModel();
  const [initial] = useState<StandardModel>(() => model ? structuredClone(model) : emptyModel());
  const [draft, setDraft] = useState(() => structuredClone(initial));
  const [tokenDrafts, setTokenDrafts] = useState(() => tokenDraftsOf(initial));
  const [errors, setErrors] = useState<Record<string, string>>({});
  const modalityMemory = useRef<Record<ModalityField, Modality[]>>({
    input_modalities: initial.capabilities.input_modalities ?? ["text"],
    output_modalities: initial.capabilities.output_modalities ?? ["text"],
  });
  const effortMemory = useRef<Effort>(initial.capabilities.effort.support === "supported"
    ? structuredClone(initial.capabilities.effort)
    : { support: "supported", levels: ["low", "medium", "high"], default: null });

  const busy = saveModel.isPending;
  const dirty = isModelDirty(draft, tokenDrafts, initial);
  const existing = models.some((item) => item.id === draft.id);
  const caps = draft.capabilities;
  const protocolConflict = protocolConflictMessage(draft, initial, t);
  useEditorGuard(dirty, busy);

  function updateCaps<K extends keyof ModelCapabilities>(field: K, value: ModelCapabilities[K]) {
    setDraft((current) => ({ ...current, capabilities: { ...current.capabilities, [field]: value } }));
    setErrors((current) => ({ ...current, [field]: "" }));
  }
  function setName(name: string) {
    setDraft((current) => ({ ...current, name }));
    setErrors((current) => ({ ...current, name: "" }));
  }
  function setBrand(brand: string) {
    setDraft((current) => ({ ...current, brand }));
  }
  function changeProtocol(protocol: ProviderType) {
    setDraft((current) => {
      const { effort } = current.capabilities;
      const levels = effort.levels.filter((level) => effortLevels(protocol).includes(level));
      return { ...current, protocol, capabilities: { ...current.capabilities, effort: { ...effort, levels, default: keepValidDefault(levels, effort.default) } } };
    });
    setErrors({});
  }
  function changeCapacity(field: CapacityField, text: string) {
    setTokenDrafts((current) => ({ ...current, [field]: text }));
    const parsed = parseTokenCapacity(text);
    if (!parsed.error) updateCaps(field, parsed.value);
    setErrors((current) => ({ ...current, context_window: "", max_output_tokens: "" }));
  }
  function blurCapacity(field: CapacityField) {
    const parsed = parseTokenCapacity(tokenDrafts[field]);
    setErrors((current) => ({ ...current, [field]: parsed.error ?? "" }));
    if (!parsed.error) setTokenDrafts((current) => ({ ...current, [field]: tokenInput(parsed.value) }));
  }
  function declareModalities(field: ModalityField, declared: boolean) {
    const current = caps[field];
    if (!declared && current) modalityMemory.current[field] = [...current];
    updateCaps(field, declared ? [...modalityMemory.current[field]] : null);
  }
  function toggleModality(field: ModalityField, id: Modality, checked: boolean) {
    const current = caps[field];
    updateCaps(field, checked
      ? MODALITIES.filter((item) => item === id || current?.includes(item))
      : current?.filter((value) => value !== id) ?? []);
  }
  function changeEffortSupport(support: EffortSupport) {
    if (caps.effort.support === "supported") effortMemory.current = structuredClone(caps.effort);
    const remembered = effortMemory.current;
    const levels = remembered.levels.filter((level) => effortLevels(draft.protocol).includes(level));
    updateCaps("effort", support === "supported"
      ? { support, levels, default: keepValidDefault(levels, remembered.default) }
      : { support, levels: [], default: null });
  }
  function toggleEffortLevel(level: string, checked: boolean) {
    const levels = checked
      ? effortLevels(draft.protocol).filter((value) => value === level || caps.effort.levels.includes(value))
      : caps.effort.levels.filter((value) => value !== level);
    updateCaps("effort", { ...caps.effort, levels, default: keepValidDefault(levels, caps.effort.default) });
  }
  function setDefaultEffort(value: string) {
    updateCaps("effort", { ...caps.effort, default: value || null });
  }
  function submit() {
    if (busy) return;
    const { capabilities, errors: invalid } = validateModel(draft, tokenDrafts, models, protocolConflict, t);
    setErrors(invalid);
    if (Object.keys(invalid).length) {
      notify(Object.values(invalid)[0], "error");
      requestAnimationFrame(() => {
        const field = document.getElementById("standard-model-form")?.querySelector<HTMLElement>('[aria-invalid="true"]');
        field?.scrollIntoView({ block: "center" });
        field?.focus();
      });
      return;
    }
    const next = { ...draft, name: draft.name.trim(), capabilities };
    saveModel.mutate(next, {
      onSuccess: () => {
        goTo("model-list");
        notify(t("model.editor.saved", { name: next.name }));
      },
      onError: (reason) => notify(t("model.editor.saveFailed", { reason: describeError(reason) }), "error"),
    });
  }

  return {
    initial, draft, tokenDrafts, errors, busy, dirty, existing, protocolConflict,
    setName, setBrand, changeProtocol, changeCapacity, blurCapacity, declareModalities, toggleModality,
    changeEffortSupport, toggleEffortLevel, setDefaultEffort, submit,
  };
}

export type ModelForm = ReturnType<typeof useModelForm>;
