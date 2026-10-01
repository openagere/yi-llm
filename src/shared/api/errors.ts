import { currentLanguage, translate } from "../i18n";
import { toAppError } from "./tauri";

const HAN = /[\p{Script=Han}]/u;

/**
 * 面向用户的错误文案。后端错误消息目前为中文，非中文界面下无法翻译时回落到通用提示，
 * 避免中英混杂；不含汉字的消息（如 IPC 层错误）原样展示。
 */
export function describeError(reason: unknown): string {
  const { message } = toAppError(reason);
  return currentLanguage() !== "zh-CN" && HAN.test(message) ? translate("error.generic") : message;
}
