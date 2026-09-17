import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createCommandClient, stubTransport, type CommandClient } from "../ipc/client";
import { tauriTransport, isDesktopShell } from "../ipc/tauriTransport";
import { CommandInvocationError } from "../ipc/protocol";
import type { CommandName, CommandRequest, CommandResponse } from "../ipc/commands";

const IpcContext = createContext<CommandClient | null>(null);

export function IpcProvider({
  children,
  client,
}: {
  children: ReactNode;
  /** 测试可以注入自定义客户端，覆盖个别命令的返回值。 */
  client?: CommandClient;
}) {
  const fallback = useMemo(
    () => createCommandClient(isDesktopShell() ? tauriTransport : stubTransport),
    [],
  );
  return <IpcContext.Provider value={client ?? fallback}>{children}</IpcContext.Provider>;
}

export function useCommands(): CommandClient {
  const client = useContext(IpcContext);
  if (!client) {
    throw new Error("useCommands 必须在 IpcProvider 内使用");
  }
  return client;
}

export interface QueryState<T> {
  readonly data: T | null;
  readonly error: CommandInvocationError | null;
  readonly pending: boolean;
}

/** 命令式取数：挂载时调用一次，卸载后不再写入状态。 */
export function useCommand<K extends CommandName>(
  name: K,
  request: CommandRequest<K>,
): QueryState<CommandResponse<K>> {
  const client = useCommands();
  const [state, setState] = useState<QueryState<CommandResponse<K>>>({
    data: null,
    error: null,
    pending: true,
  });
  const requestRef = useRef(request);
  requestRef.current = request;

  useEffect(() => {
    let alive = true;
    setState((prev) => ({ ...prev, pending: true }));
    client
      .call(name, requestRef.current)
      .then((data) => {
        if (alive) {
          setState({ data, error: null, pending: false });
        }
      })
      .catch((cause: unknown) => {
        if (!alive) {
          return;
        }
        const error =
          cause instanceof CommandInvocationError
            ? cause
            : new CommandInvocationError("E_UNKNOWN", "命令执行失败");
        setState({ data: null, error, pending: false });
      });
    return () => {
      alive = false;
    };
  }, [client, name]);

  return state;
}
