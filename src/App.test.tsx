import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { App } from "./App";
import { IpcProvider } from "./app/ipc";

function renderApp() {
  return render(
    <IpcProvider>
      <App />
    </IpcProvider>,
  );
}

describe("应用外壳", () => {
  beforeEach(() => {
    window.history.replaceState(null, "", "#/observe");
  });

  it("默认落在观境界", async () => {
    renderApp();
    expect(await screen.findByRole("heading", { name: "思维星图" })).toBeInTheDocument();
  });

  it("炉温可被辅助技术读取", async () => {
    renderApp();
    const meter = await screen.findByRole("meter");
    expect(meter).toHaveAttribute("aria-valuemin", "0");
    expect(meter).toHaveAttribute("aria-valuemax", "100");
    expect(meter.getAttribute("aria-label")).toMatch(/炉温 \d+ 度/);
  });

  it("炉温统计来自后端快照", async () => {
    renderApp();
    await waitFor(() => {
      expect(screen.getByText("11")).toBeInTheDocument();
    });
    expect(screen.getByText("10")).toBeInTheDocument();
  });

  it("切换境界只换主体内容", async () => {
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /圆桌会诊/ }));
    expect(await screen.findByRole("heading", { name: "圆桌会诊" })).toBeInTheDocument();
    // 六个席位按层次各一位
    expect(document.querySelectorAll(".seat")).toHaveLength(6);
  });

  it("切换主题改变根元素令牌且不改变结构", async () => {
    renderApp();
    const before = document.querySelectorAll(".seat").length;
    await userEvent.click(screen.getAllByRole("button", { name: "素瓷" })[0]!);
    expect(document.documentElement.dataset["theme"]).toBe("suci");
    expect(document.querySelectorAll(".seat").length).toBe(before);
  });

  it("三档选角策略只允许一档生效", async () => {
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /圆桌会诊/ }));
    const clash = await screen.findByRole("button", { name: "碰撞" });
    await userEvent.click(clash);
    expect(clash).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "稳妥" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.getByRole("button", { name: "意外" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("藏境界显示六层覆盖矩阵", async () => {
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /大师与资产/ }));
    await screen.findByRole("heading", { name: "层次覆盖" });
    const cells = document.querySelectorAll(".coverage__cell");
    expect(cells).toHaveLength(6);
    expect([...cells].map((cell) => cell.getAttribute("data-layer"))).toEqual([
      "dao",
      "fa",
      "shu",
      "qi",
      "tool",
      "shi",
    ]);
    // 种子大师包六层各一位，预览数据下不应出现空缺。
    await waitFor(() => {
      expect(document.querySelectorAll('.coverage__cell[data-empty="true"]')).toHaveLength(0);
    });
    expect(await screen.findByText("稻盛和夫")).toBeInTheDocument();
  });

  it("我境界显示运行信息", async () => {
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /成长与设置/ }));
    await userEvent.click(await screen.findByRole("tab", { name: "系统" }));
    expect(await screen.findByText("数据格式版本")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText("memory")).toBeInTheDocument();
    });
  });

  it("设置页默认关闭联网，并可逐项启用模型平台", async () => {
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: /成长与设置/ }));
    await userEvent.click(await screen.findByRole("tab", { name: "系统" }));
    const toggle = await screen.findByRole("switch", { name: "联网能力" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-checked", "true");
    expect(await screen.findByText(/本地模型/)).toBeInTheDocument();
    expect(await screen.findByText(/联网能力没有开启/)).toBeInTheDocument();
  });

  it("联网关闭时炉温环外显示虚线离线态，本机内容仍可读", async () => {
    renderApp();
    expect(await screen.findByRole("status")).toHaveTextContent(
      "离线运行 · 已装大师、记录与网络仍可读可搜",
    );
    expect(document.querySelector('.furnace[data-offline="true"]')).not.toBeNull();
    // 离线不影响本机内容：星图与记录保持可读。
    expect(await screen.findByRole("heading", { name: "思维星图" })).toBeInTheDocument();
  });

  it("Ctrl K 唤出命令面板并可跳转境界", async () => {
    renderApp();
    await screen.findByRole("heading", { name: "思维星图" });
    await userEvent.keyboard("{Control>}k{/Control}");
    const dialog = await screen.findByRole("dialog", { name: "命令面板" });
    expect(dialog).toBeInTheDocument();

    await userEvent.click(screen.getByRole("option", { name: /前往圆桌会诊/ }));
    expect(await screen.findByRole("heading", { name: "圆桌会诊" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "命令面板" })).toBeNull();
  });
});
