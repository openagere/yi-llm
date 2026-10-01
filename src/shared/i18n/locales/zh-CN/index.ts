import common from "./common";
import model from "./model";
import provider from "./provider";
import proxy from "./proxy";
import settings from "./settings";
import terminal from "./terminal";
import usage from "./usage";

/** 简体中文（默认语言）：所有文案以 zh-CN 为基准结构，其他语言必须与之键完全一致。 */
const zhCN = {
  ...common,
  ...settings,
  ...proxy,
  ...provider,
  ...model,
  ...usage,
  ...terminal,
} as const;

export default zhCN;
