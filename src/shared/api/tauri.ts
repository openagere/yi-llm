import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export type ErrorCode = "validation" | "not_found" | "conflict" | "database" | "io" | "upstream" | "internal";

const ERROR_CODES: readonly ErrorCode[] = ["validation", "not_found", "conflict", "database", "io", "upstream", "internal"];

/** 后端命令统一返回 `{ code, message }`；其余异常（IPC 故障、字符串错误）归一为 internal。 */
export class AppError extends Error {
  readonly code: ErrorCode;

  constructor(code: ErrorCode, message: string) {
    super(message);
    this.name = "AppError";
    this.code = code;
  }
}

/**
 * 命令表：由各 feature 的 api.ts 通过 module augmentation 注册，例如
 * `declare module "@/shared/api/tauri" { interface CommandMap { list_providers: { args: void; result: ProviderView[] } } }`
 */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface CommandMap {}

export type CommandName = keyof CommandMap;
type CommandArgs<K extends CommandName> = CommandMap[K] extends { args: infer A } ? A : void;
type CommandResult<K extends CommandName> = CommandMap[K] extends { result: infer R } ? R : never;

export function toAppError(reason: unknown): AppError {
  if (reason instanceof AppError) return reason;
  if (typeof reason === "object" && reason !== null) {
    const { code, message } = reason as { code?: unknown; message?: unknown };
    if (typeof message === "string") {
      const known = ERROR_CODES.find((candidate) => candidate === code);
      return new AppError(known ?? "internal", message);
    }
  }
  if (typeof reason === "string") return new AppError("internal", reason);
  return new AppError("internal", String(reason));
}

export async function invoke<K extends CommandName>(
  command: K,
  ...args: CommandArgs<K> extends void ? [] : [CommandArgs<K>]
): Promise<CommandResult<K>> {
  try {
    return await tauriInvoke<CommandResult<K>>(command, args[0] as Record<string, unknown> | undefined);
  } catch (reason) {
    throw toAppError(reason);
  }
}
