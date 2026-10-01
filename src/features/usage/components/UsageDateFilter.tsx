import { useId, useRef, useState } from "react";
import { CalendarDays, CalendarRange, Check, X } from "lucide-react";
import { useI18n, type MessageKey } from "@/shared/i18n";
import { Select } from "@/shared/ui";
import { presetRange, usageDatePresets, validateRange, type UsageDatePreset } from "../dateRange";
import { dateRange, localDay } from "../format";
import type { UsageDateRange } from "../types";

interface UsageDateFilterProps {
  range: UsageDateRange;
  preset: UsageDatePreset;
  onChange: (range: UsageDateRange, preset: UsageDatePreset) => void;
}

export function UsageDateFilter({ range, preset, onChange }: UsageDateFilterProps) {
  const { t } = useI18n();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(range);
  const [error, setError] = useState<MessageKey | null>(null);
  const errorId = useId();
  const panelId = useId();
  const select = useRef<HTMLButtonElement>(null);
  const startInput = useRef<HTMLInputElement>(null);
  const today = localDay(new Date());
  function edit() { setDraft(range); setError(null); setEditing(true); }
  function cancel() { setEditing(false); setError(null); select.current?.focus(); }
  function choose(value: UsageDatePreset) {
    if (value === "custom") { edit(); return; }
    setEditing(false); setError(null); onChange(presetRange(value), value);
  }
  function apply() {
    const message = validateRange(draft);
    if (message) { setError(message); return; }
    onChange(draft, "custom"); cancel();
  }
  return <div className="analytics-date-filter">
    <div className="analytics-date-selection">
      <Select ref={select} className="analytics-range-select" icon={CalendarDays} label={t("usage.date.range")} value={editing ? "custom" : preset} onValueChange={(value) => choose(value as UsageDatePreset)} options={usageDatePresets.map((item) => ({ value: item.value, label: t(item.labelKey), icon: item.value === "custom" ? CalendarRange : CalendarDays }))} onCloseAutoFocus={(event) => { if (editing) { event.preventDefault(); requestAnimationFrame(() => startInput.current?.focus()); } }} />
      <span className="analytics-date-range">{dateRange(range)}</span>
      {preset === "custom" && !editing && <button type="button" className="icon-button analytics-edit-range" aria-label={t("usage.date.editCustom")} title={t("usage.date.editCustom")} aria-controls={panelId} onClick={edit}><CalendarDays size={14} /></button>}
    </div>
    {editing && <form id={panelId} className="analytics-custom-range" onSubmit={(event) => { event.preventDefault(); apply(); }} onKeyDown={(event) => { if (event.key === "Escape") { event.preventDefault(); cancel(); } }}>
      <label>{t("usage.date.startDate")}<input ref={startInput} autoFocus type="date" aria-label={t("usage.date.startDate")} aria-describedby={error ? errorId : undefined} aria-invalid={Boolean(error)} required max={today} value={draft.startDate} onChange={(event) => { setDraft({ ...draft, startDate: event.target.value }); setError(null); }} /></label>
      <label>{t("usage.date.endDate")}<input type="date" aria-label={t("usage.date.endDate")} aria-describedby={error ? errorId : undefined} aria-invalid={Boolean(error)} required max={today} value={draft.endDate} onChange={(event) => { setDraft({ ...draft, endDate: event.target.value }); setError(null); }} /></label>
      <div className="analytics-date-actions"><button type="submit" className="analytics-apply-range"><Check size={14} />{t("usage.date.apply")}</button><button type="button" className="icon-button" title={t("usage.date.cancel")} aria-label={t("usage.date.cancel")} onClick={cancel}><X size={15} /></button></div>
      {error && <span id={errorId} className="analytics-date-error" role="alert">{t(error)}</span>}
    </form>}
  </div>;
}
