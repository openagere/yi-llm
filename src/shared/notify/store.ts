import { create } from "zustand";

export type NotificationKind = "success" | "error" | "info";

export interface Notification {
  id: number;
  message: string;
  kind: NotificationKind;
}

interface NotificationState {
  current: Notification | null;
  push: (message: string, kind?: NotificationKind) => void;
  dismiss: () => void;
}

let nextId = 0;

export const useNotificationStore = create<NotificationState>((set) => ({
  current: null,
  push: (message, kind = "success") => set({ current: { id: ++nextId, message, kind } }),
  dismiss: () => set({ current: null }),
}));

/** 弹出全局提示；可在组件、mutation 回调与 QueryCache 中直接调用。 */
export function notify(message: string, kind: NotificationKind = "success"): void {
  useNotificationStore.getState().push(message, kind);
}

export function dismissNotification(): void {
  useNotificationStore.getState().dismiss();
}
