import { Check } from "lucide-react";
import { PROTOCOLS, type ProviderType } from "@/shared/lib";
import { ProviderIcon } from "./ProviderIcon";

interface Props {
  value: ProviderType;
  onChange: (value: ProviderType) => void;
  label: string;
  disabled?: boolean;
  invalid?: boolean;
  describedBy?: string;
}

export function ProtocolSelector({ value, onChange, label, disabled = false, invalid = false, describedBy }: Props) {
  return <div className="upstream-selector" role="radiogroup" aria-label={label} aria-invalid={invalid} aria-describedby={describedBy} tabIndex={invalid ? -1 : undefined} onKeyDown={(event) => {
    if (disabled || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const index = PROTOCOLS.findIndex((item) => item.id === value);
    const next = event.key === "Home" ? 0 : event.key === "End" ? PROTOCOLS.length - 1
      : (index + (["ArrowRight", "ArrowDown"].includes(event.key) ? 1 : PROTOCOLS.length - 1)) % PROTOCOLS.length;
    onChange(PROTOCOLS[next].id);
    event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="radio"]')[next]?.focus();
  }}>
    {PROTOCOLS.map(({ id, shortName, name, brand }) => <button type="button" role="radio" tabIndex={value === id ? 0 : -1}
      aria-checked={value === id} key={id} title={name} disabled={disabled}
      className={`upstream-choice protocol-${id} ${value === id ? "selected" : ""}`} onClick={() => onChange(id)}>
      <ProviderIcon brand={brand} size={14} /><strong>{shortName}</strong>
      <Check size={12} className="protocol-choice-check" style={{ visibility: value === id ? "visible" : "hidden" }} />
    </button>)}
  </div>;
}
