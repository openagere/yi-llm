/** 终端接入页文案。 */
const terminal = {
  terminal: {
    header: {
      eyebrow: "终端连接",
      title: "终端接入",
      closeLabel: "关闭终端接入",
    },
    tabs: {
      label: "终端类型",
      pendingTitle: "{name} · 有未应用的修改",
      connectedTitle: "{name} · 已接入",
      disconnectedTitle: "{name} · 未接入",
    },
    status: {
      loading: "读取中",
      connected: "已接入",
      disconnected: "未接入",
      configFile: "配置文件",
    },
    loadError: {
      message: "配置读取失败：",
    },
    client: {
      minVersionTitle: "原生自定义模型列表最低版本",
    },
    models: {
      heading: "模型集合",
      expandAll: "展开全部 Provider",
      collapseAll: "收起全部 Provider",
      selectAll: "全选",
      selectMatches: "全选匹配",
      clear: "清空",
      searchLabel: "搜索终端模型",
      searchPlaceholder: "搜索 Provider 或模型",
      clearSearchTitle: "清空搜索",
      clearSearchLabel: "清空模型搜索",
      loading: "正在读取模型",
      unavailable: "模型列表暂不可用",
      noMatch: "没有匹配的模型",
      noneEnabled: "暂无支持该协议的已启用模型",
      defaultBadge: "默认",
    },
    group: {
      select: "选择 {name} 全选匹配模型",
      expandLabel: "展开 {name} 模型",
      collapseLabel: "收起 {name} 模型",
      expandTitle: "展开模型",
      collapseTitle: "收起模型",
    },
    stale: {
      heading: "已失效的模型",
      removeTitle: "移除失效模型",
      removeLabel: "移除 {model}",
    },
    defaultModel: {
      label: "默认模型",
      select: "选择终端默认模型",
      placeholder: "先选择模型",
    },
    actions: {
      dirty: "有未应用的修改",
      summary: "{models} 个模型 · {providers} 个 Providers",
      applying: "正在应用",
      update: "更新配置",
      apply: "应用配置",
    },
    feedback: {
      applied: "{name} 配置已更新 · {count} 个模型 · 待终端重启",
      clipboardUnavailable: "无法访问剪贴板",
      backups: "配置备份 ·{count} 个文件",
    },
    preview: {
      heading: "配置预览",
      copyTitle: "复制配置",
      copyLabel: "复制终端配置",
      filesLabel: "配置文件",
      generating: "正在生成预览…",
      removeStale: "请移除已失效的模型",
      empty: "尚未选择模型",
    },
  },
} as const;

export default terminal;
