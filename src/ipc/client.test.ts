import { describe, expect, it, vi } from "vitest";
import { createCommandClient, type CommandTransport } from "./client";
import { CommandInvocationError } from "./protocol";

function transportReturning(value: unknown): CommandTransport {
  return { invoke: vi.fn(async () => value) };
}

describe("command 客户端", () => {
  it("成功时直接返回数据", async () => {
    const client = createCommandClient(
      transportReturning({ ok: true, data: { name: "思想熔炉" } }),
    );
    const info = await client.call("app_info", {});
    expect(info).toEqual({ name: "思想熔炉" });
  });

  it("把错误结果转成带错误码的异常", async () => {
    const client = createCommandClient(
      transportReturning({ ok: false, code: "E_DB", message: "库损坏" }),
    );
    await expect(client.call("db_status", {})).rejects.toMatchObject({
      name: "CommandInvocationError",
      code: "E_DB",
      message: "库损坏",
    });
  });

  it("保留 detail 字段", async () => {
    const client = createCommandClient(
      transportReturning({ ok: false, code: "E_IO", message: "读写失败", detail: "/tmp/x" }),
    );
    await expect(client.call("db_status", {})).rejects.toMatchObject({ detail: "/tmp/x" });
  });

  it("传输层抛错时归一化错误码", async () => {
    const client = createCommandClient({
      invoke: vi.fn(async () => {
        throw "E_NOT_FOUND";
      }),
    });
    await expect(client.call("master_list", {})).rejects.toMatchObject({
      code: "E_NOT_FOUND",
    });
  });

  it("未知抛出物落到 E_UNKNOWN", async () => {
    const client = createCommandClient({
      invoke: vi.fn(async () => {
        throw new Error("boom");
      }),
    });
    await expect(client.call("master_list", {})).rejects.toMatchObject({
      code: "E_UNKNOWN",
      message: "boom",
    });
  });

  it("拒绝不符合约定的返回值", async () => {
    const client = createCommandClient(transportReturning({ hello: "world" }));
    const error = await client.call("app_info", {}).catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(CommandInvocationError);
    expect((error as CommandInvocationError).code).toBe("E_MALFORMED_RESPONSE");
  });

  it("把请求原样交给传输层", async () => {
    const invoke = vi.fn(async () => ({ ok: true, data: null }));
    const client = createCommandClient({ invoke });
    await client.call("settings_set", { key: "theme", value: "suci" });
    expect(invoke).toHaveBeenCalledWith("settings_set", { key: "theme", value: "suci" });
  });
});
