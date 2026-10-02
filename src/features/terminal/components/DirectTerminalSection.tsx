import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Check } from "lucide-react";
import { type ProviderView } from "@/features/providers";
import { describeError } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { PROTOCOLS_BY_ID } from "@/shared/lib";
import { ProviderIcon, Select } from "@/shared/ui";
import { useApplyTerminalDirectConfig, useTerminalDirectPreview } from "../hooks";
import { directCompatibleProviders } from "../selection";
import type { TerminalClient, TerminalDirectProfile } from "../types";

interface ControllerOptions {
  client: TerminalClient;
  providers: readonly ProviderView[];
  profile: TerminalDirectProfile;
  active: boolean;
  disabled: boolean;
  onChange: (profile: TerminalDirectProfile) => void;
  onApplied: (
    profile: TerminalDirectProfile,
    result: { backup_paths: string[]; model_count: number },
  ) => void;
  onBusyChange: (busy: boolean) => void;
}

/** Keep preview, apply state and feedback together while the action lives in the page header. */
export function useDirectTerminalController({
  client,
  providers,
  profile,
  active,
  disabled,
  onChange,
  onApplied,
  onBusyChange,
}: ControllerOptions) {
  const { t } = useI18n();
  const apply = useApplyTerminalDirectConfig();
  const [fileIndex, setFileIndex] = useState(0);
  const [feedback, setFeedback] = useState<{
    client: TerminalClient;
    text: string;
    backups: string[];
  } | null>(null);
  const compatible = useMemo(() => directCompatibleProviders(providers, client), [providers, client]);
  const selected = compatible.find((provider) => provider.id === profile.provider_id);
  const models = selected?.models.map((model) => model.name).filter(Boolean) ?? [];
  const canPreview = active && !disabled && Boolean(selected);
  const preview = useTerminalDirectPreview(profile, canPreview);
  const currentFile = preview.preview?.files[Math.min(fileIndex, (preview.preview?.files.length ?? 1) - 1)];
  const busy = apply.isPending;
  const readyToApply = canPreview && Boolean(preview.preview) && !preview.pending && !preview.error;
  const clientName = client === "codex" ? "Codex CLI" : client === "claude-code" ? "Claude Code" : "OpenCode";

  useEffect(() => {
    onBusyChange(busy);
    return () => onBusyChange(false);
  }, [busy, onBusyChange]);

  function updateProvider(id: string) {
    onChange({ ...profile, provider_id: id, model: null });
    setFeedback(null);
    setFileIndex(0);
  }

  function updateSource(source: "native" | "provider") {
    onChange({ ...profile, model_source: source, model: null });
    setFeedback(null);
    setFileIndex(0);
  }

  function applyConfig() {
    if (!readyToApply || busy) return;
    apply.mutate(profile, {
      onSuccess: (result) => {
        onApplied(profile, result);
        setFeedback({
          client,
          text: t("terminal.direct.applied", { name: clientName }),
          backups: result.backup_paths,
        });
      },
    });
  }

  return {
    compatible,
    selected,
    models,
    preview,
    currentFile,
    fileIndex,
    setFileIndex,
    busy,
    readyToApply,
    applyConfig,
    updateProvider,
    updateSource,
    success: feedback?.client === client ? feedback.text : "",
    backupPaths: feedback?.client === client ? feedback.backups : [],
    applyError: apply.isError ? describeError(apply.error) : "",
  };
}

interface Props {
  client: TerminalClient;
  profile: TerminalDirectProfile;
  disabled: boolean;
  controller: ReturnType<typeof useDirectTerminalController>;
}

export function DirectTerminalSection({ client, profile, disabled, controller }: Props) {
  const { t } = useI18n();
  const {
    compatible,
    selected,
    models,
    preview,
    currentFile,
    fileIndex,
    setFileIndex,
    busy,
    updateProvider,
    updateSource,
    success,
    backupPaths,
    applyError,
  } = controller;
  const clientName = client === "codex" ? "Codex CLI" : client === "claude-code" ? "Claude Code" : "OpenCode";

  return (
    <div className="terminal-layout terminal-direct-layout">
      <section className="terminal-config-section terminal-direct-config">
        <div className="terminal-pane-heading">
          <h2>{t("terminal.direct.title")}</h2>
          <p>{t("terminal.direct.subtitle", { client: clientName })}</p>
        </div>

        <div className="terminal-step">
          <div className="terminal-step-content terminal-direct-field">
            <span className="terminal-direct-label">{t("terminal.direct.provider")}</span>
            <Select
              label={t("terminal.direct.provider")}
              value={profile.provider_id}
              onValueChange={updateProvider}
              disabled={disabled || busy || !compatible.length}
              placeholder={
                compatible.length ? t("terminal.direct.chooseProvider") : t("terminal.direct.noProvider")
              }
              options={compatible.map((provider) => ({
                value: provider.id,
                label: provider.name,
                leading: <ProviderIcon provider={provider} size={19} />,
                description: PROTOCOLS_BY_ID[provider.provider_type].name,
              }))}
            />
          </div>
        </div>

        {selected && (
          <div className="terminal-step terminal-step-last">
            <div className="terminal-step-content terminal-direct-field">
              <span className="terminal-direct-label">{t("terminal.direct.modelSource")}</span>
              {client !== "opencode" && (
                <div
                  className="terminal-direct-options"
                  role="radiogroup"
                  aria-label={t("terminal.direct.modelSource")}
                >
                  {(["native", "provider"] as const).map((source) => (
                    <label
                      key={source}
                      className={`terminal-direct-option${profile.model_source === source ? " selected" : ""}`}
                    >
                      <input
                        type="radio"
                        name={client + "-model-source"}
                        checked={profile.model_source === source}
                        onChange={() => updateSource(source)}
                        disabled={disabled || busy}
                      />
                      <span className="terminal-direct-option-copy">
                        <strong>
                          {t(
                            source === "native"
                              ? "terminal.direct.nativeModels"
                              : "terminal.direct.providerModels",
                          )}
                        </strong>
                        <small>
                          {t(
                            source === "native"
                              ? "terminal.direct.nativeModelsHelp"
                              : "terminal.direct.providerModelsHelp",
                          )}
                        </small>
                      </span>
                    </label>
                  ))}
                </div>
              )}

              {profile.model_source === "provider" && (
                <div className="terminal-direct-models-panel">
                  <div className="terminal-direct-models-head">
                    <strong>{t("terminal.direct.syncAllTitle")}</strong>
                    <span>{models.length}</span>
                    {models[0] && (
                      <small>{t("terminal.direct.defaultModelNote", { model: models[0] })}</small>
                    )}
                  </div>
                  <div className="terminal-direct-model-chips">
                    {models.map((model) => (
                      <code key={model} className="terminal-direct-model-chip">
                        {model}
                      </code>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
        )}

        {success && (
          <div className="terminal-message success" role="status">
            <Check size={15} />
            {success}
          </div>
        )}
        {backupPaths.length > 0 && (
          <details className="terminal-backups">
            <summary>{t("terminal.feedback.backups", { count: backupPaths.length })}</summary>
            {backupPaths.map((path) => (
              <code key={path}>{path}</code>
            ))}
          </details>
        )}
        {applyError && (
          <div className="terminal-message error" role="alert">
            <AlertTriangle size={15} />
            {applyError}
          </div>
        )}
      </section>

      <section className="terminal-preview-section">
        <div className="terminal-preview-heading">
          <div>
            <h2>{t("terminal.preview.heading")}</h2>
            <p>{t("terminal.direct.previewHint")}</p>
          </div>
        </div>
        {preview.preview && preview.preview.files.length > 1 && (
          <div className="terminal-file-tabs" role="group" aria-label={t("terminal.preview.filesLabel")}>
            {preview.preview.files.map((file, index) => (
              <button
                type="button"
                key={file.path}
                aria-pressed={Math.min(fileIndex, preview.preview!.files.length - 1) === index}
                onClick={() => setFileIndex(index)}
              >
                {file.path.split(/[\\/]/).pop()}
              </button>
            ))}
          </div>
        )}
        <pre className="terminal-preview">
          <code>
            {preview.pending
              ? t("terminal.preview.generating")
              : preview.error || currentFile?.content || t("terminal.direct.chooseProvider")}
          </code>
        </pre>
      </section>
    </div>
  );
}
