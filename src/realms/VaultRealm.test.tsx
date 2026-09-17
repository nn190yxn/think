import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { VaultRealm } from "./VaultRealm";
import { IpcProvider } from "../app/ipc";

function renderRealm() {
  return render(
    <IpcProvider>
      <VaultRealm />
    </IpcProvider>,
  );
}

describe("藏境界 · 知识地形", () => {
  it("默认展示大师架子，可切换到知识地形", async () => {
    renderRealm();
    expect(await screen.findByRole("tab", { name: "大师架子" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByText("层次覆盖")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "知识地形" }));
    expect(await screen.findByText("主题分布")).toBeInTheDocument();
    expect(screen.getByText("年轮")).toBeInTheDocument();
  });

  it("知识地形列出来源与主题，离线来源保留且标记", async () => {
    renderRealm();
    await userEvent.click(await screen.findByRole("tab", { name: "知识地形" }));

    const sources = await screen.findAllByText(/D:\/笔记|E:\/阅读/);
    expect(sources.length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("离线 · 2 篇")).toBeInTheDocument();
    // 注意力经济有两个版本，主题计数为 2。
    expect(screen.getAllByText("注意力经济").length).toBeGreaterThan(0);
    expect(screen.getByText("主题", { selector: "dt" })).toBeInTheDocument();
  });

  it("可登记来源并检索主题", async () => {
    renderRealm();
    await userEvent.click(await screen.findByRole("tab", { name: "知识地形" }));

    const input = await screen.findByRole("textbox", { name: "知识库来源路径" });
    await userEvent.type(input, "D:/新资料");
    await userEvent.click(screen.getByRole("button", { name: "登记" }));
    expect(await screen.findByText("D:/新资料")).toBeInTheDocument();

    const searchBox = screen.getByRole("textbox", { name: "查找知识库" });
    await userEvent.type(searchBox, "规模");
    await userEvent.click(screen.getByRole("button", { name: "查找" }));
    await waitFor(() =>
      expect(screen.getByText("E:/阅读/规模.epub")).toBeInTheDocument(),
    );
  });
});

describe("藏境界 · 大师档案", () => {
  it("未选大师时提示先选一位", async () => {
    renderRealm();
    await userEvent.click(await screen.findByRole("tab", { name: "大师档案" }));
    expect(await screen.findByText("先选一位大师，再看他的完整档案。")).toBeInTheDocument();
  });

  it("点大师进入档案，展示技能、认知迭代与材料来源", async () => {
    renderRealm();
    const pick = await screen.findAllByRole("button", { name: /查看档案/ });
    await userEvent.click(pick[0]!);

    expect(await screen.findByRole("heading", { name: "稻盛和夫" })).toBeInTheDocument();
    expect(screen.getByText(/经营 · 道 \/ 气 · 可用/)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^技能/ })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "认知迭代" })).toBeInTheDocument();
    expect(screen.getByText("种子包首版")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "观点轨迹" })).toBeInTheDocument();
    expect(screen.getByText("这位大师还没有在会诊里发言过。")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "材料来源" })).toBeInTheDocument();
    expect(screen.getByText("稻盛和夫语料")).toBeInTheDocument();
  });

  it("六题档案列出六题，空缺题保留并标注", async () => {
    renderRealm();
    const pick = await screen.findAllByRole("button", { name: /查看档案/ });
    await userEvent.click(pick[0]!);

    expect(
      await screen.findByRole("heading", { name: /六题档案/ }),
    ).toBeInTheDocument();
    // 六题的核心问题都在，方便不同大师横向比较。
    for (const question of [
      "什么值得做",
      "规律是什么",
      "具体怎么做",
      "靠什么心力度过",
      "用什么载体放大",
      "现在是不是时候",
    ]) {
      expect(screen.getByText(question)).toBeInTheDocument();
    }
    // 稻盛和夫在道与气各有一条技能，其余四题空缺。
    expect(screen.getAllByText("这一题还没有积累")).toHaveLength(4);
    expect(screen.getAllByText("心性优先").length).toBeGreaterThan(0);
  });
});

describe("藏境界 · 资产图谱", () => {
  it("展示汇总、分类分布与 AI 能力", async () => {
    renderRealm();
    await userEvent.click(await screen.findByRole("tab", { name: "资产图谱" }));

    expect(await screen.findByText("Skill 总数")).toBeInTheDocument();
    expect(screen.getByText("覆盖领域")).toBeInTheDocument();
    // 演示数据：5 个 Skill 中 4 个启用。
    expect(screen.getByText("80%")).toBeInTheDocument();
    expect(screen.getByText("+3 / -1")).toBeInTheDocument();
    expect(screen.getAllByText("数据").length).toBeGreaterThan(0);
    expect(screen.getAllByText("已配置未启用").length).toBeGreaterThan(0);
  });

  it("待修复的 Skill 标注为裂纹陶片并可展开依赖详情", async () => {
    renderRealm();
    await userEvent.click(await screen.findByRole("tab", { name: "资产图谱" }));

    const repairing = await screen.findByRole("button", { name: /周报生成/ });
    expect(repairing).toHaveAttribute("data-repair", "true");
    await userEvent.click(repairing);
    expect(
      await screen.findByText("待修复：缺少 manifest.json 或 SKILL.md"),
    ).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /写作/ }));
    expect(await screen.findByText("typo-check")).toBeInTheDocument();
    expect(screen.getByText("0.3")).toBeInTheDocument();
  });

  it("可登记 Skill 目录，并可只看待修复", async () => {
    renderRealm();
    await userEvent.click(await screen.findByRole("tab", { name: "资产图谱" }));

    const input = await screen.findByRole("textbox", { name: "Skill 根目录路径" });
    await userEvent.type(input, "D:/新技能");
    await userEvent.click(screen.getByRole("button", { name: "登记" }));
    expect(await screen.findByText("D:/新技能")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("checkbox", { name: "只看待修复" }));
    expect(screen.getByRole("button", { name: /周报生成/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /写作/ })).not.toBeInTheDocument();
  });
});
