/** 跨功能共享文案：窗口、导航、通用按钮、共享 UI 组件与错误提示。 */
const common = {
  window: {
    titlebar: "窗口标题栏",
    minimize: "最小化",
    maximize: "最大化",
    restore: "还原窗口",
    close: "关闭窗口",
    failed: "窗口操作失败：",
  },
  nav: {
    closeSidebar: "收起侧栏",
    configuration: "配置",
    tools: "运行与工具",
    search: "搜索页面",
    clearSearch: "清空搜索",
    noResults: "没有匹配的页面",
    toggleSidebar: "展开或收起侧栏",
    toggleTheme: "切换浅色 / 深色外观",
    workspace: "工作区",
    providers: "连接",
    models: "模型管理",
    proxy: "代理运行",
    usage: "模型用量",
    terminal: "终端管理",
    terminalExpand: "展开或收起终端管理",
    settings: "设置",
    phase: {
      starting: "启动中",
      running: "运行中",
      stopping: "停止中",
      stopped: "已停止",
      error: "启动失败",
      unknown: "状态未知",
    },
    discard: {
      title: "放弃未保存的修改？",
      description: "当前未保存的修改将被丢弃，此操作无法撤销。",
      confirm: "放弃修改",
    },
  },
  pageDescriptions: {
    providers: "管理模型服务连接、协议转换和路由。",
    models: "维护标准模型及其上下文、模态和推理能力。",
    proxy: "管理本地代理服务，查看连接、请求和运行日志。",
    usage: "查看模型调用、Token 用量和请求记录。",
    terminal: "管理命令行客户端的代理与 Provider 直连配置。",
    settings: "管理外观、字体和界面语言。",
  },
  common: {
    back: "返回上一页",
    close: "关闭页面",
    save: "保存",
    saving: "保存中…",
    saved: "已保存",
    retry: "重试",
    opening: "正在打开…",
  },
  ui: {
    select: { placeholder: "请选择" },
    pagination: {
      previous: "上一页",
      next: "下一页",
      total: "（共 {total} {unit}）",
      pageOf: "第 {current} 页，共 {pages} 页",
    },
    dialog: {
      cancel: "取消",
      deleting: "删除中…",
    },
    toast: { dismiss: "关闭提示" },
    pageHeader: { actions: "{title}操作" },
  },
  error: {
    generic: "发生错误，请检查配置后重试。",
  },
} as const;

export default common;
