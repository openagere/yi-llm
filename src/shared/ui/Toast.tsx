import { useEffect } from "react";
import { CheckCircle2, CircleAlert, Info, X } from "lucide-react";
import { useI18n } from "@/shared/i18n";
import { useNotificationStore } from "@/shared/notify";

/** 全局提示条：渲染 notify() 推送的最新一条通知，非错误提示 4 秒后自动消失。 */
export function Toast() {
  const { t } = useI18n();
  const notification = useNotificationStore((state) => state.current);
  const dismiss = useNotificationStore((state) => state.dismiss);

  useEffect(() => {
    if (!notification || notification.kind === "error") return;
    const timer = window.setTimeout(dismiss, 4000);
    return () => window.clearTimeout(timer);
  }, [notification, dismiss]);

  if (!notification) return null;
  const Icon = notification.kind === "error" ? CircleAlert : notification.kind === "success" ? CheckCircle2 : Info;
  return <div key={notification.id} className={`app-toast ${notification.kind}`} role={notification.kind === "error" ? "alert" : "status"}>
    <Icon size={18} /><span>{notification.message}</span>
    <button type="button" title={t("ui.toast.dismiss")} aria-label={t("ui.toast.dismiss")} onClick={dismiss}><X size={16} /></button>
  </div>;
}
