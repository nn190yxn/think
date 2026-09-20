import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  ROSTER_CAPACITY,
  SeatPicker,
  deepestLayer,
  sortRoster,
} from "./SeatPicker";
import { DEMO_SUMMARIES } from "../ipc/demoData";

function renderPicker(
  overrides: Partial<Parameters<typeof SeatPicker>[0]> = {},
) {
  const onConfirm = vi.fn();
  const onClose = vi.fn();
  render(
    <SeatPicker
      layer="shu"
      question="要不要换一条赛道"
      roster={DEMO_SUMMARIES}
      pinned={[]}
      onConfirm={onConfirm}
      onClose={onClose}
      {...overrides}
    />,
  );
  return { onConfirm, onClose };
}

describe("点将排序与落座", () => {
  it("同一题有料的人排在没料的人前面", () => {
    const ordered = sortRoster(DEMO_SUMMARIES, "shu", "");
    expect(ordered[0]?.id).toBe("steve-jobs");
    expect(ordered.slice(1).every((master) => master.id !== "steve-jobs")).toBe(
      true,
    );
  });

  it("没有单元时按六题顺序回退声明层，与内核 primary_layer 一致", () => {
    const base = DEMO_SUMMARIES[0]!;
    // 声明层顺序故意反着写：内核取「道法术气器势」里最靠前的，不取数组第一个。
    const bare = { ...base, layers: ["tool", "fa"], layerProfile: [] } as typeof base;
    expect(deepestLayer(bare, new Set())).toBe("fa");
  });

  it("深浅并列时按道法术气器势取前一题", () => {
    const inamori = DEMO_SUMMARIES.find((master) => master.id === "okada-kazuo")!;
    expect(deepestLayer(inamori, new Set())).toBe("dao");
  });
});

describe("点将面板", () => {
  it("打开后面板标题为本席题意", () => {
    renderPicker();
    const dialog = screen.getByRole("dialog", { name: "选角" });
    expect(within(dialog).getByText("术 · 具体怎么做")).toBeInTheDocument();
  });

  it("分档排序把本席有料的人放在第一张卡片", () => {
    renderPicker();
    const cards = screen
      .getAllByRole("button")
      .filter((button) => button.classList.contains("picker__card"));
    expect(cards[0]).toHaveAttribute("data-master", "steve-jobs");
    expect(cards[0]).toHaveAttribute("data-tier", "ready");
  });

  it("空位不可点并提示获取途径", () => {
    renderPicker();
    const vacancies = screen.getAllByText(/还没招募/);
    expect(vacancies).toHaveLength(ROSTER_CAPACITY - DEMO_SUMMARIES.length);
    for (const vacancy of vacancies) {
      expect(vacancy.closest("button")).toBeNull();
    }
  });

  it("Esc 关闭面板", async () => {
    const { onClose } = renderPicker();
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });

  it("卡片可聚焦，确认入席把选中的人交出去", async () => {
    const { onConfirm } = renderPicker();
    const jobs = screen.getByRole("button", { name: /史蒂夫·乔布斯/ });
    jobs.focus();
    expect(jobs).toHaveFocus();
    await userEvent.click(jobs);
    await userEvent.click(screen.getByRole("button", { name: "确认入席" }));
    expect(onConfirm).toHaveBeenCalledWith("steve-jobs");
  });

  it("列表等效视图用数字呈现六题积累", async () => {
    renderPicker();
    await userEvent.click(screen.getByRole("button", { name: "列表" }));
    const table = screen.getByRole("table", { name: "六题积累" });
    expect(table.querySelectorAll("tbody tr")).toHaveLength(DEMO_SUMMARIES.length);
    expect(within(table).getByText("史蒂夫·乔布斯")).toBeInTheDocument();
    expect(table).toHaveTextContent("术");
  });
});
