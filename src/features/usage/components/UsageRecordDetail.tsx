import { useEffect, useId, useRef } from "react";
import { X } from "lucide-react";
import { useI18n } from "@/shared/i18n";
import { ProviderIcon } from "@/shared/ui";
import { usageProtocolNames } from "../metrics";
import type { UsageModelBrandResolver, UsageModelFormatter } from "../modelNames";
import type { UsageRecord } from "../types";
import { UsageTokenMetrics } from "./UsageCommon";

export function UsageRecordDetail({ record, onClose, formatModel, resolveBrand }: { record: UsageRecord; onClose: () => void; formatModel: UsageModelFormatter; resolveBrand: UsageModelBrandResolver }) {
  const { t, language } = useI18n();
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);

  return <dialog ref={dialog} className="app-dialog analytics-record-dialog" aria-labelledby={titleId} onCancel={(event) => { event.preventDefault(); onClose(); }} onClick={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <div className="app-dialog-body analytics-record-dialog-content">
      <header className="app-dialog-header"><h2 id={titleId}>{t("usage.detail.title")}</h2><button type="button" className="icon-button" autoFocus title={t("usage.detail.close")} aria-label={t("usage.detail.close")} onClick={onClose}><X size={17} /></button></header>
      <dl className="analytics-record-metadata"><div><dt>{t("usage.common.model")}</dt><dd className="analytics-model-identity"><ProviderIcon brand={resolveBrand(record.model)} size={18} /><span>{formatModel(record.model, record.provider_name)}</span></dd></div><div><dt>Provider</dt><dd>{record.provider_name}</dd></div><div><dt>{t("usage.detail.protocol")}</dt><dd>{usageProtocolNames[record.protocol] ?? record.protocol}</dd></div><div><dt>{t("usage.detail.requestTime")}</dt><dd>{new Date(record.requested_at * 1000).toLocaleString(language, { hour12: false })}</dd></div><div><dt>{t("usage.common.routeId")}</dt><dd>{record.model}</dd></div></dl>
      <UsageTokenMetrics values={record} />
    </div>
  </dialog>;
}
