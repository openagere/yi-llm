import type { FontId } from "../settings/types";

/** 界面字体栈；新增字体时在此登记 id 与 CSS font-family 栈。 */
export const FONT_STACKS: Record<FontId, string> = {
  system: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif',
  serif: 'Georgia, "Times New Roman", "Songti SC", "SimSun", "Noto Serif CJK SC", serif',
  mono: 'ui-monospace, SFMono-Regular, "SF Mono", Consolas, "Liberation Mono", Menlo, monospace',
};
