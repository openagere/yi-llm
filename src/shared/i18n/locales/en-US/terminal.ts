import type { DeepString } from "../../types";
import type zh from "../zh-CN/terminal";

const terminal: DeepString<typeof zh> = {
  terminal: {
    header: {
      eyebrow: "Terminal connection",
      title: "Terminal integration",
      closeLabel: "Close terminal integration",
    },
    tabs: {
      label: "Client type",
      pendingTitle: "{name} · Changes not applied",
      connectedTitle: "{name} · Connected",
      disconnectedTitle: "{name} · Not connected",
    },
    status: {
      loading: "Loading",
      connected: "Connected",
      disconnected: "Not connected",
      configFile: "Configuration file",
    },
    loadError: {
      message: "Failed to load configuration: ",
    },
    client: {
      minVersionTitle: "Minimum version for native custom model lists",
    },
    models: {
      heading: "Model group",
      expandAll: "Expand all providers",
      collapseAll: "Collapse all providers",
      selectAll: "Select all",
      selectMatches: "Select all matches",
      clear: "Clear",
      searchLabel: "Search terminal models",
      searchPlaceholder: "Search providers or models",
      clearSearchTitle: "Clear search",
      clearSearchLabel: "Clear model search",
      loading: "Loading models",
      unavailable: "Model list unavailable",
      noMatch: "No matching models",
      noneEnabled: "No enabled models support this protocol",
      defaultBadge: "Default",
    },
    group: {
      select: "Select all matching models from {name}",
      expandLabel: "Expand {name} models",
      collapseLabel: "Collapse {name} models",
      expandTitle: "Expand models",
      collapseTitle: "Collapse models",
    },
    stale: {
      heading: "Unavailable models",
      removeTitle: "Remove unavailable models",
      removeLabel: "Remove {model}",
    },
    defaultModel: {
      label: "Default model",
      select: "Select terminal default model",
      placeholder: "Select a model first",
    },
    actions: {
      dirty: "Unapplied changes",
      summary: "{models} models · {providers} providers",
      applying: "Applying",
      update: "Update configuration",
      apply: "Apply configuration",
    },
    feedback: {
      applied: "{name} configuration updated · {count} models · terminal restart required",
      clipboardUnavailable: "Clipboard access unavailable",
      backups: "Configuration backup · {count} files",
    },
    preview: {
      heading: "Configuration preview",
      copyTitle: "Copy configuration",
      copyLabel: "Copy terminal configuration",
      filesLabel: "Configuration file",
      generating: "Generating preview…",
      removeStale: "Remove unavailable models",
      empty: "No model selected",
    },
  },
};

export default terminal;
