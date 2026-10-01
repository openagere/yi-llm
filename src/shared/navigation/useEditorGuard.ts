import { useEffect } from "react";
import { useNavigationStore } from "./store";

/**
 * 编辑页向导航层上报状态：`dirty` 时离开需确认，`busy`（保存/应用中）时禁止离开。
 * 组件卸载时自动复位。
 */
export function useEditorGuard(dirty: boolean, busy = false): void {
  const setDirty = useNavigationStore((state) => state.setDirty);
  const setBusy = useNavigationStore((state) => state.setBusy);
  useEffect(() => {
    setDirty(dirty);
    return () => setDirty(false);
  }, [dirty, setDirty]);
  useEffect(() => {
    setBusy(busy);
    return () => setBusy(false);
  }, [busy, setBusy]);
}
