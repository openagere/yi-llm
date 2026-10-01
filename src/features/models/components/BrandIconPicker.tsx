import { useEffect, useId, useRef, useState } from "react";
import { Check, ChevronDown, Search } from "lucide-react";
import { useI18n } from "@/shared/i18n";
import { BRAND_ICONS, ProviderIcon } from "@/shared/ui";

interface Props {
  value: string;
  automatic?: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}

export function BrandIconPicker({ value, automatic, onChange, disabled = false }: Props) {
  const { t } = useI18n();
  const [search, setSearch] = useState("");
  const details = useRef<HTMLDetailsElement>(null);
  const id = useId();
  const selected = BRAND_ICONS.find((item) => item.id === value);
  const visible = BRAND_ICONS.filter((item) => `${item.label} ${item.id} ${item.keywords.join(" ")}`.toLowerCase().includes(search.trim().toLowerCase()));
  useEffect(() => {
    function close(event: PointerEvent) {
      if (details.current?.open && !details.current.contains(event.target as Node)) details.current.open = false;
    }
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, []);
  function closePicker() {
    if (!details.current) return;
    details.current.open = false;
    details.current.querySelector("summary")?.focus();
  }
  function select(brand: string) {
    onChange(brand);
    setSearch("");
    closePicker();
  }
  return <div className="field"><span id={id}>{t("model.editor.brand.label")}</span><details ref={details} className="brand-icon-picker">
    <summary aria-labelledby={id} aria-disabled={disabled} tabIndex={disabled ? -1 : 0} onClick={(event) => { if (disabled) event.preventDefault(); }}><ProviderIcon brand={value || automatic} size={20} /><span>{selected ? selected.label : t("model.editor.brand.auto")}</span><ChevronDown size={14} /></summary>
    <div className="brand-icon-popover" onKeyDown={(event) => { if (event.key === "Escape" && details.current) { event.stopPropagation(); closePicker(); } }}>
      <div className="brand-icon-search"><Search size={15} /><input type="search" aria-label={t("model.editor.brand.searchLabel")} placeholder={t("model.editor.brand.searchPlaceholder")} value={search} onChange={(event) => setSearch(event.target.value)} /></div>
      <button type="button" className="brand-auto-choice" onClick={() => select("")}><ProviderIcon brand={automatic} size={18} /><span>{t("model.editor.brand.auto")}</span>{!value && <Check size={14} />}</button>
      <div className="brand-icon-grid">{visible.map((item) => <button type="button" key={item.id} className={value === item.id ? "selected" : ""} aria-pressed={value === item.id} title={item.label} aria-label={t("model.editor.brand.choose", { label: item.label })} onClick={() => select(item.id)}><ProviderIcon brand={item.id} size={24} /><span>{item.label}</span></button>)}</div>
      {!visible.length && <div className="brand-icon-empty">{t("model.editor.brand.empty")}</div>}
    </div>
  </details></div>;
}
