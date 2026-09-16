import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { CouncilRealm } from "./CouncilRealm";
import { IpcProvider } from "../app/ipc";

function renderRealm() {
  return render(
    <IpcProvider>
      <CouncilRealm />
    </IpcProvider>,
  );
}

async function runCouncil() {
  renderRealm();
  await userEvent.type(screen.getByLabelText("议题"), "要不要换一条赛道");
  await userEvent.click(screen.getByRole("button", { name: "发起会诊" }));
  await waitFor(() =>
    expect(document.querySelectorAll('.seat[data-filled="true"]')).toHaveLength(6),
  );
}

describe("圆桌会诊", () => {
  beforeEach(() => {
    window.history.replaceState(null, "", "#/council");
  });

  it("发起后六席入座并给出收敛裁决", async () => {
    await runCouncil();
    expect(await screen.findByText("收敛裁决")).toBeInTheDocument();
    expect(screen.getByText(/六席的共识是/)).toBeInTheDocument();
    const divergences = screen.getByRole("list", { name: "主要分歧" });
    expect(within(divergences).getAllByRole("listitem")).toHaveLength(2);
  });

  it("空议题时给出提示且不选角", async () => {
    renderRealm();
    await userEvent.click(screen.getByRole("button", { name: "发起会诊" }));
    expect(await screen.findByText("先写下一个要判断的问题")).toBeInTheDocument();
    expect(document.querySelectorAll('.seat[data-filled="true"]')).toHaveLength(0);
  });

  it("锁定的席位在换批后仍留在名单里", async () => {
    await runCouncil();
    const firstPin = screen.getAllByRole("button", { name: "锁定" })[0]!;
    await userEvent.click(firstPin);
    expect(firstPin).toHaveAttribute("aria-pressed", "true");

    await userEvent.click(screen.getByRole("button", { name: "换一批" }));
    await waitFor(() =>
      expect(screen.getAllByRole("button", { name: "已锁定" }).length).toBeGreaterThan(0),
    );
  });

  it("候选池展示三项评分", async () => {
    renderRealm();
    expect(await screen.findByText(/候选池 · 6 位大师/)).toBeInTheDocument();
    expect(screen.getAllByText(/相关 0\.\d\d · 对立 0\.\d\d · 距离 0\.\d\d/)).toHaveLength(6);
  });

  it("会诊后给出分歧曲线与等效数据表", async () => {
    await runCouncil();
    expect(await screen.findByRole("img", { name: /分歧曲线/ })).toBeInTheDocument();
    expect(screen.getByText(/分歧度由 0\.69 走到 0\.56/)).toBeInTheDocument();
    const table = screen.getByRole("table", { name: "各质询轮的相似度与分歧度" });
    expect(table.querySelectorAll("tbody tr")).toHaveLength(2);
    expect(table).toHaveTextContent("0.69");
  });

  it("会诊后按席位展示发言、轮次与表格等效视图", async () => {
    await runCouncil();
    expect(await screen.findByText("逐席发言")).toBeInTheDocument();

    const speech = document.querySelector(".speech") as HTMLElement;
    expect(within(speech).getAllByText(/第 1 轮 · 独立作答/).length).toBeGreaterThan(0);

    const table = within(speech).getByRole("table", { name: "逐席发言记录" });
    expect(table.querySelectorAll("tbody tr").length).toBeGreaterThan(0);
    expect(within(speech).getByText("按轮次分组的发言记录")).toBeInTheDocument();
  });

  it("失败席位可单轮重试并转为已作答", async () => {
    await runCouncil();
    const speech = (await screen.findByText("逐席发言")).closest(".speech") as HTMLElement;
    const seat = speech.querySelector('.speech__seat[data-layer="qi"]') as HTMLElement;
    expect(within(seat).getByText("有失败")).toBeInTheDocument();

    await userEvent.click(within(seat).getByRole("button", { name: "重试该轮" }));

    await waitFor(() => expect(within(seat).getByText("已作答")).toBeInTheDocument());
  });

  it("结论详情页给出六段结构", async () => {
    await runCouncil();
    await userEvent.click(await screen.findByRole("button", { name: "查看结论详情" }));

    expect(await screen.findByText("一 · 结论要点")).toBeInTheDocument();
    expect(screen.getByText("二 · 分歧与未决")).toBeInTheDocument();
    expect(screen.getByText("三 · 收敛过程")).toBeInTheDocument();
    expect(screen.getByText("四 · 逐席依据")).toBeInTheDocument();
    expect(screen.getByText("五 · 外部来源")).toBeInTheDocument();
    expect(screen.getByText("六 · 演化链与追问")).toBeInTheDocument();
    expect(screen.getByText(/半年前的结论/)).toBeInTheDocument();
  });

  it("可从结论发起追问并返回母会话", async () => {
    await runCouncil();
    await userEvent.click(await screen.findByRole("button", { name: "查看结论详情" }));

    const asks = await screen.findAllByRole("button", { name: "追问" });
    await userEvent.click(asks[0]!);
    await userEvent.type(screen.getByLabelText("追问"), "如果只能验证一件事，该验证什么");
    await userEvent.click(screen.getByRole("button", { name: "发起追问" }));

    expect(await screen.findByText(/这是一场追问会话/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "返回母会话" })).toBeInTheDocument();
  });

  it("会诊后标注共享背景与逐席补充检索", async () => {
    await runCouncil();
    await screen.findByText("收敛裁决");

    const external = (await screen.findByText("外部资料")).closest(".external") as HTMLElement;
    expect(
      within(external).getByText("独立开发者的现金流与心理承受力调查"),
    ).toBeInTheDocument();
    expect(within(external).getByText("补充检索 1 条")).toBeInTheDocument();

    const speech = document.querySelector(".speech") as HTMLElement;
    expect(within(speech).getByText("补充检索 1 条")).toBeInTheDocument();
  });

  it("手动检索先给预演确认，确认后才发出请求", async () => {
    await runCouncil();
    await screen.findByText("收敛裁决");

    await userEvent.type(screen.getByLabelText("手动检索"), "要不要换一条赛道");
    await userEvent.click(screen.getByRole("button", { name: "检索" }));

    // 首次调用只拿到待发送内容与指纹，不应出现任何命中结果。
    const preflight = await screen.findByRole("group", { name: "发送前确认" });
    expect(within(preflight).getByText("实际发送")).toBeInTheDocument();
    expect(within(preflight).getByText(/关键词模式/)).toBeInTheDocument();
    expect(document.querySelector(".manual-search__hit")).toBeNull();

    await userEvent.click(within(preflight).getByRole("button", { name: "确认发送" }));

    expect(await screen.findByText(/本次获得 \d+ 条外部结果/)).toBeInTheDocument();
    expect(document.querySelectorAll(".manual-search__hit").length).toBeGreaterThan(0);
  });

  it("分歧曲线标注判定方式与回退轮次", async () => {
    await runCouncil();

    const table = await screen.findByRole("table", { name: "各质询轮的相似度与分歧度" });
    expect(within(table).getByText("判定方式")).toBeInTheDocument();
    expect(within(table).getByText("混合判定")).toBeInTheDocument();
    expect(within(table).getByText("词面判定（回退）")).toBeInTheDocument();
    expect(screen.getByText(/第 3 轮为词面判定/)).toBeInTheDocument();
  });

  it("结论详情标注可疑来源与提示词版本", async () => {
    await runCouncil();
    await userEvent.click(await screen.findByRole("button", { name: "查看结论详情" }));

    expect(await screen.findByText("可疑指令，仅作资料")).toBeInTheDocument();
    expect(screen.getByText(/提示词版本 2026-09-15\.1/)).toBeInTheDocument();
  });

  it("启动时提示未完成的会诊并可继续", async () => {
    renderRealm();
    const heading = await screen.findByText("未完成的会诊");
    const banner = heading.closest(".council__recover") as HTMLElement;
    expect(within(banner).getByText(/副业先做成能自动运转的小系统/)).toBeInTheDocument();

    await userEvent.click(within(banner).getByRole("button", { name: "继续" }));

    expect(await screen.findByText("收敛裁决")).toBeInTheDocument();
    expect(screen.queryByText("未完成的会诊")).toBeNull();
  });

  it("会诊后提示与既有原则重合的回音", async () => {
    await runCouncil();
    await screen.findByText("收敛裁决");

    expect(await screen.findByText("回音提示")).toBeInTheDocument();
    expect(screen.getByText(/不要用时间换钱/)).toBeInTheDocument();
  });

  it("勾选这一场不带我后关闭自我席位并给出说明", async () => {
    renderRealm();
    await userEvent.type(screen.getByLabelText("议题"), "要不要换一条赛道");
    await userEvent.click(screen.getByLabelText("这一场不带我"));
    await userEvent.click(screen.getByRole("button", { name: "发起会诊" }));

    expect(
      await screen.findByText("这一场按你的要求没有带你自己的席位。"),
    ).toBeInTheDocument();
  });
});
