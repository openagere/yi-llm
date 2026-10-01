import type zhCN from "./locales/zh-CN";

/** 将字典叶子节点统一放宽为 string，便于其他语言复用同一类型。 */
export type DeepString<T> = { readonly [K in keyof T]: T[K] extends string ? string : DeepString<T[K]> };
export type Messages = DeepString<typeof zhCN>;

/** 以 zh-CN 的键路径为基准的联合类型，如 "settings.theme.heading"。 */
export type MessageKey = { [K in keyof Messages & string]: Messages[K] extends string ? K : MessageKeyOf<Messages[K], K> }[keyof Messages & string];
type MessageKeyOf<T, P extends string> = { [K in keyof T & string]: T[K] extends string ? `${P}.${K}` : MessageKeyOf<T[K], `${P}.${K}`> }[keyof T & string];

export type MessageParams = Record<string, string | number>;
