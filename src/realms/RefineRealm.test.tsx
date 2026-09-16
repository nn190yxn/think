import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { RefineRealm } from "./RefineRealm";
import { IpcProvider } from "../app/ipc";

function renderRealm() {
  return render(
    <IpcProvider>
      <RefineRealm />
    </IpcProvider>,
  );
}

describe("炼境界 · 蒸馏熔炉", () => {
  it("剖面炉体展示六阶段，当前阶段被点亮", async () => {
    renderRealm();
    expect(await screen.findByText("剖面炉体")).toBeInTheDocument();
    const active = document.querySelector('.pipeline__stage[data-state="active"]');
    expect(active).not.toBeNull();
    expect(active?.textContent).toContain("三重验证");
    expect(document.querySelectorAll('.pipeline__stage[data-state="done"]').length).toBe(2);
  });

  it("五路提取臂按轨道显示候选数", async () => {
    renderRealm();
    const framework = await screen.findByText("框架");
    await waitFor(() =>
      expect(document.querySelectorAll('.arm[data-lit="true"]').length).toBe(2),
    );
    expect(framework.closest(".arm")?.textContent).toContain("1");
  });

  it("三层筛网与弃料托盘列出淘汰原因", async () => {
    renderRealm();
    expect(await screen.findByText("弃料托盘")).toBeInTheDocument();
    expect(screen.getByText("无法回答材料未明说的新问题")).toBeInTheDocument();
    expect(
      screen.getByText("四要素不全：需要触发条件、执行步骤、作用机制与适用边界"),
    ).toBeInTheDocument();
  });

  it("技能锭强制展示四要素", async () => {
    renderRealm();
    expect(await screen.findByText("多元思维模型")).toBeInTheDocument();
    expect(screen.getByText("触发条件")).toBeInTheDocument();
    expect(screen.getByText("执行步骤")).toBeInTheDocument();
    expect(screen.getByText("作用机制")).toBeInTheDocument();
    expect(screen.getByText("适用边界")).toBeInTheDocument();
  });

  it("压力测试标注诱饵题与通过率", async () => {
    renderRealm();
    expect(await screen.findByText("诱饵")).toBeInTheDocument();
    expect(screen.getByText("100%")).toBeInTheDocument();
  });

  it("待确认清单逐条接受后不再阻塞蒸馏", async () => {
    renderRealm();
    const signals = await screen.findAllByText(/芒格|穷查理|Daily Journal/);
    expect(signals.length).toBeGreaterThan(0);
    await userEvent.click(screen.getAllByRole("button", { name: "接受" })[0]!);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "接受" })).not.toBeInTheDocument(),
    );
  });

  it("主动搜集默认关闭，可开启并检索一批", async () => {
    renderRealm();
    const toggle = await screen.findByRole("switch", { name: "主动搜集" });
    expect(toggle).toHaveAttribute("aria-checked", "false");

    await userEvent.click(toggle);
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "主动搜集" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "立即检索一批" }));
    await waitFor(() =>
      expect(screen.getByText("检索到 3 条，新增 3 条待确认")).toBeInTheDocument(),
    );
  });
});
