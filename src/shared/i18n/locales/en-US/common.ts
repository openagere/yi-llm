import type { DeepString } from "../../types";
import type zh from "../zh-CN/common";

const common: DeepString<typeof zh> = {
  window: {
    titlebar: "Window title bar",
    minimize: "Minimize",
    maximize: "Maximize",
    restore: "Restore window",
    close: "Close window",
    failed: "Window operation failed: ",
  },
  nav: {
    closeSidebar: "Close sidebar",
    configuration: "Configuration",
    tools: "Runtime & tools",
    search: "Search pages",
    clearSearch: "Clear search",
    noResults: "No matching pages",
    toggleSidebar: "Toggle sidebar",
    toggleTheme: "Toggle light / dark appearance",
    workspace: "Workspace",
    providers: "Providers",
    models: "Models",
    proxy: "Proxy",
    usage: "Usage",
    terminal: "Terminal management",
    terminalExpand: "Expand or collapse terminal management",
    settings: "Settings",
    phase: {
      starting: "Starting",
      running: "Running",
      stopping: "Stopping",
      stopped: "Stopped",
      error: "Failed",
      unknown: "Unknown",
    },
    discard: {
      title: "Discard unsaved changes?",
      description: "Your unsaved changes will be discarded. This can't be undone.",
      confirm: "Discard changes",
    },
  },
  pageDescriptions: {
    providers: "Manage model providers, protocol conversion, and routing.",
    models: "Manage standard models, context limits, modalities, and reasoning.",
    proxy: "Manage the local proxy, connections, requests, and logs.",
    usage: "Review model calls, token usage, and request records.",
    terminal: "Manage proxy and direct Provider access for command-line clients.",
    settings: "Customize appearance, fonts, and interface language.",
  },
  common: {
    back: "Back",
    close: "Close page",
    save: "Save",
    saving: "Saving…",
    saved: "Saved",
    retry: "Retry",
    opening: "Opening…",
  },
  ui: {
    select: { placeholder: "Select an option" },
    pagination: {
      previous: "Previous page",
      next: "Next page",
      total: " (total {total} {unit})",
      pageOf: "Page {current} of {pages}",
    },
    dialog: {
      cancel: "Cancel",
      deleting: "Deleting…",
    },
    toast: { dismiss: "Dismiss notification" },
    pageHeader: { actions: "{title} actions" },
  },
  error: {
    generic: "An error occurred. Check the configuration and try again.",
  },
};

export default common;
