import type { Messages } from "../../types";
import common from "./common";
import model from "./model";
import provider from "./provider";
import proxy from "./proxy";
import settings from "./settings";
import terminal from "./terminal";
import usage from "./usage";

const enUS: Messages = {
  ...common,
  ...settings,
  ...proxy,
  ...provider,
  ...model,
  ...usage,
  ...terminal,
};

export default enUS;
