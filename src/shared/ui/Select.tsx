import { forwardRef, type ReactNode } from "react";
import * as SelectPrimitive from "@radix-ui/react-select";
import { Check, ChevronDown, ChevronUp, type LucideIcon } from "lucide-react";
import "@/styles/controls.css";
import { useI18n } from "@/shared/i18n";

export interface SelectOption {
  value: string;
  label: string;
  icon?: LucideIcon;
  leading?: ReactNode;
  description?: string;
  disabled?: boolean;
}

interface SelectProps {
  value: string;
  onValueChange: (value: string) => void;
  options: readonly SelectOption[];
  label: string;
  icon?: LucideIcon;
  disabled?: boolean;
  className?: string;
  menuClassName?: string;
  placeholder?: string;
  onCloseAutoFocus?: SelectPrimitive.SelectContentProps["onCloseAutoFocus"];
}

export const Select = forwardRef<HTMLButtonElement, SelectProps>(function Select({ value, onValueChange, options, label, icon: Icon, disabled, className = "", menuClassName = "", placeholder, onCloseAutoFocus }, ref) {
  const { t } = useI18n();
  // Radix reserves an empty value for the placeholder; prefix all option values.
  const encoded = options.some((option) => option.value === value) ? `option:${value}` : "";
  const selected = options.find((option) => option.value === value);
  return <div className={`ui-select ${className}`}>
    <SelectPrimitive.Root value={encoded} onValueChange={(next) => onValueChange(next.slice(7))} disabled={disabled}>
      <SelectPrimitive.Trigger ref={ref} className="ui-select-trigger" aria-label={label} title={selected?.label}>
        {selected?.leading ? <span className="ui-select-custom-leading" aria-hidden="true">{selected.leading}</span> : Icon && <Icon className="ui-select-leading-icon" size={16} aria-hidden="true" />}
        <SelectPrimitive.Value className="ui-select-value" placeholder={placeholder ?? t("ui.select.placeholder")} />
        <SelectPrimitive.Icon className="ui-select-chevron"><ChevronDown size={14} aria-hidden="true" /></SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content className={`ui-select-menu ${menuClassName}`} position="popper" sideOffset={6} collisionPadding={12} aria-label={label} onCloseAutoFocus={onCloseAutoFocus}>
          <SelectPrimitive.ScrollUpButton className="ui-select-scroll"><ChevronUp size={14} aria-hidden="true" /></SelectPrimitive.ScrollUpButton>
          <SelectPrimitive.Viewport className="ui-select-viewport">
            {options.map(({ value: optionValue, label: optionLabel, icon: OptionIcon, leading, description, disabled: optionDisabled }) => <SelectPrimitive.Item key={optionValue} className="ui-select-option" value={`option:${optionValue}`} disabled={optionDisabled} textValue={optionLabel} aria-label={optionLabel} aria-description={description}>
              {leading ? <span className="ui-select-custom-leading" aria-hidden="true">{leading}</span> : OptionIcon && <OptionIcon className="ui-select-option-icon" size={16} aria-hidden="true" />}
              <span className="ui-select-option-copy"><SelectPrimitive.ItemText>{optionLabel}</SelectPrimitive.ItemText>{description && <small>{description}</small>}</span>
              <SelectPrimitive.ItemIndicator className="ui-select-check"><Check size={15} aria-hidden="true" /></SelectPrimitive.ItemIndicator>
            </SelectPrimitive.Item>)}
          </SelectPrimitive.Viewport>
          <SelectPrimitive.ScrollDownButton className="ui-select-scroll"><ChevronDown size={14} aria-hidden="true" /></SelectPrimitive.ScrollDownButton>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  </div>;
});
