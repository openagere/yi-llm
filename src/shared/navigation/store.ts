import { create } from "zustand";
import { dismissNotification } from "../notify";

/** 应用页面标识与导航类型定义。 */
export type Page =
  | "provider-list"
  | "provider-editor"
  | "model-list"
  | "model-editor"
  | "proxy"
  | "usage"
  | "terminal-codex"
  | "terminal-claude-code"
  | "terminal-opencode"
  | "terminal-pi"
  | "terminal-deepseek-harness"
  | "settings";

/** 终端子页面与客户端的对应关系；侧边栏终端分组按此展开。 */
export const TERMINAL_PAGE_CLIENTS = {
  "terminal-codex": "codex",
  "terminal-claude-code": "claude-code",
  "terminal-opencode": "opencode",
  "terminal-pi": "pi",
  "terminal-deepseek-harness": "deepseek-harness",
} as const;
export type TerminalPageId = keyof typeof TERMINAL_PAGE_CLIENTS;
export type MainPage = Exclude<Page, "provider-editor" | "model-editor">;
export type EditorPage = Extract<Page, "provider-editor" | "model-editor">;

const COMPACT_QUERY = "(max-width: 760px)";
const isCompact = () => window.matchMedia(COMPACT_QUERY).matches;

interface NavigationState {
  page: Page;
  history: MainPage[];
  sidebarCollapsed: boolean;
  /** 代理页首次访问后保持挂载，以保留其本地状态。 */
  proxyVisited: boolean;
  dirty: boolean;
  busy: boolean;
  pendingDiscard: (() => void) | null;
  discardDraft: (() => void) | null;

  /** 执行离开编辑器的动作：忙碌时忽略，有未保存修改时先弹出确认。 */
  guarded: (action: () => void) => void;
  navigate: (next: MainPage) => void;
  back: () => void;
  close: () => void;
  /** 进入编辑页；`prepare` 在确认离开后、切页前执行（如设置选中项）。 */
  openEditor: (page: EditorPage, prepare?: () => void) => void;
  /** 保存/删除成功后无条件跳转（跳过未保存修改确认）。 */
  goTo: (page: Page) => void;
  setDirty: (dirty: boolean) => void;
  setBusy: (busy: boolean) => void;
  setDiscardDraft: (discard: (() => void) | null) => void;
  confirmDiscard: () => void;
  cancelDiscard: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebar: () => void;
}

export const useNavigationStore = create<NavigationState>((set, get) => ({
  page: "provider-list",
  history: [],
  sidebarCollapsed: typeof window !== "undefined" && isCompact(),
  proxyVisited: false,
  dirty: false,
  busy: false,
  pendingDiscard: null,
  discardDraft: null,

  guarded: (action) => {
    const { busy, dirty, pendingDiscard } = get();
    if (busy || pendingDiscard) return;
    if (dirty) {
      set({ pendingDiscard: action });
      return;
    }
    action();
  },

  navigate: (next) => {
    const current = get().page;
    if (next === current || get().busy) return;
    const switchTerminal = current in TERMINAL_PAGE_CLIENTS && next in TERMINAL_PAGE_CLIENTS;
    const move = () => {
      const { page, history } = get();
      set({
        dirty: switchTerminal ? get().dirty : false,
        history: page === "provider-editor" || page === "model-editor" ? history : [...history, page],
        proxyVisited: get().proxyVisited || next === "proxy",
        page: next,
        sidebarCollapsed: isCompact() ? true : get().sidebarCollapsed,
      });
    };
    // Terminal drafts are shared across child pages, so switching clients loses no edits.
    if (switchTerminal) move();
    else get().guarded(move);
  },

  back: () => {
    get().guarded(() => {
      const { page, history } = get();
      if (page === "provider-editor") {
        set({ dirty: false, page: "provider-list" });
        return;
      }
      if (page === "model-editor") {
        set({ dirty: false, page: "model-list" });
        return;
      }
      set({
        dirty: false,
        page: history[history.length - 1] ?? "provider-list",
        history: history.slice(0, -1),
      });
    });
  },

  close: () => get().navigate("provider-list"),

  openEditor: (page, prepare) => {
    get().guarded(() => {
      prepare?.();
      dismissNotification();
      set({ dirty: false, page });
    });
  },

  goTo: (page) => set({ dirty: false, page }),
  setDirty: (dirty) => set({ dirty }),
  setBusy: (busy) => set({ busy }),
  setDiscardDraft: (discardDraft) => set({ discardDraft }),
  confirmDiscard: () => {
    const { pendingDiscard, discardDraft } = get();
    if (!pendingDiscard) return;
    discardDraft?.();
    set({ dirty: false, pendingDiscard: null });
    pendingDiscard();
  },
  cancelDiscard: () => set({ pendingDiscard: null }),
  setSidebarCollapsed: (sidebarCollapsed) => set({ sidebarCollapsed }),
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
}));
