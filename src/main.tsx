import React from "react";
import ReactDOM from "react-dom/client";
import { isTauri } from "@tauri-apps/api/core";
import App from "./app/App";
import { AppProviders } from "./app/providers";
import { detectPlatform } from "@/shared/lib";
// 设置 store 在模块加载时即应用主题与字体，保证首帧无闪烁。
import "@/shared/settings";
import "./styles/base.css";
import "./styles/workspace.css";

// 首帧渲染前标记当前平台，样式与组件据此差异化（如 macOS 系统标题栏适配）；浏览器开发环境固定为 "web"。
document.documentElement.dataset.platform = isTauri() ? detectPlatform() : "web";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppProviders>
      <App />
    </AppProviders>
  </React.StrictMode>,
);
