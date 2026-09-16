/**
 * 前后端 command 边界。Rust 侧一律返回 CommandResult，
 * 前端只在这里理解这个形状，其他模块拿到的是已解包的数据或抛出typed错误。
 */

export const COMMAND_ERROR_CODES = [
  "E_INVALID_INPUT",
  "E_NOT_FOUND",
  "E_DB",
  "E_MIGRATION",
  "E_IO",
  "E_PACK_INVALID",
  "E_NETWORK_OFF",
  "E_MODEL_UNAVAILABLE",
  "E_MALFORMED_RESPONSE",
  "E_UNKNOWN",
] as const;

export type CommandErrorCode = (typeof COMMAND_ERROR_CODES)[number];

export interface CommandOk<T> {
  readonly ok: true;
  readonly data: T;
}

export interface CommandErr {
  readonly ok: false;
  readonly code: CommandErrorCode;
  readonly message: string;
  readonly detail?: string;
}

export type CommandResult<T> = CommandOk<T> | CommandErr;

const KNOWN_CODES: ReadonlySet<string> = new Set(COMMAND_ERROR_CODES);

export function isCommandErrorCode(value: unknown): value is CommandErrorCode {
  return typeof value === "string" && KNOWN_CODES.has(value);
}

/** 校验来自 IPC 的原始值确实是约定形状，避免把脏数据当成业务数据。 */
export function isCommandResult(value: unknown): value is CommandResult<unknown> {
  if (typeof value !== "object" || value === null || !("ok" in value)) {
    return false;
  }
  const candidate = value as { ok: unknown; code?: unknown; message?: unknown };
  if (candidate.ok === true) {
    return "data" in value;
  }
  if (candidate.ok === false) {
    return isCommandErrorCode(candidate.code) && typeof candidate.message === "string";
  }
  return false;
}

/** 借道 IPC 传递的错误，保留机器可判别的错误码。 */
export class CommandInvocationError extends Error {
  readonly code: CommandErrorCode;
  readonly detail: string | undefined;

  constructor(code: CommandErrorCode, message: string, detail?: string) {
    super(message);
    this.name = "CommandInvocationError";
    this.code = code;
    this.detail = detail;
  }
}
