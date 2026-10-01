import type { ReactNode } from "react";
import { ArrowLeft, X } from "lucide-react";
import "@/styles/page-header.css";
import { useI18n } from "@/shared/i18n";

interface Props {
  eyebrow: string;
  title: string;
  subtitle?: string;
  onBack?: () => void;
  onClose?: () => void;
  closeLabel?: string;
  actions?: ReactNode;
  primaryAction?: ReactNode;
  backLabel?: string;
  disabled?: boolean;
  navigationInToolbar?: boolean;
}

export function PageHeader({ eyebrow, title, subtitle, onBack, onClose, closeLabel, actions, primaryAction, backLabel, disabled = false, navigationInToolbar = false }: Props) {
  const { t } = useI18n();
  const back = backLabel ?? t("common.back");
  const close = closeLabel ?? t("common.close");
  return <header className="workspace-header">
    <div className="workspace-header-identity">
      {onBack && !navigationInToolbar && <button type="button" className="icon-button workspace-header-back" title={back} aria-label={back} disabled={disabled} onClick={onBack}><ArrowLeft size={18} /></button>}
      <div className="workspace-header-copy">
        <span className="workspace-header-context">{eyebrow}</span>
        <h1>{title}</h1>
        {subtitle && <p>{subtitle}</p>}
      </div>
    </div>
    {(actions || primaryAction || (onClose && !navigationInToolbar)) && <div className="workspace-header-actions" role="group" aria-label={t("ui.pageHeader.actions", { title })}>
      {actions && <div className="workspace-header-tools">{actions}</div>}
      {primaryAction && <div className="workspace-header-primary">{primaryAction}</div>}
      {onClose && !navigationInToolbar && <div className="workspace-header-dismiss"><button type="button" className="icon-button" title={close} aria-label={close} disabled={disabled} onClick={onClose}><X size={17} /></button></div>}
    </div>}
  </header>;
}
