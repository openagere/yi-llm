const settings = {
  settings: {
    eyebrow: "偏好设置",
    title: "设置",
    autoSaved: "更改会自动保存到本机，无需代理运行。",
    theme: {
      heading: "外观",
      description: "选择界面配色方案，可跟随系统外观自动切换。",
      light: "浅色",
      lightDesc: "明亮的默认界面",
      dark: "深色",
      darkDesc: "低亮度，适合夜间使用",
      system: "跟随系统",
      systemDesc: "随操作系统的明暗设置自动切换",
    },
    font: {
      heading: "字体",
      description: "选择界面文字字体。",
      system: "系统默认",
      systemDesc: "无衬线字体，界面最常用",
      serif: "衬线",
      serifDesc: "Georgia 等衬线字体",
      mono: "等宽",
      monoDesc: "代码风格等宽字体",
      preview: "Aa 中文 0123",
    },
    language: {
      heading: "语言",
      description: "选择界面显示语言。",
      "zh-CN": "简体中文",
      "en-US": "English",
    },
  },
} as const;

export default settings;
