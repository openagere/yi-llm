import { useEffect, useId, useRef, useState, type ChangeEvent, type FormEvent } from "react";
import { ArrowDownToLine, ArrowUpFromLine, LoaderCircle } from "lucide-react";
import { describeError } from "@/shared/api";
import { useI18n, type Translate } from "@/shared/i18n";
import { notify } from "@/shared/notify";
import { useExportProviderConfig, useImportProviderConfig } from "../hooks";
import type { ProviderConfigTransferResult } from "../types";

/** 与后端 `backup::MIN_PASSPHRASE_CHARS` 保持一致。 */
const MIN_PASSPHRASE_CHARS = 8;
/** 配置备份很小，超过该大小的文件直接拒绝，避免把无关大文件送进解密。 */
const MAX_IMPORT_BYTES = 8 * 1024 * 1024;

type TransferMode = "export" | "import";

/** Provider 列表头部的导入 / 导出入口：把配置加密成文件，或从加密文件恢复。 */
export function ProviderConfigTransfer({ disabled }: { disabled: boolean }) {
  const { t } = useI18n();
  const [mode, setMode] = useState<TransferMode | null>(null);
  return <>
    <button type="button" className="icon-button transfer-action" title={t("provider.list.transfer.import")} aria-label={t("provider.list.transfer.import")} disabled={disabled} onClick={() => setMode("import")}><ArrowDownToLine size={17} strokeWidth={1.9} aria-hidden="true" /></button>
    <button type="button" className="icon-button transfer-action" title={t("provider.list.transfer.export")} aria-label={t("provider.list.transfer.export")} disabled={disabled} onClick={() => setMode("export")}><ArrowUpFromLine size={17} strokeWidth={1.9} aria-hidden="true" /></button>
    {mode && <ProviderConfigTransferDialog mode={mode} onClose={() => setMode(null)} />}
  </>;
}

interface DialogProps {
  mode: TransferMode;
  onClose: () => void;
}

/** 导出收集加密字符串后由后端弹出保存对话框；导入在弹窗内选择文件再解密。 */
function ProviderConfigTransferDialog({ mode, onClose }: DialogProps) {
  const { t } = useI18n();
  const dialog = useRef<HTMLDialogElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const titleId = useId();
  const descriptionId = useId();
  const passphraseId = useId();
  const confirmationId = useId();
  const fileLabelId = useId();
  const [passphrase, setPassphrase] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [file, setFile] = useState<File | null>(null);
  const [error, setError] = useState("");
  const { mutate: runExport, isPending: exportingNow } = useExportProviderConfig();
  const { mutate: runImport, isPending: importingNow } = useImportProviderConfig();
  const exporting = mode === "export";
  const busy = exportingNow || importingNow;

  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);

  function onFile(event: ChangeEvent<HTMLInputElement>) {
    setFile(event.target.files?.[0] ?? null);
    setError("");
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy) return;
    const secret = passphrase.trim();
    // 按码位计数，与后端的字符数校验保持一致。
    if (Array.from(secret).length < MIN_PASSPHRASE_CHARS) {
      setError(t("provider.list.transfer.passphraseTooShort"));
      return;
    }
    if (exporting && secret !== confirmation.trim()) {
      setError(t("provider.list.transfer.passphraseMismatch"));
      return;
    }
    setError("");
    // 用户取消原生保存对话框时结果为 null，静默关闭即可。
    const report = (result: ProviderConfigTransferResult | null) => {
      if (result) {
        const warnings = result.terminal_config_warnings.join("; ");
        const message = transferMessage(t, mode, result);
        notify(warnings ? `${message}; ${warnings}` : message, warnings ? "info" : "success");
      }
      onClose();
    };
    const fail = (reason: unknown) => setError(describeError(reason));
    if (exporting) {
      runExport({ passphrase: secret, title: t("provider.list.transfer.exportTitle") }, { onSuccess: report, onError: fail });
      return;
    }
    if (!file) {
      setError(t("provider.list.transfer.fileRequired"));
      return;
    }
    if (file.size > MAX_IMPORT_BYTES) {
      setError(t("provider.list.transfer.fileTooLarge"));
      return;
    }
    let contents: string;
    try {
      contents = await file.text();
    } catch (reason) {
      setError(describeError(reason));
      return;
    }
    runImport({ passphrase: secret, contents, filename: file.name || "yi-llm-providers.yillm" }, { onSuccess: report, onError: fail });
  }

  const title = exporting ? t("provider.list.transfer.exportTitle") : t("provider.list.transfer.importTitle");
  const description = exporting ? t("provider.list.transfer.exportDescription") : t("provider.list.transfer.importDescription");
  const submitLabel = busy
    ? (exporting ? t("provider.list.transfer.exporting") : t("provider.list.transfer.importing"))
    : (exporting ? t("provider.list.transfer.exportAction") : t("provider.list.transfer.importAction"));

  return (
    <dialog ref={dialog} className="app-dialog transfer-dialog" aria-labelledby={titleId} aria-describedby={descriptionId}
      onCancel={(event) => { event.preventDefault(); if (!busy) onClose(); }}
      onClick={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
      <form className="app-dialog-body" onSubmit={submit}>
        <h2 id={titleId}>{title}</h2>
        <p id={descriptionId}>{description}</p>
        {!exporting && <div className="field">
          <span id={fileLabelId}>{t("provider.list.transfer.file")}</span>
          <div className="transfer-file">
            <span className={`transfer-file-name${file ? " is-selected" : ""}`} title={file?.name}>{file ? file.name : t("provider.list.transfer.fileEmpty")}</span>
            <button type="button" className="secondary-button" disabled={busy} onClick={() => fileInput.current?.click()}>{file ? t("provider.list.transfer.changeFile") : t("provider.list.transfer.chooseFile")}</button>
            <input ref={fileInput} className="transfer-file-input" type="file" accept=".yillm,.json,application/json" tabIndex={-1} aria-labelledby={fileLabelId} onChange={onFile} />
          </div>
          <small className="field-hint">{t("provider.list.transfer.fileHint")}</small>
        </div>}
        <div className="field">
          <label htmlFor={passphraseId}>{t("provider.list.transfer.passphrase")}</label>
          <input id={passphraseId} type="password" autoFocus autoComplete="off" placeholder={t("provider.list.transfer.passphrasePlaceholder")} value={passphrase} onChange={(event) => setPassphrase(event.target.value)} />
          <small className="field-hint">{t("provider.list.transfer.passphraseHint")}</small>
        </div>
        {exporting && <div className="field">
          <label htmlFor={confirmationId}>{t("provider.list.transfer.confirmPassphrase")}</label>
          <input id={confirmationId} type="password" autoComplete="off" value={confirmation} onChange={(event) => setConfirmation(event.target.value)} />
        </div>}
        {error && <p className="app-dialog-error" role="alert">{error}</p>}
        <div className="app-dialog-actions">
          <button type="button" className="secondary-button" disabled={busy} onClick={onClose}>{t("ui.dialog.cancel")}</button>
          <button type="submit" className="primary-button" disabled={busy}>{busy && <LoaderCircle size={15} className="spinning" />}{submitLabel}</button>
        </div>
      </form>
    </dialog>
  );
}

function transferMessage(t: Translate, mode: TransferMode, result: ProviderConfigTransferResult) {
  return mode === "export"
    ? t("provider.list.transfer.exported", { providers: result.providers, models: result.models, path: result.path })
    : t("provider.list.transfer.imported", { providers: result.providers, models: result.models, standards: result.standard_models });
}
