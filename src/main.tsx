import React from "react";
import ReactDOM from "react-dom/client";
import App from "./app/App";
import { AppProviders } from "./app/providers";
// 设置 store 在模块加载时即应用主题与字体，保证首帧无闪烁。
import "@/shared/settings";
import "./styles/base.css";
import "./styles/workspace.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppProviders>
      <App />
    </AppProviders>
  </React.StrictMode>,
);
