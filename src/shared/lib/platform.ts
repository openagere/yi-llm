export type AppPlatform = "macos" | "windows" | "linux" | "web";

/**
 * 通过 User-Agent 识别当前运行的操作系统。
 * Tauri 桌面端 WebView 的 UA 会携带系统标识；无法识别时返回 "web"。
 */
export function detectPlatform(): AppPlatform {
  if (typeof navigator === "undefined") return "web";
  const ua = navigator.userAgent;
  if (/iphone|ipad|ipod/i.test(ua)) return "web";
  if (/mac os x|macintosh/i.test(ua)) return "macos";
  if (/windows nt/i.test(ua)) return "windows";
  if (/android|\blinux\b/i.test(ua)) return "linux";
  return "web";
}
