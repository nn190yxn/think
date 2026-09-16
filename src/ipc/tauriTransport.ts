import { invoke } from "@tauri-apps/api/core";
import type { CommandTransport } from "./client";

/** Tauri 运行期的真实传输层。浏览器开发模式下不会走到这里。 */
export const tauriTransport: CommandTransport = {
  invoke(name, request) {
    return invoke(name, request as Record<string, unknown>);
  },
};

/** 是否运行在 Tauri 外壳内。 */
export function isDesktopShell(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
