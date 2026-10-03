import { useEffect, useId, useRef } from "react";
import { LoaderCircle } from "lucide-react";
import { useI18n } from "@/shared/i18n";

interface Props {
  title: string;
  description: string;
  confirmLabel: string;
  busy?: boolean;
  destructive?: boolean;
  error?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ title, description, confirmLabel, busy = false, destructive = false, error, onConfirm, onCancel }: Props) {
  const { t } = useI18n();
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);

  return (
    <dialog ref={dialog} className="app-dialog" aria-labelledby={titleId} aria-describedby={descriptionId}
      onCancel={(event) => { event.preventDefault(); if (!busy) onCancel(); }}
      onClick={(event) => { if (event.target === event.currentTarget && !busy) onCancel(); }}>
      <div className="app-dialog-body">
        <h2 id={titleId}>{title}</h2>
        <p id={descriptionId}>{description}</p>
        {error && <p className="app-dialog-error" role="alert">{error}</p>}
        <div className="app-dialog-actions">
          <button type="button" className="secondary-button" autoFocus disabled={busy} onClick={onCancel}>{t("ui.dialog.cancel")}</button>
          <button type="button" className={destructive ? "danger-button solid" : "primary-button"} disabled={busy} onClick={onConfirm}>
            {busy && <LoaderCircle size={15} className="spinning" />}{busy ? t("ui.dialog.deleting") : confirmLabel}
          </button>
        </div>
      </div>
    </dialog>
  );
}
