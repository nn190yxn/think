import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { CouncilRealm } from "./CouncilRealm";
import { IpcProvider } from "../app/ipc";
import { createCommandClient, stubTransport } from "../ipc/client";
import { demoSelection } from "../ipc/demoData";

function renderRealm() {
  return render(
    <IpcProvider>
      <CouncilRealm />
    </IpcProvider>,
  );
}

/** 库里没有谈「势」的大师时，选角会在已经有人站上的「道」再补一位。 */
function duplicatedSelection() {
  const base = demoSelection("steady");
  const dao = base.seats.find((seat) => seat.layer === "dao")!;
  return {
    ...base,
    seats: [
      ...base.seats.filter((seat) => seat.layer !== "shi"),
      {
        ...dao,
        masterId: "viktor-frankl",
        name: "维克多·弗兰克尔",
        domain: "意义",
        layers: ["qi", "dao"],
        score: 0.58,
      },
    ],
    gaps: ["shi"],
  };
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

  it("发起后六席入座并给出本次结论", async () => {
    await runCouncil();
    expect(await screen.findByText("本次结论")).toBeInTheDocument();
    expect(screen.getByText(/六席的共识是/)).toBeInTheDocument();
    const divergences = screen.getByRole("list", { name: "主要分歧" });
    expect(within(divergences).getAllByRole("listitem")).toHaveLength(2);
  });

  it("六个座位各代表一题，入座后显示该题的核心问题", async () => {
    await runCouncil();
    const questions = Array.from(document.querySelectorAll(".seat")).map(
      (seat) => seat.querySelector(".seat__question")?.textContent,
    );
    expect(questions).toEqual([
      "什么值得做",
      "规律是什么",
      "具体怎么做",
      "靠什么心力度过",
      "用什么载体放大",
      "现在是不是时候",
    ]);
  });

  it("同一题站上两位时，两位都在圆桌上露面并可分别锁定", async () => {
    const selection = duplicatedSelection();
    const client = createCommandClient({
      invoke: async (name, request) => {
        if (name === "council_select" || name === "council_rotate") {
          return { ok: true, data: selection };
        }
        return stubTransport.invoke(name, request);
      },
    });
    render(
      <IpcProvider client={client}>
        <CouncilRealm />
      </IpcProvider>,
    );
    await userEvent.type(screen.getByLabelText("议题"), "要不要换一条赛道");
    await userEvent.click(screen.getByRole("button", { name: "发起会诊" }));

    const dao = await waitFor(() => {
      const seat = document.querySelector('.seat[data-layer="dao"]') as HTMLElement;
      expect(seat.querySelectorAll(".seat__person")).toHaveLength(2);
      return seat;
    });
    expect(within(dao).getByText("同一题上有 2 位")).toBeInTheDocument();
    expect(within(dao).getByText("稻盛和夫")).toBeInTheDocument();
    expect(within(dao).getByText("维克多·弗兰克尔")).toBeInTheDocument();
    expect(within(dao).getAllByRole("button", { name: "锁定" })).toHaveLength(2);
    // 没人谈的「势」仍然是缺口题，也仍然只占一个空位。
    expect(document.querySelector('.seat[data-layer="shi"]')).toHaveTextContent(
      "这一题还要再谈",
    );
  });

  it("空议题时给出提示且不选角", async () => {
    renderRealm();
    await userEvent.click(screen.getByRole("button", { name: "发起会诊" }));
    expect(await screen.findByText("先写下一个要判断的问题")).toBeInTheDocument();
    expect(document.querySelectorAll('.seat[data-filled="true"]')).toHaveLength(0);
  });

  it("还要再谈的题单独列出，并在对应的席位上标注", async () => {
    await runCouncil();

    const gaps = (await screen.findByRole("region", { name: "还要再谈的题" })) as HTMLElement;
    expect(within(gaps).getByText("还要再谈的题 · 2 道")).toBeInTheDocument();
    expect(within(gaps).getByText("术 · 具体怎么做")).toBeInTheDocument();
    expect(within(gaps).getByText("势 · 现在是不是时候")).toBeInTheDocument();
    expect(within(gaps).getByText(/换一批时会优先给这几道题补人/)).toBeInTheDocument();

    // 圆桌上对应的席位也要标出来，颜色之外还有文字标记。
    const marked = Array.from(document.querySelectorAll('.seat[data-gap="true"]'));
    expect(marked).toHaveLength(2);
    for (const seat of marked) {
      expect(seat).toHaveTextContent("这一题还要再谈");
    }
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
    expect(
      screen.getAllByText(/相关度 0\.\d\d · 对立度 0\.\d\d · 领域跨度 0\.\d\d/),
    ).toHaveLength(6);
  });

  it("会诊后给出分歧曲线与等效数据表", async () => {
    await runCouncil();
    expect(await screen.findByRole("img", { name: /分歧变化图/ })).toBeInTheDocument();
    expect(screen.getByText(/分歧程度从 0\.69 变到 0\.56/)).toBeInTheDocument();
    const table = screen.getByRole("table", { name: "每一轮的相似程度与分歧程度" });
    expect(table.querySelectorAll("tbody tr")).toHaveLength(2);
    expect(table).toHaveTextContent("0.69");
  });

  it("会诊后按席位展示发言、轮次与表格等效视图", async () => {
    await runCouncil();
    expect(await screen.findByText("逐席发言")).toBeInTheDocument();

    const speech = document.querySelector(".speech") as HTMLElement;
    expect(within(speech).getAllByText(/第 1 轮 · 独立作答/).length).toBeGreaterThan(0);
    // 逐席发言标注该席位负责的题，与圆桌口径一致。
    expect(within(speech).getAllByText(/道 · 什么值得做/).length).toBeGreaterThan(0);

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
    expect(screen.getByText("二 · 还没谈拢的分歧")).toBeInTheDocument();
    expect(screen.getByText("三 · 分歧的变化")).toBeInTheDocument();
    expect(screen.getByText("四 · 各位大师的依据")).toBeInTheDocument();
    expect(screen.getByText("五 · 用到的外部资料")).toBeInTheDocument();
    expect(screen.getByText("六 · 前后几次结论")).toBeInTheDocument();
    expect(screen.getByText(/半年前的结论/)).toBeInTheDocument();
  });

  it("还没谈拢的分歧按题分组，并保留表格等效视图", async () => {
    await runCouncil();
    await userEvent.click(await screen.findByRole("button", { name: "查看结论详情" }));

    const groups = document.querySelector(".conclusion__divergences") as HTMLElement;
    const headings = Array.from(groups.querySelectorAll(".conclusion__divergence-head")).map(
      (node) => node.textContent,
    );
    expect(headings).toHaveLength(2);
    expect(headings[0]).toContain("道 · 什么值得做");
    expect(headings[1]).toContain("气 · 靠什么心力度过");

    const table = screen.getByRole("table", { name: "还没谈拢的分歧" });
    expect(table.querySelectorAll("tbody tr")).toHaveLength(2);
    expect(table).toHaveTextContent("气 · 靠什么心力度过");
  });

  it("每题立场与上一轮相比的变化带题标注与变化说明", async () => {
    await runCouncil();
    await userEvent.click(await screen.findByRole("button", { name: "查看结论详情" }));

    const block = (await screen.findByText("每题与上一轮相比")).closest(
      ".conclusion__stances",
    ) as HTMLElement;
    const items = Array.from(block.querySelectorAll(".conclusion__stance"));
    expect(items).toHaveLength(4);
    expect(items[0]).toHaveTextContent("道 · 什么值得做");
    expect(items[0]).toHaveTextContent("转向");
    expect(items[0]).toHaveTextContent("上一轮");
    expect(items[3]).toHaveTextContent("势 · 现在是不是时候");
    expect(items[3]).toHaveTextContent("新谈");
    // 颜色之外必须有文字标签，不靠颜色单通道表达变化。
    for (const item of items) {
      expect(item.querySelector(".conclusion__stance-change")?.textContent).toBeTruthy();
    }
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
    await screen.findByText("本次结论");

    const external = (await screen.findByText("外部资料")).closest(".external") as HTMLElement;
    expect(
      within(external).getByText("独立开发者的现金流与心理承受力调查"),
    ).toBeInTheDocument();
    expect(within(external).getByText("另外查了 1 条资料")).toBeInTheDocument();

    const speech = document.querySelector(".speech") as HTMLElement;
    expect(within(speech).getByText("另外查了 1 条资料")).toBeInTheDocument();
  });

  it("手动检索先给预演确认，确认后才发出请求", async () => {
    await runCouncil();
    await screen.findByText("本次结论");

    await userEvent.type(screen.getByLabelText("主动查资料"), "要不要换一条赛道");
    await userEvent.click(screen.getByRole("button", { name: "查一查" }));

    // 首次调用只拿到待发送内容与指纹，不应出现任何命中结果。
    const preflight = await screen.findByRole("group", { name: "发送前确认" });
    expect(within(preflight).getByText("实际发送")).toBeInTheDocument();
    expect(within(preflight).getByText(/关键词模式/)).toBeInTheDocument();
    expect(document.querySelector(".manual-search__hit")).toBeNull();

    await userEvent.click(within(preflight).getByRole("button", { name: "确认发送" }));

    expect(await screen.findByText(/这次查到 \d+ 条资料/)).toBeInTheDocument();
    expect(document.querySelectorAll(".manual-search__hit").length).toBeGreaterThan(0);
  });

  it("分歧曲线标注判定方式与回退轮次", async () => {
    await runCouncil();

    const table = await screen.findByRole(
      "table",
      { name: "每一轮的相似程度与分歧程度" },
    );
    expect(within(table).getByText("判定方式")).toBeInTheDocument();
    expect(within(table).getByText("按用词与观点方向共同判断")).toBeInTheDocument();
    expect(within(table).getByText("按用词重合度判断（回退）")).toBeInTheDocument();
    expect(screen.getByText(/第 3 轮只能按用词判断/)).toBeInTheDocument();
  });

  it("结论详情标注可疑来源与提示词版本", async () => {
    await runCouncil();
    await userEvent.click(await screen.findByRole("button", { name: "查看结论详情" }));

    expect(await screen.findByText("可疑指令，仅作资料")).toBeInTheDocument();
    expect(screen.getByText(/提问模板 2026-09-17\.1/)).toBeInTheDocument();
  });

  it("启动时提示未完成的会诊并可继续", async () => {
    renderRealm();
    const heading = await screen.findByText("未完成的会诊");
    const banner = heading.closest(".council__recover") as HTMLElement;
    expect(within(banner).getByText(/副业先做成能自动运转的小系统/)).toBeInTheDocument();

    await userEvent.click(within(banner).getByRole("button", { name: "继续" }));

    expect(await screen.findByText("本次结论")).toBeInTheDocument();
    expect(screen.queryByText("未完成的会诊")).toBeNull();
  });

  it("会诊后提示与既有原则重合的回音", async () => {
    await runCouncil();
    await screen.findByText("本次结论");

    expect(await screen.findByText("和既有原则重合的地方")).toBeInTheDocument();
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

  it("待选角可打开点将面板，确认后座位留痕并可取消", async () => {
    renderRealm();
    const empty = await screen.findAllByRole("button", { name: "待选角" });
    await userEvent.click(empty[0]!);
    const dialog = await screen.findByRole("dialog", { name: "选角" });
    expect(within(dialog).getByText("道 · 什么值得做")).toBeInTheDocument();
    await userEvent.click(await within(dialog).findByRole("button", { name: /稻盛和夫/ }));
    await userEvent.click(within(dialog).getByRole("button", { name: "确认入席" }));
    expect(screen.queryByRole("dialog", { name: "选角" })).toBeNull();
    expect(await screen.findByText("已点 · 稻盛和夫")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.queryByText("已点 · 稻盛和夫")).toBeNull();
    expect(screen.getAllByRole("button", { name: "待选角" }).length).toBeGreaterThan(0);
  });

  it("会诊进行中点将不可点，并说明下次生效", async () => {
    const client = createCommandClient({
      invoke: async (name, request) => {
        if (name === "council_run") {
          await new Promise(() => undefined);
        }
        return stubTransport.invoke(name, request);
      },
    });
    render(
      <IpcProvider client={client}>
        <CouncilRealm />
      </IpcProvider>,
    );
    await userEvent.type(screen.getByLabelText("议题"), "要不要换一条赛道");
    await userEvent.click(screen.getByRole("button", { name: "发起会诊" }));
    await waitFor(() =>
      expect(document.querySelectorAll('.seat[data-filled="true"]')).toHaveLength(6),
    );
    expect(screen.getByText("本轮已经开始，换人下次生效")).toBeInTheDocument();
    const swaps = screen.getAllByRole("button", { name: "换人" });
    expect(swaps.length).toBeGreaterThan(0);
    for (const button of swaps) {
      expect(button).toBeDisabled();
    }
  });
});
